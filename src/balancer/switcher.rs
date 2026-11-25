use super::models::{RestartProcess, SessionSnapshotMeta, SnapshotResult};
use crate::auth::get_desktop_keyring_token;
use crate::storage::models::AccountsRegistry;
use crate::storage::{load_accounts, save_accounts};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

pub fn default_sessions_dir() -> PathBuf {
    dirs::config_dir().map_or_else(
        || PathBuf::from(".config/antigravity-token-tracker/sessions"),
        |c| c.join("antigravity-token-tracker/sessions"),
    )
}

pub fn default_desktop_config_dir() -> PathBuf {
    dirs::config_dir().map_or_else(
        || PathBuf::from(".config/Antigravity"),
        |c| c.join("Antigravity"),
    )
}

pub fn default_ide_state_db() -> PathBuf {
    dirs::config_dir().map_or_else(
        || PathBuf::from(".config/Antigravity IDE/User/globalStorage/state.vscdb"),
        |c| c.join("Antigravity IDE/User/globalStorage/state.vscdb"),
    )
}

pub const DESKTOP_FILES_TO_SAVE: &[&str] = &[
    "app_storage.json",
    "Cookies",
    "Network Persistent State",
    "SharedStorage",
    "Preferences",
    "Local State",
];

pub const DESKTOP_DIRS_TO_SAVE: &[&str] = &["Local Storage", "Session Storage"];

pub fn ensure_sessions_dir() {
    let dir = default_sessions_dir();
    let _ = fs::create_dir_all(&dir);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
}

pub fn account_session_dir(email: &str) -> PathBuf {
    let clean = email.trim().to_lowercase();
    default_sessions_dir().join(clean)
}

pub fn ensure_account_session_dir(email: &str) -> PathBuf {
    let p = account_session_dir(email);
    ensure_sessions_dir();
    let _ = fs::create_dir_all(&p);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o700));
    }
    p
}

pub fn get_account_session_dir(email: &str, create: bool) -> PathBuf {
    if create {
        ensure_account_session_dir(email)
    } else {
        account_session_dir(email)
    }
}

pub fn has_account_credentials(email: &str, registered: Option<&AccountsRegistry>) -> bool {
    let clean = email.trim().to_lowercase();
    let s_dir = account_session_dir(&clean);
    if s_dir.is_dir() {
        let has_desktop = s_dir.join("desktop").is_dir();
        let has_keyring = s_dir.join("keyring_token.json").is_file();
        let has_ide = s_dir.join("ide_tokens.json").is_file();
        if has_desktop || has_keyring || has_ide {
            return true;
        }
    }

    if let Some(reg) = registered {
        if let Some(acc) = reg.accounts.get(&clean) {
            return acc.refresh_token.as_ref().is_some_and(|r| !r.is_empty());
        }
    } else if let Ok(reg) = load_accounts()
        && let Some(acc) = reg.accounts.get(&clean)
    {
        return acc.refresh_token.as_ref().is_some_and(|r| !r.is_empty());
    }

    false
}

pub async fn set_keyring_secret(secret_json_str: &str) -> bool {
    crate::auth::set_desktop_keyring_secret(secret_json_str)
        .await
        .is_ok()
}

pub async fn clear_keyring_secret() -> bool {
    crate::auth::clear_desktop_keyring_secret()
        .await
        .unwrap_or(false)
}

pub async fn snapshot_desktop_session(email: &str) -> bool {
    let desktop_dir = default_desktop_config_dir();
    if !desktop_dir.is_dir() {
        return false;
    }

    let clean = email.trim().to_lowercase();
    let target_dir = ensure_account_session_dir(&clean).join("desktop");
    let _ = fs::create_dir_all(&target_dir);

    let mut copied_any = false;
    for &f in DESKTOP_FILES_TO_SAVE {
        let src = desktop_dir.join(f);
        if src.is_file() {
            let dest = target_dir.join(f);
            if fs::copy(&src, &dest).is_ok() {
                copied_any = true;
            }
        }
    }

    for &d in DESKTOP_DIRS_TO_SAVE {
        let src = desktop_dir.join(d);
        if src.is_dir() {
            let dest = target_dir.join(d);
            let _ = fs::remove_dir_all(&dest);
            if copy_dir_recursive(&src, &dest).is_ok() {
                copied_any = true;
            }
        }
    }

    // Keyring snapshot
    if let Some(record) = get_desktop_keyring_token().await
        && (record.email.is_empty() || record.email.eq_ignore_ascii_case(&clean))
    {
        let keyring_file = account_session_dir(&clean).join("keyring_token.json");
        if fs::write(&keyring_file, &record.raw_secret).is_ok() {
            copied_any = true;

            if let Some(tok) = record.data.get("token")
                && let Ok(mut reg) = load_accounts()
                && let Some(acc) = reg.accounts.get_mut(&clean)
            {
                if let Some(a_tok) = tok.get("access_token").and_then(|v| v.as_str()) {
                    acc.access_token = Some(a_tok.to_string());
                }
                if let Some(r_tok) = tok.get("refresh_token").and_then(|v| v.as_str()) {
                    acc.refresh_token = Some(r_tok.to_string());
                }
                acc.updated_at = Some(chrono::Utc::now().timestamp_millis() as f64 / 1000.0);
                let _ = save_accounts(&reg);
            }
        }
    }

    // Metadata
    let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
    let meta_file = account_session_dir(&clean).join("snapshot_meta.json");
    let meta = SessionSnapshotMeta {
        email: clean.clone(),
        snapshotted_at: now,
        has_desktop_session: copied_any,
        has_keyring_token: account_session_dir(&clean)
            .join("keyring_token.json")
            .is_file(),
        has_ide_session: account_session_dir(&clean)
            .join("ide_tokens.json")
            .is_file(),
    };
    if let Ok(content) = serde_json::to_string_pretty(&meta) {
        let _ = fs::write(meta_file, content);
    }

    copied_any
}

pub fn snapshot_ide_session(email: &str) -> bool {
    let ide_db = default_ide_state_db();
    if !ide_db.is_file() {
        return false;
    }

    let clean = email.trim().to_lowercase();

    // Verify token owner against existing database
    if let Some(tokens) = crate::auth::extract_tokens_from_state_db(&ide_db) {
        if let Ok(reg) = load_accounts() {
            let mut token_owner = None;
            for (em, acc) in &reg.accounts {
                if let Some(r_tok) = &acc.refresh_token
                    && r_tok == &tokens.refresh_token
                {
                    token_owner = Some(em.to_lowercase());
                    break;
                }
            }
            if let Some(owner) = token_owner
                && owner != clean
            {
                return false;
            }
        }

        if let Some(db_email) = crate::auth::extract_user_email_from_state_db(&ide_db)
            && db_email != clean
        {
            return false;
        }
    }

    let uri = format!("file:{}?mode=ro", ide_db.to_string_lossy());
    let Ok(conn) = Connection::open_with_flags(
        &uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    ) else {
        return false;
    };

    let Ok(mut stmt) = conn.prepare(
        "SELECT key, value FROM ItemTable WHERE key IN ('antigravityUnifiedStateSync.oauthToken', 'antigravityUnifiedStateSync.userStatus', 'antigravity.profileUrl')",
    ) else {
        return false;
    };

    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .ok();

    let mut saved = HashMap::new();
    if let Some(rows) = rows {
        for item in rows.flatten() {
            saved.insert(item.0, item.1);
        }
    }

    if saved.is_empty() {
        return false;
    }

    let ide_file = ensure_account_session_dir(&clean).join("ide_tokens.json");
    if let Ok(content) = serde_json::to_string_pretty(&saved) {
        let _ = fs::write(&ide_file, content);
    }

    // Persist tokens without clobbering fresher desktop keyring credentials.
    if let Some(tokens) = crate::auth::extract_tokens_from_state_db(&ide_db)
        && let Ok(mut reg) = load_accounts()
        && let Some(acc) = reg.accounts.get_mut(&clean)
    {
        if !tokens.access_token.is_empty() && acc.access_token.as_deref().is_none_or(str::is_empty)
        {
            acc.access_token = Some(tokens.access_token.clone());
        }
        if !tokens.refresh_token.is_empty()
            && acc.refresh_token.as_deref().is_none_or(str::is_empty)
        {
            acc.refresh_token = Some(tokens.refresh_token.clone());
        }
        let _ = save_accounts(&reg);
    }

    // Synthesize keyring token if missing
    let keyring_file = account_session_dir(&clean).join("keyring_token.json");
    if !keyring_file.is_file()
        && let Some(tokens) = crate::auth::extract_tokens_from_state_db(&ide_db)
        && !tokens.refresh_token.is_empty()
    {
        let token_obj = serde_json::json!({
            "token": {
                "access_token": tokens.access_token,
                "token_type": "Bearer",
                "refresh_token": tokens.refresh_token
            },
            "auth_method": "consumer",
            "id_token_email": clean
        });
        if let Ok(content) = serde_json::to_string_pretty(&token_obj) {
            let _ = fs::write(&keyring_file, content);
        }
    }

    // Metadata
    let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
    let meta_file = account_session_dir(&clean).join("snapshot_meta.json");
    let meta = SessionSnapshotMeta {
        email: clean.clone(),
        snapshotted_at: now,
        has_desktop_session: account_session_dir(&clean).join("desktop").is_dir(),
        has_keyring_token: account_session_dir(&clean)
            .join("keyring_token.json")
            .is_file(),
        has_ide_session: true,
    };
    if let Ok(content) = serde_json::to_string_pretty(&meta) {
        let _ = fs::write(meta_file, content);
    }

    true
}

pub async fn snapshot_account(email: &str) -> SnapshotResult {
    let clean = email.trim().to_lowercase();
    let desktop = snapshot_desktop_session(&clean).await;
    let ide = snapshot_ide_session(&clean);
    SnapshotResult {
        desktop_saved: desktop,
        ide_saved: ide,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionRestoreError {
    #[error("No credentials, saved session files, or refresh tokens found for '{0}'")]
    MissingCredentials(String),
    #[error("Keyring update failed: {0}")]
    Keyring(String),
    #[error("I/O error during session restoration: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse or serialize session JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("SQLite database error during IDE session restoration: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub(crate) fn update_app_storage_onboarding(storage_file: &Path, clean_email: &str) {
    if storage_file.is_file()
        && let Ok(content) = fs::read_to_string(storage_file)
        && let Ok(mut val) = serde_json::from_str::<Value>(&content)
    {
        val["agy.onboarding.lastLoginUsername"] = Value::String(clean_email.to_string());
        val["jetski.onboarding.lastLoginUsername"] = Value::String(clean_email.to_string());
        if let Ok(updated) = serde_json::to_string_pretty(&val) {
            let _ = fs::write(storage_file, updated);
        }
    }
}

fn format_rfc3339_expiry(expires_at: Option<f64>) -> Option<String> {
    let ts = expires_at?;
    if !ts.is_finite() || ts <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    let secs = ts.trunc() as i64;
    chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339())
}

fn account_recency_timestamp(acc: &crate::storage::AccountProfile) -> Option<f64> {
    match (acc.last_synced, acc.updated_at) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    }
}

fn file_mtime_secs(path: &Path) -> Option<f64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs_f64())
}

/// Builds the JSON payload to write into `org.freedesktop.secrets` for Antigravity Desktop.
///
/// Preserves existing `writer_id` / `auth_method` metadata from `keyring_token.json` when
/// available, ensures `id_token_email` matches `clean_email`, strips empty `expiry` strings
/// that fail Go RFC3339 parsing, and promotes fresher `refresh_token`/`access_token` values
/// from `accounts.json` when newer than the saved keyring snapshot.
#[must_use]
pub(crate) fn build_restore_keyring_payload(
    clean_email: &str,
    keyring_raw: Option<&str>,
    keyring_mtime: Option<f64>,
    account: Option<&crate::storage::AccountProfile>,
) -> Option<String> {
    if let Some(raw) = keyring_raw
        && let Ok(mut root) = serde_json::from_str::<Value>(raw)
        && root.is_object()
    {
        root["id_token_email"] = Value::String(clean_email.to_string());
        if root
            .get("auth_method")
            .and_then(|v| v.as_str())
            .is_none_or(|s| s.trim().is_empty())
        {
            root["auth_method"] = Value::String("consumer".to_string());
        }

        if let Some(acc) = account
            && let Some(acc_rt) = acc
                .refresh_token
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        {
            if !root.get("token").is_some_and(Value::is_object) {
                root["token"] = serde_json::json!({});
            }
            let existing_rt = root["token"]
                .get("refresh_token")
                .and_then(|v| v.as_str())
                .map_or("", str::trim);
            let acc_ts = account_recency_timestamp(acc);
            let account_is_newer = match (acc_ts, keyring_mtime) {
                (Some(a), Some(k)) => a > k,
                (Some(_), None) => true,
                _ => false,
            };

            if existing_rt.is_empty() || (acc_rt != existing_rt && account_is_newer) {
                root["token"]["refresh_token"] = Value::String(acc_rt.to_string());
                if let Some(acc_at) = acc
                    .access_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    root["token"]["access_token"] = Value::String(acc_at.to_string());
                }
                if let Some(expiry_rfc) = format_rfc3339_expiry(acc.expires_at) {
                    root["token"]["expiry"] = Value::String(expiry_rfc);
                }
            } else if acc_rt == existing_rt && account_is_newer {
                if let Some(acc_at) = acc
                    .access_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    root["token"]["access_token"] = Value::String(acc_at.to_string());
                }
                if let Some(expiry_rfc) = format_rfc3339_expiry(acc.expires_at) {
                    root["token"]["expiry"] = Value::String(expiry_rfc);
                }
            }
        }

        if let Some(tok_obj) = root.get_mut("token").and_then(Value::as_object_mut)
            && tok_obj.get("expiry").and_then(Value::as_str) == Some("")
        {
            tok_obj.remove("expiry");
        }

        return serde_json::to_string_pretty(&root).ok();
    }

    let acc = account?;
    let ref_tok = acc
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;

    let mut token_map = serde_json::Map::new();
    token_map.insert(
        "access_token".to_string(),
        Value::String(acc.access_token.clone().unwrap_or_default()),
    );
    token_map.insert(
        "token_type".to_string(),
        Value::String("Bearer".to_string()),
    );
    token_map.insert(
        "refresh_token".to_string(),
        Value::String(ref_tok.to_string()),
    );
    if let Some(expiry_rfc) = format_rfc3339_expiry(acc.expires_at) {
        token_map.insert("expiry".to_string(), Value::String(expiry_rfc));
    }

    let token_obj = serde_json::json!({
        "token": Value::Object(token_map),
        "auth_method": "consumer",
        "id_token_email": clean_email,
    });
    serde_json::to_string_pretty(&token_obj).ok()
}

pub(crate) async fn restore_desktop_session_in_dirs<SetFn, SetFut, ClearFn, ClearFut>(
    clean_email: &str,
    desktop_dir: &Path,
    source_dir: &Path,
    keyring_file: &Path,
    reg: &AccountsRegistry,
    set_secret: SetFn,
    clear_secret: ClearFn,
) -> Result<(), SessionRestoreError>
where
    SetFn: FnOnce(String) -> SetFut,
    SetFut: std::future::Future<Output = Result<(), crate::auth::KeyringError>>,
    ClearFn: FnOnce() -> ClearFut,
    ClearFut: std::future::Future<Output = Result<bool, crate::auth::KeyringError>>,
{
    let has_files = source_dir.is_dir();
    let has_keyring = keyring_file.is_file();
    let account = reg.accounts.get(clean_email);
    let has_refresh_token = account
        .and_then(|a| a.refresh_token.as_ref())
        .is_some_and(|r| !r.trim().is_empty());

    if !has_files && !has_keyring && !has_refresh_token {
        return Err(SessionRestoreError::MissingCredentials(
            clean_email.to_string(),
        ));
    }

    fs::create_dir_all(desktop_dir)?;

    if has_files {
        for &f in DESKTOP_FILES_TO_SAVE {
            let src = source_dir.join(f);
            if src.is_file() {
                let _ = fs::copy(&src, desktop_dir.join(f));
            }
        }

        for &d in DESKTOP_DIRS_TO_SAVE {
            let src = source_dir.join(d);
            if src.is_dir() {
                let dest = desktop_dir.join(d);
                let _ = fs::remove_dir_all(&dest);
                let _ = copy_dir_recursive(&src, &dest);
            }
        }

        update_app_storage_onboarding(&desktop_dir.join("app_storage.json"), clean_email);
    }

    let keyring_raw = if has_keyring {
        fs::read_to_string(keyring_file).ok()
    } else {
        None
    };
    let keyring_mtime = if has_keyring {
        file_mtime_secs(keyring_file)
    } else {
        None
    };

    if let Some(payload) =
        build_restore_keyring_payload(clean_email, keyring_raw.as_deref(), keyring_mtime, account)
    {
        set_secret(payload.clone())
            .await
            .map_err(|e| SessionRestoreError::Keyring(e.to_string()))?;

        if keyring_raw.as_deref() != Some(payload.as_str()) {
            if let Some(parent) = keyring_file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(keyring_file, &payload);
        }
    } else {
        clear_secret()
            .await
            .map_err(|e| SessionRestoreError::Keyring(e.to_string()))?;
    }

    Ok(())
}

pub async fn restore_desktop_session(email: &str) -> Result<(), SessionRestoreError> {
    let clean = email.trim().to_lowercase();
    let source_dir = account_session_dir(&clean).join("desktop");
    let keyring_file = account_session_dir(&clean).join("keyring_token.json");
    let desktop_dir = default_desktop_config_dir();
    let reg = load_accounts().unwrap_or_default();

    restore_desktop_session_in_dirs(
        &clean,
        &desktop_dir,
        &source_dir,
        &keyring_file,
        &reg,
        |raw| async move { crate::auth::set_desktop_keyring_secret(&raw).await },
        crate::auth::clear_desktop_keyring_secret,
    )
    .await
}

pub fn restore_ide_session(email: &str) -> Result<(), SessionRestoreError> {
    let clean = email.trim().to_lowercase();
    let ide_file = account_session_dir(&clean).join("ide_tokens.json");
    if !ide_file.is_file() {
        return Err(SessionRestoreError::MissingCredentials(clean));
    }

    let ide_db = default_ide_state_db();
    if let Some(parent) = ide_db.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = fs::read_to_string(&ide_file)?;
    let saved: HashMap<String, String> = serde_json::from_str(&content)?;

    let conn = Connection::open(&ide_db)?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )?;

    for (k, v) in saved {
        let _ = conn.execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
            [&k, &v],
        );
    }

    Ok(())
}

#[cfg(unix)]
fn is_zombie_process(pid: i32) -> bool {
    let stat_path = format!("/proc/{pid}/stat");
    let Ok(stat) = fs::read_to_string(stat_path) else {
        return false;
    };
    let Some(rparen) = stat.rfind(')') else {
        return false;
    };
    stat.as_bytes()
        .get(rparen + 2)
        .is_some_and(|&state| state == b'Z')
}

#[cfg(unix)]
fn is_process_alive(pid: i32) -> bool {
    !is_zombie_process(pid) && nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
}

#[cfg(not(unix))]
fn is_process_alive(pid: i32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

pub fn antigravity_pids() -> Vec<i32> {
    let mut s = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()),
    );
    s.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let my_pid = std::process::id() as usize;
    let mut pids = Vec::new();

    for (pid, process) in s.processes() {
        if process.status() == sysinfo::ProcessStatus::Zombie {
            continue;
        }

        let pid_u = pid.as_u32() as usize;
        if pid_u == my_pid {
            continue;
        }

        let name = process.name().to_string_lossy().to_lowercase();
        let cmd = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");

        // Ignore our own binaries / trackers
        if cmd.contains("antigravity-token-tracker") || cmd.contains("agy-token") {
            continue;
        }

        // Must not be the IDE (which has its own independent session)
        if cmd.contains("antigravity-ide") || name.contains("antigravity-ide") {
            continue;
        }

        // Match Antigravity desktop app or snap processes
        let is_desktop = cmd.contains("/snap/antigravity/")
            || cmd.contains("/antigravity")
            || name == "antigravity";

        if is_desktop {
            pids.push(pid.as_u32() as i32);
        }
    }

    pids.sort();
    pids.dedup();
    pids
}

#[inline]
pub fn get_antigravity_pids() -> Vec<i32> {
    antigravity_pids()
}

pub fn is_antigravity_running() -> bool {
    !antigravity_pids().is_empty()
}

pub fn stop_antigravity() -> bool {
    let pids = antigravity_pids();
    if !pids.is_empty() {
        for &p in &pids {
            #[cfg(unix)]
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(p),
                nix::sys::signal::Signal::SIGTERM,
            );
        }

        // Wait up to 4s
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(100));
            let remaining: Vec<i32> = pids
                .iter()
                .copied()
                .filter(|&p| is_process_alive(p))
                .collect();
            if remaining.is_empty() {
                break;
            }
        }

        // Force SIGKILL if needed
        for &p in &pids {
            if is_process_alive(p) {
                #[cfg(unix)]
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(p),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    // Clean up singleton locks
    let desktop_dir = default_desktop_config_dir();
    for lock_name in &["SingletonLock", "SingletonCookie", "SingletonSocket"] {
        let lock_path = desktop_dir.join(lock_name);
        let _ = fs::remove_file(lock_path);
    }

    true
}

pub fn launch_antigravity() -> bool {
    let bin_path = if Path::new("/snap/bin/antigravity").is_file() {
        PathBuf::from("/snap/bin/antigravity")
    } else {
        PathBuf::from("antigravity")
    };

    let mut cmd = Command::new(bin_path);
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    match cmd.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            true
        }
        Err(_) => false,
    }
}

pub fn restart_antigravity() -> bool {
    stop_antigravity();
    launch_antigravity()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SwitchSurface {
    #[default]
    Both,
    Desktop,
    Ide,
}

impl std::fmt::Display for SwitchSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Both => write!(f, "both"),
            Self::Desktop => write!(f, "desktop"),
            Self::Ide => write!(f, "ide"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error)]
#[error("unknown switch surface: '{0}', expected 'both', 'desktop', or 'ide'")]
pub struct ParseSwitchSurfaceError(pub String);

impl std::str::FromStr for SwitchSurface {
    type Err = ParseSwitchSurfaceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "both" | "all" => Ok(Self::Both),
            "desktop" | "app" => Ok(Self::Desktop),
            "ide" => Ok(Self::Ide),
            other => Err(ParseSwitchSurfaceError(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SwitchOutcome {
    AlreadyActive {
        email: String,
    },
    Switched {
        from: Option<String>,
        to: String,
        surface: SwitchSurface,
        restarted: bool,
    },
}

impl SwitchOutcome {
    #[must_use]
    pub fn is_already_active(&self) -> bool {
        matches!(self, Self::AlreadyActive { .. })
    }

    #[must_use]
    pub fn is_switched(&self) -> bool {
        matches!(self, Self::Switched { .. })
    }

    #[must_use]
    pub fn target_email(&self) -> &str {
        match self {
            Self::AlreadyActive { email } => email,
            Self::Switched { to, .. } => to,
        }
    }

    #[must_use]
    pub fn from_email(&self) -> Option<&str> {
        match self {
            Self::AlreadyActive { .. } => None,
            Self::Switched { from, .. } => from.as_deref(),
        }
    }
}

impl std::fmt::Display for SwitchOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyActive { email } => {
                write!(f, "Account '{email}' is already the active session.")
            }
            Self::Switched { to, .. } => {
                write!(f, "Successfully switched active session to '{to}'.")
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SwitchError {
    #[error("Account not found: {0}")]
    AccountNotFound(String),
    #[error("Failed to update credentials: {0}")]
    UpdateFailed(String),
    #[error("Session restoration error: {0}")]
    Restore(#[from] SessionRestoreError),
    #[error("I/O error during session switch: {0}")]
    Io(#[from] std::io::Error),
}

pub(crate) fn combine_surface_restore_results(
    surface: SwitchSurface,
    desktop_res: Option<Result<(), SessionRestoreError>>,
    ide_res: Option<Result<(), SessionRestoreError>>,
) -> Result<(bool, bool), SessionRestoreError> {
    match surface {
        SwitchSurface::Desktop => {
            if let Some(res) = desktop_res {
                res?;
            }
            Ok((true, false))
        }
        SwitchSurface::Ide => {
            if let Some(res) = ide_res {
                res?;
            }
            Ok((false, true))
        }
        SwitchSurface::Both => {
            let desktop_ok = match desktop_res {
                Some(Ok(())) => true,
                Some(Err(SessionRestoreError::MissingCredentials(_))) | None => false,
                Some(Err(err)) => return Err(err),
            };
            let ide_ok = match ide_res {
                Some(Ok(())) => true,
                Some(Err(SessionRestoreError::MissingCredentials(_))) | None => false,
                Some(Err(err)) => return Err(err),
            };
            if !desktop_ok && !ide_ok {
                return Err(SessionRestoreError::MissingCredentials(
                    "no desktop or IDE session credentials available".into(),
                ));
            }
            Ok((desktop_ok, ide_ok))
        }
    }
}

pub async fn switch_to_account(
    target_email: &str,
    restart: impl Into<RestartProcess>,
    surface: SwitchSurface,
) -> Result<SwitchOutcome, SwitchError> {
    let restart = restart.into().should_restart();
    let clean = target_email.trim().to_lowercase();
    let mut reg = load_accounts()
        .map_err(|e| SwitchError::UpdateFailed(format!("Failed to load accounts: {e}")))?;

    let current_active = reg.active_email.clone();
    if let Some(ref curr) = current_active {
        if curr.eq_ignore_ascii_case(&clean) {
            return Ok(SwitchOutcome::AlreadyActive { email: clean });
        }
        // Snapshot current
        let _ = snapshot_account(curr).await;
    }

    if !has_account_credentials(&clean, Some(&reg)) {
        return Err(SwitchError::AccountNotFound(format!(
            "No saved session or refresh token found for '{clean}'."
        )));
    }

    let running = is_antigravity_running();
    let restarted = restart && running;
    if restarted {
        let _ = tokio::task::spawn_blocking(stop_antigravity).await;
    }

    let do_desktop = matches!(surface, SwitchSurface::Both | SwitchSurface::Desktop);
    let do_ide = matches!(surface, SwitchSurface::Both | SwitchSurface::Ide);

    let desktop_res = if do_desktop {
        Some(restore_desktop_session(&clean).await)
    } else {
        None
    };
    let ide_res = if do_ide {
        Some(restore_ide_session(&clean))
    } else {
        None
    };

    let (restored_desktop, restored_ide) =
        combine_surface_restore_results(surface, desktop_res, ide_res)?;

    // Update active_email and surfaces in registry
    reg.active_email = Some(clean.clone());
    for (em, acc) in reg.accounts.iter_mut() {
        if em.eq_ignore_ascii_case(&clean) {
            acc.active = Some(true);
            if do_desktop && restored_desktop {
                acc.is_current_desktop_session = Some(true);
            }
            if do_ide && restored_ide {
                acc.is_current_ide_session = Some(true);
            }
        } else {
            acc.active = Some(false);
            if do_desktop && restored_desktop {
                acc.is_current_desktop_session = Some(false);
            }
            if do_ide && restored_ide {
                acc.is_current_ide_session = Some(false);
            }
        }
    }
    save_accounts(&reg)
        .map_err(|e| SwitchError::UpdateFailed(format!("Failed to save accounts: {e}")))?;

    if restarted {
        launch_antigravity();
    }

    Ok(SwitchOutcome::Switched {
        from: current_active,
        to: clean,
        surface,
        restarted,
    })
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let dest_child = dest.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_child)?;
        } else {
            let _ = fs::copy(entry.path(), dest_child);
        }
    }
    Ok(())
}
