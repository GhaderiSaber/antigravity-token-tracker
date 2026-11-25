# Authentication & Session Discovery (`src/auth`)

## Overview
Discovers active Antigravity Desktop App and Antigravity IDE sessions, extracts OAuth credentials from system keyrings and SQLite state databases, handles Google OAuth2 token refresh workflows, and synchronizes accounts into the persistent registry.

## Interface
- Module exports (`src/auth/mod.rs`): `pub mod keyring`, `pub mod models`, `pub mod oauth`, `pub mod process_discovery`, `pub mod sqlite`, `pub mod sync`, explicit symbol re-exports at `auth::*`, plus backwards-compatibility module aliases `keyring_reader`, `credentials`, and `sqlite_reader` `[src/auth/mod.rs]`
- `pub async fn desktop_keyring_token() -> Option<KeyringTokenRecord>` / `pub async fn get_desktop_keyring_token() -> Option<KeyringTokenRecord>`: Queries the Linux Secret Service D-Bus interface for stored desktop tokens `[src/auth/keyring.rs:desktop_keyring_token]`
- `pub struct DiscoveredSession`: Holds active session metadata, discovered email, surface type, tokens, PID, and CSRF parameters `[src/auth/models.rs:DiscoveredSession]`
- `pub struct OAuthTokens`: Container for OAuth access tokens and refresh tokens `[src/auth/models.rs:OAuthTokens]`
- `pub struct UserInfo`: Google user profile metadata returned by the userinfo endpoint `[src/auth/models.rs:UserInfo]`
- `pub struct KeyringTokenRecord`: Decoded email, parsed JSON token payload, and raw secret string from the keyring `[src/auth/models.rs:KeyringTokenRecord]`
- `pub static CLIENT_ID: LazyLock<String>`, `pub static CLIENT_SECRETS: LazyLock<Vec<String>>`, `pub const TOKEN_URL: &str`, `pub const USERINFO_URL: &str`: OAuth2 client configuration and Google endpoints `[src/auth/oauth.rs:CLIENT_ID, src/auth/oauth.rs:CLIENT_SECRETS, src/auth/oauth.rs:TOKEN_URL, src/auth/oauth.rs:USERINFO_URL]`
- `pub async fn refresh_access_token(refresh_token: &str) -> Option<(String, i64)>`: Exchanges an OAuth refresh token for a fresh access token `[src/auth/oauth.rs:refresh_access_token]`
- `pub async fn fetch_user_info(access_token: &str) -> Option<UserInfo>`: Fetches user profile metadata from Google OAuth endpoints `[src/auth/oauth.rs:fetch_user_info]`
- `pub async fn ensure_valid_token(mut account: AccountProfile) -> (Option<String>, AccountProfile)`: Validates current access token or automatically refreshes it `[src/auth/oauth.rs:ensure_valid_token]`
- `pub fn listening_ports_by_pid() -> HashMap<i32, Vec<u16>>` / `pub fn get_listening_ports_by_pid() -> HashMap<i32, Vec<u16>>`: Maps process IDs to their active `127.0.0.1` TCP listening ports `[src/auth/process_discovery.rs:listening_ports_by_pid]`
- `pub async fn query_local_user_status(port: u16, csrf_token: &str) -> Option<(String, String, String)>`: Queries local Language Server `GetUserStatus` RPC for `(email, name, tier)` `[src/auth/process_discovery.rs:query_local_user_status]`
- `pub async fn fetch_quota_from_desktop_app(port: u16, csrf_token: &str) -> Option<Value>`: Queries local Language Server `RetrieveUserQuotaSummary` RPC `[src/auth/process_discovery.rs:fetch_quota_from_desktop_app]`
- `pub fn is_desktop_language_server_cmdline(cmd: &str) -> bool`: Returns `true` when `/proc/{pid}/cmdline` belongs to the Desktop App language server `[src/auth/process_discovery.rs:is_desktop_language_server_cmdline]`
- `pub fn read_desktop_last_login_username(app_storage_path: &Path) -> Option<String>`: Reads `agy.onboarding.lastLoginUsername` (or fallback `jetski.onboarding.lastLoginUsername`) from `app_storage.json` `[src/auth/process_discovery.rs:read_desktop_last_login_username]`
- `pub fn resolve_desktop_session_email(live_email: &str, keyring_email: Option<&str>, app_storage_email: Option<&str>) -> Option<String>`: Resolves and normalizes desktop session email by priority `[src/auth/process_discovery.rs:resolve_desktop_session_email]`
- `pub async fn discover_antigravity_desktop_app() -> Option<DiscoveredSession>`: Discovers running Desktop App processes (with headless v2.35+ keyring/`app_storage.json` and `main.log` fallbacks) `[src/auth/process_discovery.rs:discover_antigravity_desktop_app]`
- `pub async fn discover_antigravity_ide() -> Option<DiscoveredSession>`: Discovers running Antigravity IDE language server processes `[src/auth/process_discovery.rs:discover_antigravity_ide]`
- `pub fn default_ide_state_db() -> PathBuf`: Returns the default path to the Antigravity IDE `state.vscdb` file `[src/auth/sqlite.rs:default_ide_state_db]`
- `pub fn extract_tokens_from_state_db(db_path: &Path) -> Option<OAuthTokens>`: Extracts OAuth tokens from the IDE's SQLite state database `[src/auth/sqlite.rs:extract_tokens_from_state_db]`
- `pub fn extract_user_email_from_state_db(db_path: &Path) -> Option<String>`: Extracts normalized user email from the IDE's `userStatus` row `[src/auth/sqlite.rs:extract_user_email_from_state_db]`
- `pub fn extract_user_tier(db_path: &Path) -> String`: Resolves subscription tier from the IDE's `userStatus` row `[src/auth/sqlite.rs:extract_user_tier]`
- `pub enum SyncError`: Typed sync error with `Storage(#[from] StorageError)` and `Io(#[from] std::io::Error)` (`#[must_use]`, `thiserror::Error`) `[src/auth/sync.rs:SyncError]`
- `pub async fn sync_from_antigravity(storage: &StorageManager) -> Result<(Option<String>, HashMap<String, AccountProfile>), SyncError>`: High-level sync entry point orchestrating full session discovery and persisting updated accounts `[src/auth/sync.rs:sync_from_antigravity]`

## Invariants
- `desktop_keyring_token` searches `service='gemini'`, `username='antigravity'` and returns `None` if Secret Service `SearchItems` returns an empty unlocked items list `[src/auth/keyring.rs:desktop_keyring_token]`
- Requires `id_token` to contain at least 2 dot-separated segments for JWT payload email extraction, normalized to trimmed lowercase `[src/auth/keyring.rs:desktop_keyring_token]`
- `CLIENT_ID` loads from `ANTIGRAVITY_CLIENT_ID` with fallback to XOR-decoded static constant (`0x42` key); `CLIENT_SECRETS` loads primary secret from `ANTIGRAVITY_CLIENT_SECRET` with XOR-decoded fallback plus a secondary XOR-decoded secret `[src/auth/oauth.rs:CLIENT_ID, src/auth/oauth.rs:CLIENT_SECRETS]`
- `refresh_access_token` returns `None` immediately if `refresh_token.trim()` is empty, tries `CLIENT_SECRETS` in order, and defaults token expiration to 3600 seconds if `expires_in` is absent `[src/auth/oauth.rs:refresh_access_token]`
- `ensure_valid_token` retains current access token if `fetch_user_info` succeeds (populating missing `email` and `name`); if expired/invalid, refreshes via `refresh_token` and populates missing `email`, `name`, and `picture` `[src/auth/oauth.rs:ensure_valid_token]`
- `query_local_user_status` defaults tier to `'Standard'` if `userStatus.userTier.name` is absent or non-string, and normalizes email to trimmed lowercase `[src/auth/process_discovery.rs:query_local_user_status]`
- `discover_antigravity_desktop_app` candidate `/proc/{pid}/cmdline` must satisfy `is_desktop_language_server_cmdline` (containing `'language_server'`, matching `'subclient_type hub'` or `'app_data_dir antigravity'`/`'/snap/antigravity/'`, and excluding `'antigravity-ide'` / `'subclient_type ide'`); when Antigravity v2.35+ passes stdin host-bridge tokens without `--csrf_token`, falls back to keyring + `app_storage.json`, then `~/.config/Antigravity/logs/main.log` `[src/auth/process_discovery.rs:discover_antigravity_desktop_app]`
- `discover_antigravity_ide` candidate `/proc/{pid}/cmdline` must contain `'language_server'` and either `'antigravity-ide'` or `'subclient_type ide'` `[src/auth/process_discovery.rs:discover_antigravity_ide]`
- `extract_tokens_from_state_db` returns `None` if `db_path` does not exist or if neither `ya29.*` access token nor `1//*` refresh token is found `[src/auth/sqlite.rs:extract_tokens_from_state_db]`
- `extract_user_tier` returns `'Unknown'` if `db_path` does not exist; otherwise resolves to `'Google AI Ultra'` if content contains `'Google AI Ultra'`, `'Google AI Pro'` if it contains `'Google AI Pro'` or `'g1-pro-tier'`, `'Google One'` if it contains `'g1-'`, and `'Standard'` on DB/decode error or fallback `[src/auth/sqlite.rs:extract_user_tier]`
- Active `active_email` discovery order precedence: desktop app session first, then IDE process session, then IDE `state.vscdb` SQLite file; `sync_from_antigravity` recomputes `is_current_desktop_session` and `is_current_ide_session` for all accounts strictly from live `desktop_email` and `ide_proc_email` (so `state.vscdb` fallback sets `active_email` and tokens without marking `is_current_ide_session` when no live IDE process is running) and returns `SyncError::Storage` if `storage.save_accounts(&registry)` fails `[src/auth/sync.rs:sync_from_antigravity]`

## Side Effects
- D-Bus connection and Secret Service method calls on `org.freedesktop.secrets` `[src/auth/keyring.rs:desktop_keyring_token]`
- Outbound HTTPS POST requests to `https://oauth2.googleapis.com/token` `[src/auth/oauth.rs:refresh_access_token]`
- Outbound HTTPS GET requests to `https://www.googleapis.com/oauth2/v3/userinfo` `[src/auth/oauth.rs:fetch_user_info]`
- Executes `ss -tulpn` child process to map listening ports `[src/auth/process_discovery.rs:listening_ports_by_pid]`
- Reads `/proc/{pid}/cmdline`, `~/.config/Antigravity/app_storage.json`, and `~/.config/Antigravity/logs/main.log` `[src/auth/process_discovery.rs:discover_antigravity_desktop_app, src/auth/process_discovery.rs:discover_antigravity_ide]`
- Outbound HTTPS POST requests to local Language Server endpoints at `https://127.0.0.1:{port}` with disabled TLS certificate verification `[src/auth/process_discovery.rs:query_local_user_status, src/auth/process_discovery.rs:fetch_quota_from_desktop_app]`
- Reads SQLite database `state.vscdb` in read-only mode `[src/auth/sqlite.rs:extract_tokens_from_state_db, src/auth/sqlite.rs:extract_user_email_from_state_db, src/auth/sqlite.rs:extract_user_tier]`
- Reads and updates persistent accounts registry on disk via `storage.load_accounts()` and `storage.save_accounts()` `[src/auth/sync.rs:sync_from_antigravity]`

## Verification
- Run: `cargo test`
