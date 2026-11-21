# Authentication & Session Discovery (`src/auth`)

## Overview
Discovers active Antigravity Desktop App and Antigravity IDE sessions, extracts OAuth credentials from system keyrings and SQLite state databases, handles Google OAuth2 token refresh workflows, and synchronizes accounts into the persistent registry.

## Interface
- `pub async fn get_desktop_keyring_token() -> Option<(String, Value, String)>`: Queries the Linux Secret Service D-Bus interface for stored desktop tokens `[src/auth/keyring.rs:get_desktop_keyring_token]`
- `pub struct DiscoveredSession`: Holds active session metadata, discovered email, surface type, tokens, and CSRF parameters `[src/auth/models.rs:DiscoveredSession]`
- `pub struct OAuthTokens`: Container for OAuth access tokens and refresh tokens `[src/auth/models.rs:OAuthTokens]`
- `pub struct UserInfo`: Google user profile metadata returned by the userinfo endpoint `[src/auth/models.rs:UserInfo]`
- `pub async fn refresh_access_token(refresh_token: &str) -> Option<(String, i64)>`: Exchanges an OAuth refresh token for a fresh access token `[src/auth/oauth.rs:refresh_access_token]`
- `pub async fn fetch_user_info(access_token: &str) -> Option<UserInfo>`: Fetches user profile metadata from Google OAuth endpoints `[src/auth/oauth.rs:fetch_user_info]`
- `pub async fn ensure_valid_token(mut account: AccountProfile) -> (Option<String>, AccountProfile)`: Validates current access token or automatically refreshes it `[src/auth/oauth.rs:ensure_valid_token]`
- `pub fn get_listening_ports_by_pid() -> HashMap<i32, Vec<u16>>`: Maps process IDs to their active TCP listening ports `[src/auth/process_discovery.rs:get_listening_ports_by_pid]`
- `pub async fn discover_antigravity_desktop_app() -> Option<DiscoveredSession>`: Discovers running Desktop App processes and extracts session status `[src/auth/process_discovery.rs:discover_antigravity_desktop_app]`
- `pub async fn discover_antigravity_ide() -> Option<DiscoveredSession>`: Discovers running Antigravity IDE language server processes `[src/auth/process_discovery.rs:discover_antigravity_ide]`
- `pub fn extract_tokens_from_state_db(db_path: &Path) -> Option<OAuthTokens>`: Extracts OAuth tokens from the IDE's SQLite state database `[src/auth/sqlite.rs:extract_tokens_from_state_db]`
- `pub async fn sync_from_antigravity(storage: &StorageManager) -> Result<(Option<String>, HashMap<String, AccountProfile>), Box<dyn std::error::Error + Send + Sync>>`: High-level sync entry point orchestrating full session discovery `[src/auth/sync.rs:sync_from_antigravity]`

## Invariants
- Returns None if Secret Service SearchItems returns an empty unlocked items list `[src/auth/keyring.rs:get_desktop_keyring_token]`
- Requires `id_token` to contain at least 2 dot-separated segments for JWT payload extraction `[src/auth/keyring.rs:get_desktop_keyring_token]`
- `CLIENT_ID` loaded from `ANTIGRAVITY_CLIENT_ID` environment variable with fallback to XOR-decoded static constant (0x42 key) `[src/auth/oauth.rs:CLIENT_ID]`
- `CLIENT_SECRETS` loaded from `ANTIGRAVITY_CLIENT_SECRET` environment variable with fallback to XOR-decoded static constants (0x42 key) `[src/auth/oauth.rs:CLIENT_SECRETS]`
- `refresh_access_token` returns None immediately if `refresh_token` trimmed is empty `[src/auth/oauth.rs:refresh_access_token]`
- Defaults token expiration to 3600 seconds if `expires_in` is absent `[src/auth/oauth.rs:refresh_access_token]`
- `ensure_valid_token` retains current access token if `fetch_user_info` succeeds; refreshes via `refresh_token` if expired `[src/auth/oauth.rs:ensure_valid_token]`
- Defaults tier to `'Standard'` if `userStatus.userTier.name` is absent or non-string, and email is normalized to trimmed lowercase `[src/auth/process_discovery.rs:query_local_user_status]`
- Candidate process cmdline must contain `'language_server'`, match hub/antigravity app_data_dir, and must not contain `'antigravity-ide'` or `'subclient_type ide'` `[src/auth/process_discovery.rs:discover_antigravity_desktop_app]`
- Candidate process cmdline must contain `'language_server'` and either `'antigravity-ide'` or `'subclient_type ide'` `[src/auth/process_discovery.rs:discover_antigravity_ide]`
- `extract_tokens_from_state_db` returns None if `db_path` does not exist or if neither `ya29.*` access token nor `1//*` refresh token is found `[src/auth/sqlite.rs:extract_tokens_from_state_db]`
- Tier resolves to `'Google AI Ultra'` if content contains `'Google AI Ultra'`, `'Google AI Pro'` if contains `'Google AI Pro'` or `'g1-pro-tier'`, `'Google One'` if contains `'g1-'`, and `'Standard'` on error or fallback `[src/auth/sqlite.rs:extract_user_tier]`
- Active session discovery order precedence: desktop app session first, then IDE process session, then IDE `state.vscdb` SQLite file `[src/auth/sync.rs:sync_from_antigravity]`

## Side Effects
- D-Bus connection and Secret Service method calls on `org.freedesktop.secrets` `[src/auth/keyring.rs:get_desktop_keyring_token]`
- Outbound HTTPS POST requests to `https://oauth2.googleapis.com/token` `[src/auth/oauth.rs:refresh_access_token]`
- Outbound HTTPS GET requests to `https://www.googleapis.com/oauth2/v3/userinfo` `[src/auth/oauth.rs:fetch_user_info]`
- Executes `ss -tulpn` child process to map listening ports `[src/auth/process_discovery.rs:get_listening_ports_by_pid]`
- Outbound HTTPS POST requests to local Language Server endpoints at `https://127.0.0.1:{port}` with disabled TLS certificate verification `[src/auth/process_discovery.rs:query_local_user_status]`
- Reads SQLite database `state.vscdb` in read-only mode `[src/auth/sqlite.rs:extract_tokens_from_state_db]`
- Reads and updates persistent accounts registry on disk via `storage.save_accounts()` `[src/auth/sync.rs:sync_from_antigravity]`

## Verification
- Run: `cargo test`
