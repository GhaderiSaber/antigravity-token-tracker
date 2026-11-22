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
    let Ok(connection) = zbus::Connection::session().await else {
        return false;
    };
    let Ok(service_proxy) = zbus::Proxy::new(
        &connection,
        "org.freedesktop.secrets",
        "/org/freedesktop/secrets",
        "org.freedesktop.Secret.Service",
    )
    .await
    else {
        return false;
    };

    let Ok((_empty, session_path)): Result<
        (zbus::zvariant::OwnedValue, zbus::zvariant::OwnedObjectPath),
        _,
    > = service_proxy.call("OpenSession", &("plain", "")).await
    else {
        return false;
    };

    let mut search_attrs = HashMap::new();
    search_attrs.insert("service", "gemini");
    search_attrs.insert("username", "antigravity");

    let Ok((unlocked, _locked)): Result<
        (
            Vec<zbus::zvariant::OwnedObjectPath>,
            Vec<zbus::zvariant::OwnedObjectPath>,
        ),
        _,
    > = service_proxy
        .call("SearchItems", &(search_attrs.clone(),))
        .await
    else {
        return false;
    };

    let secret_struct = (
        session_path,
        Vec::<u8>::new(),
        secret_json_str.as_bytes().to_vec(),
        "text/plain; charset=utf-8",
    );

    if !unlocked.is_empty() {
        let Ok(item_proxy) = zbus::Proxy::new(
            &connection,
            "org.freedesktop.secrets",
            unlocked[0].as_str(),
            "org.freedesktop.Secret.Item",
        )
        .await
        else {
            return false;
        };

        let res: Result<(), _> = item_proxy.call("SetSecret", &(secret_struct,)).await;
        res.is_ok()
    } else {
        let Ok(default_col): Result<zbus::zvariant::OwnedObjectPath, _> =
            service_proxy.call("ReadAlias", &("default",)).await
        else {
            return false;
        };
        let Ok(col_proxy) = zbus::Proxy::new(
            &connection,
            "org.freedesktop.secrets",
            default_col.as_str(),
            "org.freedesktop.Secret.Collection",
        )
        .await
        else {
            return false;
        };

        let mut item_props = HashMap::new();
        item_props.insert(
            "org.freedesktop.Secret.Item.Label",
            zbus::zvariant::Value::from("Password for 'antigravity' on 'gemini'"),
        );
        item_props.insert(
            "org.freedesktop.Secret.Item.Attributes",
            zbus::zvariant::Value::from(search_attrs),
        );

        let res: Result<
            (
                zbus::zvariant::OwnedObjectPath,
                zbus::zvariant::OwnedObjectPath,
            ),
            _,
        > = col_proxy
            .call("CreateItem", &(item_props, secret_struct, true))
            .await;
        res.is_ok()
    }
}

pub async fn clear_keyring_secret() -> bool {
    let Ok(connection) = zbus::Connection::session().await else {
        return false;
    };
    let Ok(service_proxy) = zbus::Proxy::new(
        &connection,
        "org.freedesktop.secrets",
        "/org/freedesktop/secrets",
        "org.freedesktop.Secret.Service",
    )
    .await
    else {
        return false;
    };

    let mut search_attrs = HashMap::new();
    search_attrs.insert("service", "gemini");
    search_attrs.insert("username", "antigravity");

    let Ok((unlocked, _locked)): Result<
        (
            Vec<zbus::zvariant::OwnedObjectPath>,
            Vec<zbus::zvariant::OwnedObjectPath>,
        ),
        _,
    > = service_proxy.call("SearchItems", &(search_attrs,)).await
    else {
        return false;
    };

    if unlocked.is_empty() {
        return false;
    }

    let Ok(item_proxy) = zbus::Proxy::new(
        &connection,
        "org.freedesktop.secrets",
        unlocked[0].as_str(),
        "org.freedesktop.Secret.Item",
    )
    .await
    else {
        return false;
    };

    let res: Result<zbus::zvariant::OwnedObjectPath, _> = item_proxy.call("Delete", &()).await;
    res.is_ok()
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

    // Persist tokens
    if let Some(tokens) = crate::auth::extract_tokens_from_state_db(&ide_db)
        && let Ok(mut reg) = load_accounts()
        && let Some(acc) = reg.accounts.get_mut(&clean)
    {
        if !tokens.access_token.is_empty() && acc.access_token.is_none() {
            acc.access_token = Some(tokens.access_token.clone());
        }
        if !tokens.refresh_token.is_empty() {
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
                "refresh_token": tokens.refresh_token,
                "expiry": ""
            },
            "auth_method": "OAUTH",
            "id_token": ""
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
    #[error("I/O error during session restoration: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse or serialize session JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("SQLite database error during IDE session restoration: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub async fn restore_desktop_session(email: &str) -> Result<(), SessionRestoreError> {
    let clean = email.trim().to_lowercase();
    let source_dir = account_session_dir(&clean).join("desktop");
    let keyring_file = account_session_dir(&clean).join("keyring_token.json");

    let has_files = source_dir.is_dir();
    let has_keyring = keyring_file.is_file();

    let reg = load_accounts().unwrap_or_default();
    let has_refresh_token = reg
        .accounts
        .get(&clean)
        .and_then(|a| a.refresh_token.as_ref())
        .is_some_and(|r| !r.is_empty());

    if !has_files && !has_keyring && !has_refresh_token {
        return Err(SessionRestoreError::MissingCredentials(clean));
    }

    let desktop_dir = default_desktop_config_dir();
    fs::create_dir_all(&desktop_dir)?;

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

        // Update app_storage.json
        let storage_file = desktop_dir.join("app_storage.json");
        if storage_file.is_file()
            && let Ok(content) = fs::read_to_string(&storage_file)
            && let Ok(mut val) = serde_json::from_str::<Value>(&content)
        {
            val["jetski.onboarding.lastLoginUsername"] = Value::String(clean.clone());
            if let Ok(updated) = serde_json::to_string_pretty(&val) {
                let _ = fs::write(&storage_file, updated);
            }
        }
    }

    // Restore keyring
    if has_keyring {
        if let Ok(raw) = fs::read_to_string(&keyring_file) {
            let _ = set_keyring_secret(&raw).await;
        }
    } else if let Some(acc) = reg.accounts.get(&clean)
        && let Some(ref_tok) = &acc.refresh_token
        && !ref_tok.is_empty()
    {
        let token_obj = serde_json::json!({
            "token": {
                "access_token": acc.access_token.clone().unwrap_or_default(),
                "token_type": "Bearer",
                "refresh_token": ref_tok,
                "expiry": ""
            },
            "auth_method": "OAUTH",
            "id_token": ""
        });
        if let Ok(raw) = serde_json::to_string_pretty(&token_obj) {
            let _ = set_keyring_secret(&raw).await;
            let _ = fs::write(&keyring_file, &raw);
        }
    } else {
        let _ = clear_keyring_secret().await;
    }

    Ok(())
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
fn is_process_alive(pid: i32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
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

    cmd.spawn().is_ok()
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

    let mut restored_desktop = false;
    let mut restored_ide = false;

    if do_desktop {
        restored_desktop = restore_desktop_session(&clean).await.is_ok();
    }
    if do_ide {
        restored_ide = restore_ide_session(&clean).is_ok();
    }

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
