use super::burnrate::default_history_file;
pub use super::models::{AccountQuotaResult, QuotaRefreshPolicy};
use crate::auth::{
    discover_antigravity_desktop_app, discover_antigravity_ide, ensure_valid_token,
    fetch_quota_from_desktop_app,
};
use crate::storage::{
    AccountProfile, ModelQuota, QuotaBucket, QuotaGroup, QuotaSnapshot, load_accounts,
    save_accounts, upsert_account,
};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

pub const QUOTA_SUMMARY_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary";
pub const QUOTA_MODELS_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";
pub const CACHE_TTL_SECONDS: f64 = 600.0;

static QUOTA_CACHE: LazyLock<Mutex<HashMap<String, (f64, AccountQuotaResult)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn clear_quota_cache() {
    QUOTA_CACHE
        .lock()
        .expect("quota cache lock poisoned")
        .clear();
}

pub fn historical_quota(email: &str) -> Option<QuotaSnapshot> {
    let history_file = default_history_file();
    if !history_file.is_file() {
        return None;
    }

    let email_clean = email.trim().to_lowercase();
    let mut last_record: Option<Value> = None;
    let mut last_timestamp: Option<f64> = None;

    if let Ok(file) = File::open(&history_file) {
        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(data) = serde_json::from_str::<Value>(trimmed)
                && let Some(accs) = data.get("accounts").and_then(|a| a.as_object())
                && let Some(acc_val) = accs.get(&email_clean)
                && let Some(g_data) = acc_val.get("groups").and_then(|g| g.as_object())
            {
                let has_weekly = g_data
                    .values()
                    .any(|v| v.get("weekly_pct").is_some_and(|p| !p.is_null()));
                if has_weekly {
                    last_record = Some(acc_val.clone());
                    last_timestamp = data.get("timestamp").and_then(serde_json::Value::as_f64);
                }
            }
        }
    }

    let record = last_record?;
    let groups_obj = record.get("groups")?.as_object()?;
    let mut parsed_groups = Vec::new();

    for (g_name, g_vals) in groups_obj {
        let w_pct = g_vals.get("weekly_pct").and_then(serde_json::Value::as_f64);
        let w_reset = g_vals
            .get("weekly_reset")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let f_pct = g_vals.get("5h_pct").and_then(serde_json::Value::as_f64);

        let weekly_bucket = w_pct.map(|pct| QuotaBucket {
            bucket_id: Some(format!(
                "{}-weekly",
                g_name.to_lowercase().replace(' ', "-")
            )),
            window: Some("weekly".to_string()),
            display_name: Some(g_name.clone()),
            remaining_fraction: Some((pct / 100.0 * 10000.0).round() / 10000.0),
            remaining_percent: Some((pct * 100.0).round() / 100.0),
            reset_time: Some(w_reset.to_string()),
            description: Some(String::new()),
            ..Default::default()
        });

        let five_hour_bucket = f_pct.map(|pct| QuotaBucket {
            bucket_id: Some(format!("{}-5h", g_name.to_lowercase().replace(' ', "-"))),
            window: Some("5h".to_string()),
            display_name: Some(g_name.clone()),
            remaining_fraction: Some((pct / 100.0 * 10000.0).round() / 10000.0),
            remaining_percent: Some((pct * 100.0).round() / 100.0),
            reset_time: Some(String::new()),
            description: Some(String::new()),
            ..Default::default()
        });

        parsed_groups.push(QuotaGroup {
            display_name: g_name.clone(),
            description: Some(String::new()),
            weekly: weekly_bucket,
            five_hour: five_hour_bucket,
        });
    }

    let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
    Some(QuotaSnapshot {
        checked_at: Some(last_timestamp.unwrap_or(now)),
        groups: parsed_groups,
        models: Vec::new(),
        is_cached: Some(true),
        is_gemini_exhausted: None,
        is_3p_exhausted: None,
    })
}

#[inline]
pub fn get_historical_quota(email: &str) -> Option<QuotaSnapshot> {
    historical_quota(email)
}

pub fn parse_quota_summary(quota_summary_data: &Value) -> Vec<QuotaGroup> {
    let mut parsed_groups = Vec::new();
    let raw_groups = quota_summary_data
        .get("groups")
        .and_then(|g| g.as_array())
        .cloned()
        .unwrap_or_default();

    for rg in raw_groups {
        let group_name = rg
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or("Unnamed Group")
            .to_string();
        let group_desc = rg
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let buckets = rg
            .get("buckets")
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();

        let mut weekly_bucket = None;
        let mut five_hour_bucket = None;

        for b in buckets {
            let bucket_id = b
                .get("bucketId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let window = b
                .get("window")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let remaining_fraction = b
                .get("remainingFraction")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let remaining_percent = ((remaining_fraction * 100.0) * 100.0).round() / 100.0;
            let reset_time = b
                .get("resetTime")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let desc = b
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let display_name = b
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(&bucket_id)
                .to_string();

            let bucket_info = QuotaBucket {
                bucket_id: Some(bucket_id.clone()),
                window: Some(window.clone()),
                display_name: Some(display_name),
                remaining_fraction: Some(remaining_fraction),
                remaining_percent: Some(remaining_percent),
                reset_time: Some(reset_time),
                description: Some(desc),
                ..Default::default()
            };

            if bucket_id.contains("weekly") || window == "weekly" {
                weekly_bucket = Some(bucket_info);
            } else if bucket_id.contains("5h") || window == "5h" {
                five_hour_bucket = Some(bucket_info);
            }
        }

        parsed_groups.push(QuotaGroup {
            display_name: group_name,
            description: Some(group_desc),
            weekly: weekly_bucket,
            five_hour: five_hour_bucket,
        });
    }

    parsed_groups
}

pub fn parse_models_quota(models_data: &Value) -> Vec<ModelQuota> {
    let mut parsed_models = Vec::new();
    if let Some(buckets) = models_data.get("buckets").and_then(|b| b.as_array()) {
        for mb in buckets {
            let model_id = match mb.get("modelId").and_then(|v| v.as_str()) {
                Some(id) if !id.is_empty() => id.to_string(),
                _ => continue,
            };
            let frac = mb
                .get("remainingFraction")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let pct = ((frac * 100.0) * 100.0).round() / 100.0;
            let reset = mb
                .get("resetTime")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            parsed_models.push(ModelQuota {
                model_id,
                remaining_fraction: Some(frac),
                remaining_percent: Some(pct),
                reset_time: Some(reset),
            });
        }
    }
    parsed_models
}

pub async fn fetch_account_quota(
    account: &mut AccountProfile,
    policy: impl Into<QuotaRefreshPolicy>,
) -> AccountQuotaResult {
    let force_refresh = policy.into().is_force_refresh();
    let email = account.email.trim().to_lowercase();
    let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;

    if !force_refresh {
        let cache = QUOTA_CACHE.lock().expect("quota cache lock poisoned");
        if let Some((cached_time, cached_data)) = cache.get(&email)
            && now - cached_time < CACHE_TTL_SECONDS
        {
            return cached_data.clone();
        }
    }

    let mut quota_summary_data: Option<Value> = None;
    let mut models_data: Option<Value> = None;

    // Option A: Active Desktop App
    let desktop_session = discover_antigravity_desktop_app().await;
    if let Some(ds) = &desktop_session
        && ds.email.trim().eq_ignore_ascii_case(&email)
        && let (Some(port), Some(csrf)) = (ds.port, &ds.csrf_token)
    {
        quota_summary_data = fetch_quota_from_desktop_app(port, csrf).await;
    }

    // Option A2: Active IDE
    if quota_summary_data.is_none() {
        let ide_session = discover_antigravity_ide().await;
        if let Some(is) = &ide_session
            && is.email.trim().eq_ignore_ascii_case(&email)
            && let (Some(port), Some(csrf)) = (is.port, &is.csrf_token)
        {
            quota_summary_data = fetch_quota_from_desktop_app(port, csrf).await;
        }
    }

    // Option B: Query Google CloudCode API using OAuth token
    if quota_summary_data.is_none()
        && (account.access_token.is_some() || account.refresh_token.is_some())
    {
        let (token_opt, updated_acc) = ensure_valid_token(account.clone()).await;
        if let Some(token) = token_opt {
            if updated_acc != *account {
                *account = updated_acc.clone();
                let _ = upsert_account(&updated_acc);
            }

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(12))
                .user_agent("antigravity/2.16.0")
                .build();

            if let Ok(client) = client {
                let summary_res = client
                    .post(QUOTA_SUMMARY_URL)
                    .bearer_auth(&token)
                    .header("Content-Type", "application/json")
                    .body("{}")
                    .send()
                    .await;

                if let Ok(resp) = summary_res
                    && resp.status().is_success()
                {
                    quota_summary_data = resp.json::<Value>().await.ok();
                }

                let models_res = client
                    .post(QUOTA_MODELS_URL)
                    .bearer_auth(&token)
                    .header("Content-Type", "application/json")
                    .body("{}")
                    .send()
                    .await;

                if let Ok(resp) = models_res
                    && resp.status().is_success()
                {
                    models_data = resp.json::<Value>().await.ok();
                }
            }
        }
    }

    let Some(summary_val) = quota_summary_data else {
        // Fallback to persistent last-known quota if available
        let mut last_known = account.last_known_quota.clone();
        if last_known.as_ref().is_none_or(|k| k.groups.is_empty())
            && let Some(hist) = historical_quota(&email)
        {
            account.last_known_quota = Some(hist.clone());
            let _ = upsert_account(account);
            last_known = Some(hist);
        }

        if let Some(lk) = last_known
            && !lk.groups.is_empty()
        {
            let result = AccountQuotaResult {
                email: email.clone(),
                name: account.name.clone().unwrap_or_default(),
                tier: account.tier.clone().unwrap_or_else(|| "Standard".into()),
                is_current_ide_session: account.is_current_ide_session.unwrap_or(false),
                is_current_desktop_session: account.is_current_desktop_session.unwrap_or(false),
                groups: lk.groups,
                models: lk.models,
                checked_at: lk.checked_at.unwrap_or(now),
                is_cached: true,
                error: None,
            };
            QUOTA_CACHE
                .lock()
                .expect("quota cache lock poisoned")
                .insert(email, (now, result.clone()));
            return result;
        }

        return AccountQuotaResult {
            email: email.clone(),
            name: account.name.clone().unwrap_or_default(),
            tier: account.tier.clone().unwrap_or_else(|| "Standard".into()),
            is_current_ide_session: account.is_current_ide_session.unwrap_or(false),
            is_current_desktop_session: account.is_current_desktop_session.unwrap_or(false),
            groups: Vec::new(),
            models: Vec::new(),
            checked_at: now,
            is_cached: false,
            error: Some("Account session inactive. Log into Antigravity to refresh quota.".into()),
        };
    };

    let parsed_groups = parse_quota_summary(&summary_val);
    let parsed_models = models_data
        .as_ref()
        .map(parse_models_quota)
        .unwrap_or_default();

    let result = AccountQuotaResult {
        email: email.clone(),
        name: account.name.clone().unwrap_or_default(),
        tier: account.tier.clone().unwrap_or_else(|| "Standard".into()),
        is_current_ide_session: account.is_current_ide_session.unwrap_or(false),
        is_current_desktop_session: account.is_current_desktop_session.unwrap_or(false),
        groups: parsed_groups.clone(),
        models: parsed_models.clone(),
        checked_at: now,
        is_cached: false,
        error: None,
    };

    account.last_known_quota = Some(QuotaSnapshot {
        checked_at: Some(now),
        groups: parsed_groups,
        models: parsed_models,
        is_cached: Some(false),
        is_gemini_exhausted: None,
        is_3p_exhausted: None,
    });
    let _ = upsert_account(account);

    QUOTA_CACHE
        .lock()
        .expect("quota cache lock poisoned")
        .insert(email, (now, result.clone()));

    result
}

pub async fn fetch_all_accounts_quota(
    policy: impl Into<QuotaRefreshPolicy>,
) -> HashMap<String, AccountQuotaResult> {
    let force_refresh = policy.into().is_force_refresh();
    let mut registry = load_accounts().unwrap_or_default();
    let mut results = HashMap::new();

    for (email, mut acc) in registry.accounts.clone() {
        if !email.is_empty() {
            let res = fetch_account_quota(&mut acc, force_refresh).await;
            registry.accounts.insert(email.clone(), acc);
            results.insert(email, res);
        }
    }

    let _ = save_accounts(&registry);
    results
}
