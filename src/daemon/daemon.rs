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

/// Controls whether automatic failover restarts Antigravity processes after switching accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FailoverRestart {
    /// Restart Antigravity processes after swapping credentials.
    #[default]
    Restart,
    /// Swap credentials on disk without restarting running Antigravity processes.
    NoRestart,
}

impl FailoverRestart {
    /// Returns `true` when Antigravity processes should be restarted on failover.
    #[must_use]
    pub const fn should_restart(self) -> bool {
        matches!(self, Self::Restart)
    }
}

/// Configuration options for [`run_daemon_with_options`] and [`run_daemon_step_with_options`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DaemonRunOptions {
    /// Polling interval in seconds between daemon iterations.
    pub interval_secs: u64,
    /// Whether `--auto-switch` was requested for daemon failover handling.
    pub auto_switch: bool,
    /// Quota percentage threshold at or below which automatic failover triggers.
    pub threshold: f64,
    /// Minimum seconds between consecutive automatic failovers.
    pub cooldown_seconds: f64,
    /// Whether Antigravity processes are restarted during automatic failover.
    pub restart: FailoverRestart,
}

impl Default for DaemonRunOptions {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            auto_switch: false,
            threshold: 1.0,
            cooldown_seconds: 120.0,
            restart: FailoverRestart::Restart,
        }
    }
}

impl DaemonRunOptions {
    /// Creates daemon run options initialized to default polling and failover settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Errors returned by [`run_daemon_with_options`].
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    /// Underlying I/O error from daemon execution.
    #[error("daemon I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Executes a single daemon monitoring iteration using the supplied [`DaemonRunOptions`].
///
/// Returns `(balancer_triggered, failover_triggered)`.
pub async fn run_daemon_step_with_options(
    storage: &StorageManager,
    history_file: Option<&Path>,
    options: &DaemonRunOptions,
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
    let failover_res = evaluate_and_execute_failover(
        &all_quotas,
        options.threshold,
        options.restart.should_restart(),
        options.cooldown_seconds,
        false,
    )
    .await;

    (balancer_res.triggered, failover_res.triggered)
}

pub async fn run_daemon_step(
    storage: &StorageManager,
    history_file: Option<&Path>,
) -> (bool, bool) {
    run_daemon_step_with_options(storage, history_file, &DaemonRunOptions::default()).await
}

/// Runs the periodic daemon monitoring loop with explicit [`DaemonRunOptions`] until `shutdown` signals `true`.
///
/// # Errors
///
/// Returns [`DaemonError`] if a fatal I/O failure occurs while running the daemon loop.
pub async fn run_daemon_with_options(
    options: DaemonRunOptions,
    mut shutdown: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<(), DaemonError> {
    if shutdown.as_ref().is_some_and(|rx| *rx.borrow()) {
        return Ok(());
    }

    let storage = StorageManager::default();
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
        options.interval_secs.max(1),
    ));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let _ = run_daemon_step_with_options(&storage, None, &options).await;
            }
            () = async {
                if let Some(ref mut rx) = shutdown {
                    if *rx.borrow() {
                        return;
                    }
                    let _ = rx.changed().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                break;
            }
        }
    }
    Ok(())
}

pub async fn run_daemon(
    interval_secs: u64,
    auto_switch: bool,
    shutdown: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let options = DaemonRunOptions {
        interval_secs,
        auto_switch,
        ..DaemonRunOptions::default()
    };
    run_daemon_with_options(options, shutdown).await?;
    Ok(())
}
