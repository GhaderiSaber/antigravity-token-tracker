# Quota & Burn Rate Engine (`src/quota`)

## Overview
Coordinates token quota extraction, remote Google CloudCode PA endpoint polling, local session querying, in-memory caching, and historical burn rate velocity estimation across all registered accounts.

## Interface
- `pub struct AccountQuotaResult`: Unified container for an account's quota groups, model limits, and session states `[src/quota/models.rs:AccountQuotaResult]`
- `pub struct AccountVelocity`: Consumption rate metrics, depletion ETAs, and pace indicators for an account `[src/quota/models.rs:AccountVelocity]`
- `pub struct MultiwindowVelocity`: Multi-timeframe (1h, 6h, 24h) consumption analytics `[src/quota/models.rs:MultiwindowVelocity]`
- `pub struct PoolBurnrates`: Aggregate fleet consumption rates and active account burn metrics `[src/quota/models.rs:PoolBurnrates]`
- `pub fn calculate_account_velocity(snapshots: &[Value], email: &str, group_name: &str, window_seconds: f64, current_quota_pct: Option<f64>, now_epoch: f64) -> AccountVelocity`: Computes hourly depletion velocity and remaining time estimates `[src/quota/burnrate.rs:calculate_account_velocity]`
- `pub fn calculate_multiwindow_velocity(snapshots: &[Value], email: &str, group_name: &str, current_quota_pct: Option<f64>, now_epoch: f64) -> MultiwindowVelocity`: Computes multi-window velocity profiles `[src/quota/burnrate.rs:calculate_multiwindow_velocity]`
- `pub fn calculate_pool_burnrates(analyzed_quotas: &HashMap<String, AccountQuotaResult>, active_email: Option<String>, history_file: Option<&Path>, now_epoch: f64) -> PoolBurnrates`: Evaluates burn rates for all accounts in the pool `[src/quota/burnrate.rs:calculate_pool_burnrates]`
- `pub async fn fetch_account_quota(account: &mut AccountProfile, force_refresh: bool) -> AccountQuotaResult`: Fetches current quotas via tiered discovery cascade `[src/quota/quota.rs:fetch_account_quota]`
- `pub async fn fetch_all_accounts_quota(force_refresh: bool) -> HashMap<String, AccountQuotaResult>`: Concurrently retrieves quota snapshots for all registered accounts `[src/quota/quota.rs:fetch_all_accounts_quota]`
- `pub fn clear_quota_cache()`: Flushes the in-memory quota cache `[src/quota/quota.rs:clear_quota_cache]`

## Invariants
- Cached quota is returned without refreshing if `!force_refresh` and `now - cached_time < CACHE_TTL_SECONDS` (600.0s) `[src/quota/quota.rs:CACHE_TTL_SECONDS]`
- Quota discovery strictly cascades through tiered priority: (1) active Desktop App session, (2) active IDE session, (3) Google CloudCode PA OAuth endpoint, and (4) persistent/historical quota fallback `[src/quota/quota.rs:fetch_account_quota]`
- Buckets are classified as weekly if `bucket_id.contains("weekly") || window == "weekly"`, and 5h if `bucket_id.contains("5h") || window == "5h"` `[src/quota/quota.rs:parse_quota_summary]`
- Snapshots are filtered to `ts >= now_epoch - max_age_seconds`, sorted ascending by timestamp, and truncated to the most recent `limit` entries `[src/quota/burnrate.rs:load_snapshots]`
- When series has fewer than 2 points, velocity is 0.0, pace_status is `"IDLE"`, pace_icon is `"💤"`, and is_depleting is false `[src/quota/burnrate.rs:calculate_account_velocity]`
- If the time delta between first and last snapshot is under 60 seconds, velocity is 0.0 `[src/quota/burnrate.rs:calculate_account_velocity]`
- An account is depleting (`is_depleting`) strictly when velocity >= 0.5 and current quota percentage > 0.0 `[src/quota/burnrate.rs:calculate_account_velocity]`
- Pace status is tiered strictly by velocity: >= 25.0 (`"HIGH BURN"`), >= 10.0 (`"ACTIVE"`), >= 2.0 (`"MODERATE"`), and < 2.0 (`"IDLE"`) `[src/quota/burnrate.rs:calculate_account_velocity]`
- Primary velocity window is 1h by default, but falls back to 6h if `v_1h.datapoints < 3` and `v_6h.datapoints >= 3` `[src/quota/burnrate.rs:calculate_multiwindow_velocity]`
- If `seconds <= 0.0`, `format_eta_countdown` returns `"Now"` `[src/quota/burnrate.rs:format_eta_countdown]`

## Side Effects
- Clears and mutates the global static in-memory `QUOTA_CACHE` mutex map `[src/quota/quota.rs:clear_quota_cache]`
- Initiates process and port discovery for local Antigravity Language Server processes `[src/quota/quota.rs:fetch_account_quota]`
- Dispatches local HTTPS requests to Desktop/IDE Language Server endpoints `[src/quota/quota.rs:fetch_account_quota]`
- Performs outbound HTTPS POST requests to Google CloudCode PA endpoints (`daily-cloudcode-pa.googleapis.com`) `[src/quota/quota.rs:fetch_account_quota]`
- Reads and updates account profiles in persistent storage via `upsert_account` and `save_accounts` `[src/quota/quota.rs:fetch_account_quota]`
- Reads `token_history.jsonl` from disk line-by-line `[src/quota/burnrate.rs:load_snapshots]`

## Verification
- Run: `cargo test`
