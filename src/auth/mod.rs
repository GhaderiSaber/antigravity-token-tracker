//! Auth and credential discovery module.

pub mod keyring;
pub mod models;
pub mod oauth;
pub mod process_discovery;
pub mod sqlite;
pub mod sync;

pub use keyring::*;
pub use models::*;
pub use oauth::*;
pub use process_discovery::*;
pub use sqlite::*;
pub use sync::*;

// Backwards-compatibility aliases
pub use keyring as keyring_reader;
pub use oauth as credentials;
pub use sqlite as sqlite_reader;

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use rusqlite::Connection;
    use tempfile::tempdir;

    #[test]
    fn test_client_credentials_decoding() {
        assert!(!CLIENT_ID.is_empty());
        assert!(CLIENT_ID.contains(".apps.googleusercontent.com"));
        assert!(!CLIENT_SECRETS.is_empty());
        assert!(!CLIENT_SECRETS[0].is_empty());
    }

    #[test]
    fn test_sqlite_token_extraction() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("state.vscdb");

        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();

        // 1. Insert dummy oauthToken payload
        let raw_token_inner = b"dummy prefix ya29.a0TestAccessToken123456789012345678901234567890 mid 1//01TestRefreshToken12345678901234567890 end";
        let encoded_inner = BASE64.encode(raw_token_inner);
        let outer_payload = format!("some_header_{encoded_inner}_footer");
        let encoded_outer = BASE64.encode(outer_payload.as_bytes());

        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('antigravityUnifiedStateSync.oauthToken', ?)",
            [&encoded_outer],
        )
        .unwrap();

        // 2. Insert dummy userStatus payload with email and tier
        let raw_status = b"some binary data user@example.com and Google AI Pro tier info";
        let encoded_status = BASE64.encode(raw_status);

        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES ('antigravityUnifiedStateSync.userStatus', ?)",
            [&encoded_status],
        )
        .unwrap();

        // Verify token extraction
        let tokens = extract_tokens_from_state_db(&db_path).unwrap();
        assert!(tokens.access_token.starts_with("ya29."));
        assert!(tokens.refresh_token.starts_with("1//"));

        // Verify email extraction
        let email = extract_user_email_from_state_db(&db_path).unwrap();
        assert_eq!(email, "user@example.com");

        // Verify tier extraction
        let tier = extract_user_tier(&db_path);
        assert_eq!(tier, "Google AI Pro");
    }
}
