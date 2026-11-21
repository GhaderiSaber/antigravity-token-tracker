# Storage & Lifecycle Engine (`src/storage`)

## Overview
Manages persistent local file storage for account profiles, credentials, token quota history logs, and quota lifecycle countdown calculations.

## Interface
- `pub struct StorageManager`: Thread-safe persistence manager for the accounts registry and configuration files `[src/storage/storage.rs:StorageManager]`
- `pub struct AccountProfile`: Data model representing a registered Google account, OAuth refresh tokens, tier, and session metadata `[src/storage/models.rs:AccountProfile]`
- `pub struct AccountsRegistry`: Root persistent container maintaining registered accounts and the active email pointer `[src/storage/models.rs:AccountsRegistry]`
- `pub struct QuotaSnapshot`: Snapshot of an account's quota state, reset times, and exhaustion flags `[src/storage/models.rs:QuotaSnapshot]`
- `pub fn load_accounts() -> Result<AccountsRegistry>`: Loads the accounts registry from `accounts.json` `[src/storage/storage.rs:load_accounts]`
- `pub fn save_accounts(registry: &AccountsRegistry) -> Result<()>`: Atomically saves accounts registry with secure file permissions `[src/storage/storage.rs:save_accounts]`
- `pub fn upsert_account(account: &AccountProfile) -> Result<AccountProfile>`: Inserts or updates an account profile `[src/storage/storage.rs:upsert_account]`
- `pub fn remove_account(identifier: &str) -> Result<Option<AccountProfile>>`: Removes an account by index, email, or name substring `[src/storage/storage.rs:remove_account]`
- `pub fn analyze_account_lifecycle(quota: &QuotaSnapshot, now: DateTime<Utc>) -> QuotaSnapshot`: Calculates reset countdowns, urgency, and exhaustion flags `[src/storage/lifecycle.rs:analyze_account_lifecycle]`
- `pub fn compute_switching_recommendation(all_analyzed_quotas: &HashMap<String, QuotaSnapshot>, active_desktop_email: Option<&str>) -> Option<String>`: Evaluates registered accounts and recommends the healthiest candidate `[src/storage/lifecycle.rs:compute_switching_recommendation]`
- `pub fn log_lifecycle_snapshot(history_file_path: &Path, analyzed_quotas: &HashMap<String, (Option<String>, QuotaSnapshot)>) -> std::io::Result<()>`: Appends an analytical snapshot entry to `token_history.jsonl` `[src/storage/lifecycle.rs:log_lifecycle_snapshot]`

## Invariants
- Account email cannot be empty after trimming; returns `StorageError::InvalidData` if empty `[src/storage/storage.rs:StorageManager::upsert_account]`
- Account email is always normalized to trimmed lowercase before persistence and lookup `[src/storage/storage.rs:StorageManager::upsert_account]`
- When updating existing account, existing `refresh_token` is preserved if the updated `refresh_token` is None or empty; tier and name are preserved if updated values are None `[src/storage/storage.rs:StorageManager::upsert_account]`
- `updated_at` timestamp is always updated to current UTC epoch timestamp in fractional seconds `[src/storage/storage.rs:StorageManager::upsert_account]`
- If the removed account email matches `registry.active_email`, `active_email` is reset to None `[src/storage/storage.rs:StorageManager::remove_account]`
- Identifier matching follows strict resolution order: (1) 1-based index of alphabetically sorted emails, (2) exact lowercase email match, (3) case-insensitive substring search in account name or email `[src/storage/storage.rs:StorageManager::remove_account]`
- Returned list of `AccountProfile` entries is always sorted alphabetically by email `[src/storage/storage.rs:StorageManager::list_accounts]`
- Deserializes both V2 format (object with `active_email` and `accounts`) and legacy V1 format (flat map of accounts) `[src/storage/models.rs:AccountsRegistry::deserialize]`
- Always serializes `AccountsRegistry` into V2 format `[src/storage/models.rs:AccountsRegistry::serialize]`
- Maps remaining percentage thresholds to badges: `<= 1.0%` -> `'EXHAUSTED / FINISHED'`, `<= 10.0%` -> `'CRITICAL'`, `<= 25.0%` -> `'LOW'`, `<= 50.0%` -> `'MODERATE'`, else -> `'HEALTHY'` `[src/storage/lifecycle.rs:get_status_badge]`
- Returns `('N/A', 0.0)` if timestamp string cannot be parsed as ISO 8601; returns `('Refreshed / Ready', 0.0)` if `total_secs <= 0.0` `[src/storage/lifecycle.rs:format_countdown]`
- Bucket is flagged `is_exhausted = true` if remaining fraction `<= 0.01` `[src/storage/lifecycle.rs:analyze_account_lifecycle]`

## Side Effects
- Creates directory hierarchy at config directory and sets Unix permissions to 0o700 `[src/storage/storage.rs:StorageManager::ensure_config_dir]`
- Writes formatted JSON to temporary file (`.tmp`) with Unix permissions 0o600, flushes to disk, and atomically renames to `accounts.json` `[src/storage/storage.rs:StorageManager::save_accounts]`
- Reads `accounts.json` from disk using `fs::read_to_string` `[src/storage/storage.rs:StorageManager::load_accounts]`
- Opens and appends JSON-lines entries to `token_history.jsonl` `[src/storage/lifecycle.rs:log_lifecycle_snapshot]`

## Verification
- Run: `cargo test`
