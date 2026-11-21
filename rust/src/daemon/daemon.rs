use super::failover::evaluate_and_execute_failover;
use crate::balancer::evaluate_and_execute_balancer;
use crate::quota::AccountQuotaResult;
use crate::storage::StorageManager;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn default_token_history_file() -> PathBuf {
    dirs::home_dir().map_or_else(
        || PathBuf::from(".gemini/antigravity/token_history.jsonl"),
        |h| h.join(".gemini/antigravity/token_history.jsonl"),
    )
}

pub fn record_token_history_snapshot(
    analyzed_quotas: &HashMap<String, AccountQuotaResult>,
    history_file: Option<&Path>,
) -> std::io::Result<()> {
    let def_path = default_token_history_file();
    let path = history_file.unwrap_or(&def_path);

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
    let mut accounts_map = serde_json::Map::new();

    for (email, q) in analyzed_quotas {
        let mut groups_map = serde_json::Map::new();
        for g in &q.groups {
            let mut g_vals = serde_json::Map::new();
            if let Some(w) = &g.weekly {
                if let Some(pct) = w.remaining_percent {
                    g_vals.insert("weekly_pct".into(), serde_json::json!(pct));
                }
                if let Some(reset) = &w.reset_time {
                    g_vals.insert("weekly_reset".into(), serde_json::json!(reset));
                }
            }
            if let Some(f) = &g.five_hour
                && let Some(pct) = f.remaining_percent
            {
                g_vals.insert("5h_pct".into(), serde_json::json!(pct));
            }
            groups_map.insert(g.display_name.clone(), serde_json::Value::Object(g_vals));
        }

        let mut acc_obj = serde_json::Map::new();
        acc_obj.insert("groups".into(), serde_json::Value::Object(groups_map));
        accounts_map.insert(email.clone(), serde_json::Value::Object(acc_obj));
    }

    let line_data = serde_json::json!({
        "timestamp": now,
        "accounts": accounts_map
    });

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    writeln!(file, "{}", serde_json::to_string(&line_data)?)?;
    Ok(())
}

pub async fn run_daemon_step(
    storage: &StorageManager,
    history_file: Option<&Path>,
) -> (bool, bool) {
    // 1. Geo-Shield check
    let config_dir = crate::storage::default_config_dir();
    let _ = crate::shield::evaluate_and_enforce_shield(&config_dir, None).await;

    // 2. Sync from Antigravity
    let _ = crate::auth::sync_from_antigravity(storage).await;

    // 3. Fetch quotas
    let all_quotas = crate::quota::fetch_all_accounts_quota(false).await;

    // 4. Record history snapshot
    let _ = record_token_history_snapshot(&all_quotas, history_file);

    // 5. Evaluate auto-balancer
    let balancer_res = evaluate_and_execute_balancer(&all_quotas, false, false, None).await;

    // 6. Evaluate emergency failover if active exhausted
    let failover_res = evaluate_and_execute_failover(&all_quotas, 1.0, true, 120.0, false).await;

    (balancer_res.triggered, failover_res.triggered)
}
