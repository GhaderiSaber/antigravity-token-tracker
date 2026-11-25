use super::burnrate::default_history_file;
use super::models::{AccountQuotaResult, QuotaRefreshPolicy};
use crate::auth::{
    DiscoveredSession, discover_antigravity_desktop_app, discover_antigravity_ide,
    ensure_valid_token, fetch_quota_from_desktop_app,
};
use crate::storage::{
    AccountProfile, ModelQuota, QuotaBucket, QuotaGroup, QuotaSnapshot, load_accounts,
    save_accounts, upsert_account,
};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use tokio::task::JoinSet;

/// Endpoint returning grouped user quota summaries from Google CloudCode PA.
pub const QUOTA_SUMMARY_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary";

/// Endpoint returning per-model quota buckets from Google CloudCode PA.
pub const QUOTA_MODELS_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";

/// Time-to-live in seconds for in-memory cached quota results.
pub const CACHE_TTL_SECONDS: f64 = 600.0;

const QUOTA_HTTP_TIMEOUT_SECS: u64 = 12;
const QUOTA_USER_AGENT: &str = "antigravity/2.16.0";

static QUOTA_CACHE: LazyLock<Mutex<HashMap<String, (f64, AccountQuotaResult)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Shared session discovery and HTTP client context for fetching account quotas.
#[derive(Debug, Clone, Default)]
pub struct QuotaFetchContext {
    /// Active Antigravity Desktop session discovered for the fetch batch.
    pub desktop_session: Option<DiscoveredSession>,
    /// Active Antigravity IDE session discovered for the fetch batch.
    pub ide_session: Option<DiscoveredSession>,
    /// Shared HTTP client with configured timeout and user-agent.
    pub client: Option<reqwest::Client>,
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<QuotaFetchContext>();
};

impl QuotaFetchContext {
    /// Creates an empty [`QuotaFetchContext`] with no discovered sessions or HTTP client.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Discovers active Desktop and IDE sessions once and builds a shared HTTP client.
    pub async fn discover() -> Self {
        let (desktop_session, ide_session) = tokio::join!(
            discover_antigravity_desktop_app(),
            discover_antigravity_ide()
        );
        Self {
            desktop_session,
            ide_session,
            client: build_quota_http_client(),
        }
    }
}

fn build_quota_http_client() -> Option<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(QUOTA_HTTP_TIMEOUT_SECS))
        .user_agent(QUOTA_USER_AGENT)
        .build()
        .ok()
}

fn current_epoch_seconds() -> f64 {
    chrono::Utc::now().timestamp_millis() as f64 / 1000.0
}

fn cached_quota_result(email: &str, now: f64) -> Option<AccountQuotaResult> {
    let cache = QUOTA_CACHE.lock().expect("quota cache lock poisoned");
    cache
        .get(email)
        .filter(|(cached_time, _)| now - cached_time < CACHE_TTL_SECONDS)
        .map(|(_, cached_data)| cached_data.clone())
}

fn insert_quota_cache(email: String, now: f64, result: &AccountQuotaResult) {
    QUOTA_CACHE
        .lock()
        .expect("quota cache lock poisoned")
        .insert(email, (now, result.clone()));
}

/// Clears all entries from the in-memory quota cache.
pub fn clear_quota_cache() {
    QUOTA_CACHE
        .lock()
        .expect("quota cache lock poisoned")
        .clear();
}

/// Reconstructs the most recent historical quota snapshot for `email` from `history_file`.
#[must_use]
pub fn historical_quota_from_file(email: &str, history_file: &Path) -> Option<QuotaSnapshot> {
    if !history_file.is_file() {
        return None;
    }

    let email_clean = email.trim().to_lowercase();
    let mut last_record: Option<Value> = None;
    let mut last_timestamp: Option<f64> = None;

    if let Ok(file) = File::open(history_file) {
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
                    last_timestamp = data.get("timestamp").and_then(Value::as_f64);
                }
            }
        }
    }

    let record = last_record?;
    let groups_obj = record.get("groups")?.as_object()?;
    let mut parsed_groups = Vec::with_capacity(groups_obj.len());

    for (g_name, g_vals) in groups_obj {
        let w_pct = g_vals.get("weekly_pct").and_then(Value::as_f64);
        let w_reset = g_vals
            .get("weekly_reset")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let f_pct = g_vals.get("5h_pct").and_then(Value::as_f64);

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

    let now = current_epoch_seconds();
    Some(QuotaSnapshot {
        checked_at: Some(last_timestamp.unwrap_or(now)),
        groups: parsed_groups,
        models: Vec::new(),
        is_cached: Some(true),
        is_gemini_exhausted: None,
        is_3p_exhausted: None,
    })
}

/// Reconstructs the most recent historical quota snapshot for `email` from the default history file.
#[must_use]
pub fn historical_quota(email: &str) -> Option<QuotaSnapshot> {
    historical_quota_from_file(email, &default_history_file())
}

/// Compatibility alias for [`historical_quota`].
#[inline]
#[must_use]
pub fn get_historical_quota(email: &str) -> Option<QuotaSnapshot> {
    historical_quota(email)
}

/// Parses grouped weekly and 5-hour quota buckets from a `retrieveUserQuotaSummary` response.
#[must_use]
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
                .and_then(Value::as_f64)
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

/// Parses individual model quota buckets from a `retrieveUserQuota` response.
#[must_use]
pub fn parse_models_quota(models_data: &Value) -> Vec<ModelQuota> {
    let mut parsed_models = Vec::new();
    if let Some(buckets) = models_data.get("buckets").and_then(|b| b.as_array()) {
        for mb in buckets {
            let Some(id) = mb.get("modelId").and_then(|v| v.as_str()) else {
                continue;
            };
            if id.is_empty() {
                continue;
            }
            let model_id = id.to_string();
            let frac = mb
                .get("remainingFraction")
                .and_then(Value::as_f64)
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

enum QuotaSource {
    Live,
    Cached,
}

struct QuotaResultPayload {
    groups: Vec<QuotaGroup>,
    models: Vec<ModelQuota>,
    checked_at: f64,
    source: QuotaSource,
    error: Option<String>,
}

fn build_quota_result(
    account: &AccountProfile,
    email: &str,
    payload: QuotaResultPayload,
) -> AccountQuotaResult {
    AccountQuotaResult {
        email: email.to_string(),
        name: account.name.clone().unwrap_or_default(),
        tier: account.tier.clone().unwrap_or_else(|| "Standard".into()),
        is_current_ide_session: account.is_current_ide_session.unwrap_or(false),
        is_current_desktop_session: account.is_current_desktop_session.unwrap_or(false),
        groups: payload.groups,
        models: payload.models,
        checked_at: payload.checked_at,
        is_cached: matches!(payload.source, QuotaSource::Cached),
        error: payload.error,
    }
}

async fn fetch_from_local_session(
    session: Option<&DiscoveredSession>,
    email: &str,
) -> Option<Value> {
    let session = session?;
    if !session.email.trim().eq_ignore_ascii_case(email) {
        return None;
    }
    let (Some(port), Some(csrf)) = (session.port, &session.csrf_token) else {
        return None;
    };
    fetch_quota_from_desktop_app(port, csrf).await
}

async fn post_cloudcode_json(client: &reqwest::Client, url: &str, token: &str) -> Option<Value> {
    let resp = client
        .post(url)
        .bearer_auth(token)
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .ok()?;
    if resp.status().is_success() {
        resp.json::<Value>().await.ok()
    } else {
        None
    }
}

async fn fetch_from_cloudcode_api(
    account: &mut AccountProfile,
    shared_client: Option<&reqwest::Client>,
) -> (Option<Value>, Option<Value>) {
    if account.access_token.is_none() && account.refresh_token.is_none() {
        return (None, None);
    }

    let (token_opt, updated_acc) = ensure_valid_token(account.clone()).await;
    let Some(token) = token_opt else {
        return (None, None);
    };

    if updated_acc != *account {
        *account = updated_acc;
    }

    let owned_client;
    let client = match shared_client {
        Some(client) => client,
        None => {
            owned_client = build_quota_http_client();
            let Some(client) = owned_client.as_ref() else {
                return (None, None);
            };
            client
        }
    };

    tokio::join!(
        post_cloudcode_json(client, QUOTA_SUMMARY_URL, &token),
        post_cloudcode_json(client, QUOTA_MODELS_URL, &token),
    )
}

async fn fetch_account_quota_inner(
    account: &mut AccountProfile,
    policy: QuotaRefreshPolicy,
    ctx: &QuotaFetchContext,
    history_file: Option<&Path>,
) -> AccountQuotaResult {
    let email = account.email.trim().to_lowercase();
    let now = current_epoch_seconds();

    if !policy.is_force_refresh()
        && let Some(cached) = cached_quota_result(&email, now)
    {
        return cached;
    }

    // Option A / A2: Active Desktop App or IDE session
    let mut quota_summary_data =
        fetch_from_local_session(ctx.desktop_session.as_ref(), &email).await;
    if quota_summary_data.is_none() {
        quota_summary_data = fetch_from_local_session(ctx.ide_session.as_ref(), &email).await;
    }

    // Option B: Query Google CloudCode API using OAuth token
    let mut models_data = None;
    if quota_summary_data.is_none() {
        (quota_summary_data, models_data) =
            fetch_from_cloudcode_api(account, ctx.client.as_ref()).await;
    }

    let Some(summary_val) = quota_summary_data else {
        // Fallback to persistent last-known quota or historical JSONL snapshot
        let mut last_known = account.last_known_quota.clone();
        if last_known.as_ref().is_none_or(|k| k.groups.is_empty()) {
            let hist = match history_file {
                Some(path) => historical_quota_from_file(&email, path),
                None => historical_quota(&email),
            };
            if let Some(hist) = hist {
                account.last_known_quota = Some(hist.clone());
                last_known = Some(hist);
            }
        }

        if let Some(lk) = last_known
            && !lk.groups.is_empty()
        {
            let result = build_quota_result(
                account,
                &email,
                QuotaResultPayload {
                    groups: lk.groups,
                    models: lk.models,
                    checked_at: lk.checked_at.unwrap_or(now),
                    source: QuotaSource::Cached,
                    error: None,
                },
            );
            insert_quota_cache(email, now, &result);
            return result;
        }

        return build_quota_result(
            account,
            &email,
            QuotaResultPayload {
                groups: Vec::new(),
                models: Vec::new(),
                checked_at: now,
                source: QuotaSource::Live,
                error: Some(
                    "Account session inactive. Log into Antigravity to refresh quota.".into(),
                ),
            },
        );
    };

    let parsed_groups = parse_quota_summary(&summary_val);
    let parsed_models = models_data
        .as_ref()
        .map(parse_models_quota)
        .unwrap_or_default();

    let result = build_quota_result(
        account,
        &email,
        QuotaResultPayload {
            groups: parsed_groups.clone(),
            models: parsed_models.clone(),
            checked_at: now,
            source: QuotaSource::Live,
            error: None,
        },
    );

    account.last_known_quota = Some(QuotaSnapshot {
        checked_at: Some(now),
        groups: parsed_groups,
        models: parsed_models,
        is_cached: Some(false),
        is_gemini_exhausted: None,
        is_3p_exhausted: None,
    });

    insert_quota_cache(email, now, &result);
    result
}

/// Fetches quota for a single `account` using pre-discovered sessions and a shared HTTP client.
///
/// Updates `account` in memory (tokens or `last_known_quota`) without writing to disk.
pub async fn fetch_account_quota_with_context(
    account: &mut AccountProfile,
    policy: impl Into<QuotaRefreshPolicy>,
    ctx: &QuotaFetchContext,
) -> AccountQuotaResult {
    fetch_account_quota_inner(account, policy.into(), ctx, None).await
}

/// Fetches quota for a single `account` and persists any profile updates to storage.
pub async fn fetch_account_quota(
    account: &mut AccountProfile,
    policy: impl Into<QuotaRefreshPolicy>,
) -> AccountQuotaResult {
    let policy = policy.into();
    let email = account.email.trim().to_lowercase();
    let now = current_epoch_seconds();

    if !policy.is_force_refresh()
        && let Some(cached) = cached_quota_result(&email, now)
    {
        return cached;
    }

    let ctx = QuotaFetchContext::discover().await;
    let original = account.clone();
    let result = fetch_account_quota_with_context(account, policy, &ctx).await;
    if *account != original {
        let _ = upsert_account(account);
    }
    result
}

pub(super) async fn fetch_accounts_quota_with_history(
    accounts: &mut HashMap<String, AccountProfile>,
    policy: impl Into<QuotaRefreshPolicy>,
    ctx: &QuotaFetchContext,
    history_file: Option<&Path>,
) -> (HashMap<String, AccountQuotaResult>, bool) {
    let policy = policy.into();
    let history_path = history_file.map(Path::to_path_buf);
    let mut join_set = JoinSet::new();

    for (email, acc) in accounts.iter() {
        if email.is_empty() {
            continue;
        }
        let email_key = email.clone();
        let mut acc_owned = acc.clone();
        let ctx_owned = ctx.clone();
        let hist_owned = history_path.clone();

        join_set.spawn(async move {
            let original = acc_owned.clone();
            let res = fetch_account_quota_inner(
                &mut acc_owned,
                policy,
                &ctx_owned,
                hist_owned.as_deref(),
            )
            .await;
            let acc_changed = acc_owned != original;
            (email_key, acc_owned, acc_changed, res)
        });
    }

    let mut results = HashMap::with_capacity(accounts.len());
    let mut any_updated = false;

    while let Some(joined) = join_set.join_next().await {
        if let Ok((email, updated_acc, acc_changed, res)) = joined {
            if acc_changed {
                accounts.insert(email.clone(), updated_acc);
                any_updated = true;
            }
            results.insert(email, res);
        }
    }

    (results, any_updated)
}

/// Concurrently fetches quota for every account in `accounts` using a shared [`QuotaFetchContext`].
pub async fn fetch_accounts_quota_with_context(
    accounts: &mut HashMap<String, AccountProfile>,
    policy: impl Into<QuotaRefreshPolicy>,
    ctx: &QuotaFetchContext,
) -> HashMap<String, AccountQuotaResult> {
    let (results, _) = fetch_accounts_quota_with_history(accounts, policy, ctx, None).await;
    results
}

/// Discovers active sessions once, fetches all registered account quotas concurrently, and saves changes once.
pub async fn fetch_all_accounts_quota(
    policy: impl Into<QuotaRefreshPolicy>,
) -> HashMap<String, AccountQuotaResult> {
    let policy = policy.into();
    let mut registry = load_accounts().unwrap_or_default();
    if registry.accounts.is_empty() {
        return HashMap::new();
    }

    let ctx = QuotaFetchContext::discover().await;
    let (results, updated) =
        fetch_accounts_quota_with_history(&mut registry.accounts, policy, &ctx, None).await;

    if updated {
        let _ = save_accounts(&registry);
    }
    results
}
