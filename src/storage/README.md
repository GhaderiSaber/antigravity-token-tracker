# Storage & Lifecycle Engine (`src/storage`)

## Overview
Manages persistent local file storage for account profiles, credentials, advisory lock coordination, token quota history logs, and quota lifecycle countdown calculations.

## Interface
- Module exports (`src/storage/mod.rs`): `pub mod lifecycle`, `pub mod models`, `pub mod storage` with `pub use ...::*;` re-exports `[src/storage/mod.rs]`
- `pub enum StorageError`: Error enum covering `Io(io::Error)`, `Json(serde_json::Error)`, and `InvalidData(String)` `[src/storage/storage.rs:StorageError]`
- `pub type Result<T>`: Module result alias `std::result::Result<T, StorageError>` `[src/storage/storage.rs:Result]`
- `pub fn default_config_dir() -> PathBuf`: Resolves `dirs::config_dir()` (`XDG_CONFIG_HOME` when absolute, else `$HOME/.config`) joined with `antigravity-token-tracker` (or relative `.config/antigravity-token-tracker`) `[src/storage/storage.rs:default_config_dir]`
- `pub fn default_history_file() -> PathBuf`: Resolves `~/.gemini/antigravity/token_history.jsonl` (or relative `.gemini/antigravity/token_history.jsonl`) `[src/storage/storage.rs:default_history_file]`
- `pub struct StorageManager` (`pub type Storage = StorageManager`): Persistent manager for `accounts.json`, `.accounts.lock`, and config directory; exposes `new`, `config_dir`, `accounts_file`, `accounts_lock_file`, `ensure_config_dir`, `load_accounts`, `save_accounts`, `account`, `get_account`, `list_accounts`, `upsert_account`, `remove_account` `[src/storage/storage.rs:StorageManager]`
- `pub struct ModelQuota`: Per-model quota entry (`model_id`, `remaining_fraction`, `remaining_percent`, `reset_time`) `[src/storage/models.rs:ModelQuota]`
- `pub struct QuotaBucket`: Window quota bucket (`weekly` / `fiveHour`) with lifecycle-populated `countdown`, `seconds_remaining`, `status_badge`, `status_style`, and `is_exhausted` `[src/storage/models.rs:QuotaBucket]`
- `pub struct QuotaGroup`: Named quota group containing optional `weekly` and `five_hour` buckets `[src/storage/models.rs:QuotaGroup]`
- `pub struct QuotaSnapshot`: Snapshot storing `checked_at`, `groups`, `models`, `is_cached`, `is_gemini_exhausted`, and `is_3p_exhausted` (`reset_time` resides in nested `QuotaBucket` and `ModelQuota`) `[src/storage/models.rs:QuotaSnapshot]`
- `pub struct AccountProfile`: Data model representing a registered Google account, OAuth tokens, tier, active surface flags, and session metadata `[src/storage/models.rs:AccountProfile]`
- `pub struct AccountsRegistry`: Root persistent container maintaining `active_email` and `accounts` map `[src/storage/models.rs:AccountsRegistry]`
- `pub fn load_accounts() -> Result<AccountsRegistry>` / `save_accounts` / `upsert_account` / `list_accounts` / `account` / `get_account` / `remove_account`: Top-level convenience wrappers delegating to `StorageManager::default()` `[src/storage/storage.rs]`
- `pub struct StatusBadge`: Badge label and style pair `[src/storage/lifecycle.rs:StatusBadge]`
- `pub fn status_badge(remaining_fraction: f64) -> StatusBadge` / `get_status_badge`: Maps remaining fraction to status badge `[src/storage/lifecycle.rs:status_badge]`
- `pub fn parse_iso_utc(ts_str: &str) -> Option<DateTime<Utc>>`: Parses RFC3339 or `%Y-%m-%dT%H:%M:%SZ` timestamp strings into UTC `[src/storage/lifecycle.rs:parse_iso_utc]`
- `pub fn format_countdown(reset_time_str: &str, now: DateTime<Utc>) -> (String, f64)`: Formats remaining time until reset and returns seconds remaining `[src/storage/lifecycle.rs:format_countdown]`
- `pub fn analyze_account_lifecycle(quota: &QuotaSnapshot, now: DateTime<Utc>) -> QuotaSnapshot`: Calculates reset countdowns, badges, and exhaustion flags `[src/storage/lifecycle.rs:analyze_account_lifecycle]`
- `pub fn compute_switching_recommendation(all_analyzed_quotas: &HashMap<String, QuotaSnapshot>, active_desktop_email: Option<&str>) -> Option<String>`: Evaluates registered accounts and recommends switching when Gemini quota is exhausted `[src/storage/lifecycle.rs:compute_switching_recommendation]`
- `pub fn log_lifecycle_snapshot(history_file_path: &Path, analyzed_quotas: &HashMap<String, (Option<String>, QuotaSnapshot)>) -> std::io::Result<()>`: Appends an analytical snapshot entry to `token_history.jsonl` `[src/storage/lifecycle.rs:log_lifecycle_snapshot]`

## Invariants
- Advisory file locking (`AccountsLockGuard(File)` over `.accounts.lock` with `0o600` Unix permissions): `load_accounts` acquires a shared lock (`FileExt::lock_shared`) when `config_dir` exists; `save_accounts`, `upsert_account`, and `remove_account` acquire an exclusive lock (`FileExt::lock_exclusive`) covering the full read-modify-write cycle; `AccountsLockGuard::drop` unlocks without unlinking `.accounts.lock` to prevent inode races `[src/storage/storage.rs:StorageManager::acquire_lock]`
- `load_accounts` returns `Ok(AccountsRegistry::default())` when `config_dir` does not exist, `accounts.json` does not exist, or `accounts.json` contains only whitespace `[src/storage/storage.rs:StorageManager::load_accounts]`
- Account email cannot be empty after trimming; returns `StorageError::InvalidData` if empty; always normalized to trimmed lowercase before persistence and lookup `[src/storage/storage.rs:StorageManager::upsert_account]`
- When updating an existing account, existing `refresh_token` is preserved if the updated `refresh_token` is `None` or empty; `tier` and `name` are preserved if updated values are `None` `[src/storage/storage.rs:StorageManager::upsert_account]`
- `updated_at` timestamp is always updated to current UTC epoch timestamp in fractional seconds `[src/storage/storage.rs:StorageManager::upsert_account]`
- If the removed account email matches `registry.active_email`, `active_email` is reset to `None`; identifier matching resolves (1) 1-based index of alphabetically sorted emails, (2) exact lowercase email match, (3) case-insensitive substring in name or email `[src/storage/storage.rs:StorageManager::remove_account]`
- Returned list of `AccountProfile` entries is always sorted alphabetically by email `[src/storage/storage.rs:StorageManager::list_accounts]`
- Deserializes both V2 format (object with `active_email` and `accounts`) and legacy V1 format (flat map of accounts); always serializes `AccountsRegistry` into V2 format `[src/storage/models.rs:AccountsRegistry]`
- Maps remaining percentage (`remaining_fraction * 100.0`) thresholds to badges: `<= 1.0%` -> `'EXHAUSTED / FINISHED'`, `<= 10.0%` -> `'CRITICAL'`, `<= 25.0%` -> `'LOW'`, `<= 50.0%` -> `'MODERATE'`, else -> `'HEALTHY'` `[src/storage/lifecycle.rs:status_badge]`
- Returns `('N/A', 0.0)` if timestamp string cannot be parsed as ISO 8601; returns `('Refreshed / Ready', 0.0)` if `total_secs <= 0.0` `[src/storage/lifecycle.rs:format_countdown]`
- Weekly and 5-hour buckets are flagged `is_exhausted = true` if remaining fraction `<= 0.01`; `is_gemini_exhausted` and `is_3p_exhausted` track weekly exhaustion by whether `group.display_name` contains `"gemini"` (case-insensitive) `[src/storage/lifecycle.rs:analyze_account_lifecycle]`
- In `compute_switching_recommendation`: for `< 2` accounts, returns a single-account exhaustion notice when `is_gemini_exhausted` is `true`; for `>= 2` accounts, skips accounts with empty `groups` and classifies Gemini weekly `remaining_percent <= 1.0` as exhausted even if `is_gemini_exhausted` is `false`/`None` `[src/storage/lifecycle.rs:compute_switching_recommendation]`

## Side Effects
- Creates directory hierarchy at config directory (`0o700` on Unix) and advisory lockfile `.accounts.lock` (`0o600` on Unix) `[src/storage/storage.rs:StorageManager::ensure_config_dir, src/storage/storage.rs:StorageManager::acquire_lock]`
- Writes formatted JSON to temporary `.tmp` file with Unix permissions `0o600`, flushes to disk, and atomically renames to `accounts.json` `[src/storage/storage.rs:StorageManager::save_accounts]`
- Reads `accounts.json` from disk using `fs::read_to_string` `[src/storage/storage.rs:StorageManager::load_accounts]`
- Creates parent directories if missing and opens/appends JSON-lines entries to `token_history.jsonl` `[src/storage/lifecycle.rs:log_lifecycle_snapshot]`

## Verification
- Run: `cargo test`
