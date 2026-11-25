# Quota & Burn Rate Engine (`src/quota`)

## Overview
Coordinates concurrent token quota extraction, remote Google CloudCode PA endpoint polling, local session querying, in-memory caching, and historical burn rate velocity estimation across all registered accounts.

## Interface
- Module exports (`src/quota/mod.rs`): `pub mod burnrate`, `pub mod models`, `pub mod quota`, with explicit symbol re-exports at `quota::*` (no glob `*` re-exports) `[src/quota/mod.rs]`
- `pub use crate::storage::models::{ModelQuota, QuotaBucket, QuotaGroup, QuotaSnapshot}`: Re-exported storage quota schema types `[src/quota/models.rs:ModelQuota]`
- `pub enum QuotaRefreshPolicy`: Cache vs force-refresh policy (`UseCache`, `ForceRefresh`) implementing `From<bool>` and `is_force_refresh` `[src/quota/models.rs:QuotaRefreshPolicy]`
- `pub struct AccountQuotaResult`: Unified container for an account's quota groups, model limits, and session states `[src/quota/models.rs:AccountQuotaResult]`
- `pub struct AccountVelocity`, `pub struct MultiwindowVelocity`, `pub struct PoolBurnrates`: Consumption rate metrics, depletion ETAs, multi-timeframe (`15m`, `1h`, `6h`) analytics, and fleet burn metrics `[src/quota/models.rs:AccountVelocity]`
- `pub const QUOTA_SUMMARY_URL: &str` / `pub const QUOTA_MODELS_URL: &str` / `pub const CACHE_TTL_SECONDS: f64`: Remote API URLs and cache TTL (`600.0`) `[src/quota/quota.rs:CACHE_TTL_SECONDS]`
- `pub struct QuotaFetchContext`: Shared batch fetch context holding `desktop_session: Option<DiscoveredSession>`, `ide_session: Option<DiscoveredSession>`, and `client: Option<reqwest::Client>`, with `QuotaFetchContext::new()` and `QuotaFetchContext::discover().await` `[src/quota/quota.rs:QuotaFetchContext]`
- `pub fn clear_quota_cache()`: Flushes the in-memory `QUOTA_CACHE` `[src/quota/quota.rs:clear_quota_cache]`
- `pub fn historical_quota_from_file(email: &str, history_file: &Path) -> Option<QuotaSnapshot>` / `pub fn historical_quota(email: &str) -> Option<QuotaSnapshot>` / `get_historical_quota`: Reconstructs last recorded weekly/5h snapshot from `token_history.jsonl` `[src/quota/quota.rs:historical_quota_from_file]`
- `pub fn parse_quota_summary(quota_summary_data: &Value) -> Vec<QuotaGroup>` / `pub fn parse_models_quota(models_data: &Value) -> Vec<ModelQuota>`: Parses group weekly/5h and per-model quota buckets `[src/quota/quota.rs:parse_quota_summary]`
- `pub async fn fetch_account_quota_with_context(account: &mut AccountProfile, policy: impl Into<QuotaRefreshPolicy>, ctx: &QuotaFetchContext) -> AccountQuotaResult`: Fetches quota for a single account using a shared [`QuotaFetchContext`] without writing to disk `[src/quota/quota.rs:fetch_account_quota_with_context]`
- `pub async fn fetch_account_quota(account: &mut AccountProfile, policy: impl Into<QuotaRefreshPolicy>) -> AccountQuotaResult`: Discovers context, fetches quota for a single account, and persists changed profile state via `upsert_account` `[src/quota/quota.rs:fetch_account_quota]`
- `pub async fn fetch_accounts_quota_with_context(accounts: &mut HashMap<String, AccountProfile>, policy: impl Into<QuotaRefreshPolicy>, ctx: &QuotaFetchContext) -> HashMap<String, AccountQuotaResult>`: Concurrently fetches quotas for a map of accounts using a shared [`QuotaFetchContext`] `[src/quota/quota.rs:fetch_accounts_quota_with_context]`
- `pub async fn fetch_all_accounts_quota(policy: impl Into<QuotaRefreshPolicy>) -> HashMap<String, AccountQuotaResult>`: Discovers sessions once, fetches all registered account quotas concurrently via `JoinSet`, and saves `accounts.json` once if any profile changed `[src/quota/quota.rs:fetch_all_accounts_quota]`
- `pub fn default_history_file() -> PathBuf` / `pub fn load_snapshots(history_file: &Path, max_age_seconds: f64, limit: usize, now_epoch: f64) -> Vec<Value>`: Resolves and loads/filters JSONL snapshots `[src/quota/burnrate.rs:load_snapshots]`
- `pub fn extract_account_quota_pct(snapshot: &Value, email: &str, group_name: &str) -> Option<f64>`: Extracts `weekly_pct` by exact group, `"gemini"` substring, or first group `[src/quota/burnrate.rs:extract_account_quota_pct]`
- `pub fn format_eta_countdown(seconds: f64) -> String` / `format_depletion_clock(target_timestamp: f64) -> String`: Formats human-readable ETA and local depletion clock `[src/quota/burnrate.rs:format_eta_countdown]`
- `pub fn calculate_account_velocity(...) -> AccountVelocity` / `calculate_multiwindow_velocity(...) -> MultiwindowVelocity` / `calculate_pool_burnrates(...) -> PoolBurnrates`: Computes hourly depletion velocity, multi-window profiles, and fleet burn rates `[src/quota/burnrate.rs:calculate_account_velocity]`
- `pub fn format_burnrate_badge(metrics: Option<&MultiwindowVelocity>) -> String` / `format_burnrate_tray_summary(metrics: Option<&MultiwindowVelocity>) -> String`: Formats Rich/tray burnrate status strings `[src/quota/burnrate.rs:format_burnrate_badge]`

## Invariants
- Cached quota is returned without refreshing if `!policy.into().is_force_refresh()` and `now - cached_time < CACHE_TTL_SECONDS` (`600.0`s) `[src/quota/quota.rs:fetch_account_quota_inner]`
- `QuotaFetchContext::discover` discovers Desktop and IDE sessions concurrently via `tokio::join!` and builds one shared `reqwest::Client` with a `12`s timeout and `antigravity/2.16.0` User-Agent `[src/quota/quota.rs:QuotaFetchContext::discover]`
- Quota discovery cascades through: (1) active Desktop App session (`models` remains empty), (2) active IDE session (`models` remains empty), (3) Google CloudCode PA OAuth endpoints (`QUOTA_SUMMARY_URL` and `QUOTA_MODELS_URL` queried concurrently via `tokio::join!`), and (4) persistent `last_known_quota` or `historical_quota` fallback (cached in `QUOTA_CACHE` with `is_cached: true`) `[src/quota/quota.rs:fetch_account_quota_inner]`
- `fetch_accounts_quota_with_context` and `fetch_all_accounts_quota` skip empty email keys and execute per-account fetches concurrently via `tokio::task::JoinSet` `[src/quota/quota.rs:fetch_accounts_quota_with_history]`
- Buckets are classified as weekly if `bucket_id.contains("weekly") || window == "weekly"`, and 5h if `bucket_id.contains("5h") || window == "5h"` `[src/quota/quota.rs:parse_quota_summary]`
- Snapshots are filtered to `ts >= now_epoch - max_age_seconds`, sorted ascending by timestamp, and truncated to the most recent `limit` entries `[src/quota/burnrate.rs:load_snapshots]`
- When series has fewer than 2 points or time span is `< 60.0`s, velocity is `0.0`; positive quota deltas (resets) are ignored when summing `total_consumed` `[src/quota/burnrate.rs:calculate_account_velocity]`
- An account is depleting (`is_depleting`) strictly when `velocity >= 0.5 && curr_pct > 0.0`; when `!is_depleting && curr_pct <= 1.0`, `eta_seconds` is `Some(0.0)`, `eta_human` is `"Exhausted"`, and `depletion_time_str` is `"Now"` `[src/quota/burnrate.rs:calculate_account_velocity]`
- Pace status is tiered strictly by velocity: `>= 25.0` (`"HIGH BURN"`), `>= 10.0` (`"ACTIVE"`), `>= 2.0` (`"MODERATE"`), and `< 2.0` (`"IDLE"`) `[src/quota/burnrate.rs:calculate_account_velocity]`
- Primary velocity window is `1h` by default, falling back to `6h` if `v_1h.datapoints < 3 && v_6h.datapoints >= 3`; `calculate_pool_burnrates` loads up to 500 snapshots within the last 86,400 seconds `[src/quota/burnrate.rs:calculate_multiwindow_velocity]`
- If `seconds <= 0.0`, `format_eta_countdown` returns `"Now"` `[src/quota/burnrate.rs:format_eta_countdown]`

## Side Effects
- Clears and mutates the global static in-memory `QUOTA_CACHE` mutex map (inserting both live and fallback results) `[src/quota/quota.rs:clear_quota_cache, src/quota/quota.rs:fetch_account_quota_inner]`
- Initiates process and port discovery for local Antigravity Desktop and IDE Language Server processes `[src/quota/quota.rs:QuotaFetchContext::discover]`
- Dispatches local HTTPS requests to Desktop/IDE Language Server endpoints and outbound HTTPS POST requests to Google CloudCode PA endpoints (`daily-cloudcode-pa.googleapis.com`) after `ensure_valid_token` `[src/quota/quota.rs:fetch_account_quota_inner]`
- Mutates `&mut AccountProfile` in place; `fetch_account_quota` persists changes via `upsert_account`, while `fetch_all_accounts_quota` reads via `load_accounts` and persists once via `save_accounts` when any profile changed `[src/quota/quota.rs:fetch_account_quota, src/quota/quota.rs:fetch_all_accounts_quota]`
- Reads `token_history.jsonl` from disk line-by-line `[src/quota/quota.rs:historical_quota_from_file, src/quota/burnrate.rs:load_snapshots]`

## Verification
- Run: `cargo test`
