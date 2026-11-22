use super::models::OAuthTokens;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use regex::bytes::Regex as ByteRegex;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static CHUNK_RE_40: LazyLock<ByteRegex> = LazyLock::new(|| {
    ByteRegex::new(r"[A-Za-z0-9+/=]{40,}").expect("valid 40-char chunk regex pattern")
});
static CHUNK_RE_20: LazyLock<ByteRegex> = LazyLock::new(|| {
    ByteRegex::new(r"[A-Za-z0-9+/=]{20,}").expect("valid 20-char chunk regex pattern")
});
static ACC_RE: LazyLock<ByteRegex> = LazyLock::new(|| {
    ByteRegex::new(r"ya29\.[a-zA-Z0-9_-]+").expect("valid access token regex pattern")
});
static REF_RE: LazyLock<ByteRegex> = LazyLock::new(|| {
    ByteRegex::new(r"1//[a-zA-Z0-9_-]+").expect("valid refresh token regex pattern")
});
static EMAIL_RE: LazyLock<ByteRegex> = LazyLock::new(|| {
    ByteRegex::new(
        r#"(?:^|[\x00-\x20":<>,'\(\)\[\]])([a-zA-Z0-9][a-zA-Z0-9._%+-]*@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,})"#,
    )
    .expect("valid email regex pattern")
});

#[must_use]
pub fn default_ide_state_db() -> PathBuf {
    dirs::home_dir().map_or_else(
        || PathBuf::from(".config/Antigravity IDE/User/globalStorage/state.vscdb"),
        |h| h.join(".config/Antigravity IDE/User/globalStorage/state.vscdb"),
    )
}

pub fn extract_tokens_from_state_db(db_path: &Path) -> Option<OAuthTokens> {
    if !db_path.exists() {
        return None;
    }

    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let mut stmt = conn
        .prepare("SELECT value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.oauthToken'")
        .ok()?;

    let row_val: String = stmt.query_row([], |row| row.get(0)).ok()?;
    let raw1 = BASE64.decode(row_val.trim()).ok()?;

    let mut access_token = None;
    let mut refresh_token = None;

    for m in CHUNK_RE_40.find_iter(&raw1) {
        if let Ok(dec) = BASE64.decode(m.as_bytes()) {
            if access_token.is_none()
                && let Some(m_acc) = ACC_RE.find(&dec)
                && let Ok(s) = std::str::from_utf8(m_acc.as_bytes())
            {
                access_token = Some(s.to_string());
            }
            if refresh_token.is_none()
                && let Some(m_ref) = REF_RE.find(&dec)
                && let Ok(s) = std::str::from_utf8(m_ref.as_bytes())
            {
                refresh_token = Some(s.to_string());
            }
        }
    }

    if access_token.is_some() || refresh_token.is_some() {
        Some(OAuthTokens {
            access_token: access_token.unwrap_or_default(),
            refresh_token: refresh_token.unwrap_or_default(),
        })
    } else {
        None
    }
}

pub fn extract_user_email_from_state_db(db_path: &Path) -> Option<String> {
    if !db_path.exists() {
        return None;
    }

    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let mut stmt = conn
        .prepare("SELECT value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.userStatus'")
        .ok()?;

    let row_val: String = stmt.query_row([], |row| row.get(0)).ok()?;
    let raw = BASE64.decode(row_val.trim()).ok()?;

    if let Some(cap) = EMAIL_RE.captures(&raw)
        && let Some(m) = cap.get(1)
        && let Ok(s) = std::str::from_utf8(m.as_bytes())
    {
        return Some(s.trim().to_lowercase());
    }

    // Try nested base64 chunks
    for ch in CHUNK_RE_20.find_iter(&raw) {
        if let Ok(dec) = BASE64.decode(ch.as_bytes())
            && let Some(cap) = EMAIL_RE.captures(&dec)
            && let Some(m) = cap.get(1)
            && let Ok(s) = std::str::from_utf8(m.as_bytes())
        {
            return Some(s.trim().to_lowercase());
        }
    }

    None
}

pub fn extract_user_tier(db_path: &Path) -> String {
    if !db_path.exists() {
        return "Unknown".to_string();
    }

    let Ok(conn) = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return "Standard".to_string();
    };

    let Ok(mut stmt) = conn.prepare(
        "SELECT value FROM ItemTable WHERE key = 'antigravityUnifiedStateSync.userStatus'",
    ) else {
        return "Standard".to_string();
    };

    let Ok(row_val) = stmt.query_row([], |row| row.get::<_, String>(0)) else {
        return "Standard".to_string();
    };

    let Ok(raw) = BASE64.decode(row_val.trim()) else {
        return "Standard".to_string();
    };

    let s = String::from_utf8_lossy(&raw);
    if s.contains("Google AI Ultra") {
        "Google AI Ultra".to_string()
    } else if s.contains("Google AI Pro") || s.contains("g1-pro-tier") {
        "Google AI Pro".to_string()
    } else if s.contains("g1-") {
        "Google One".to_string()
    } else {
        "Standard".to_string()
    }
}
