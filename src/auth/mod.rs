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

    #[test]
    fn test_read_desktop_last_login_username_supports_agy_and_jetski_keys() {
        let dir = tempdir().unwrap();
        let agy_path = dir.path().join("app_storage_agy.json");
        std::fs::write(
            &agy_path,
            r#"{"agy.onboarding.lastLoginUsername": "User@Example.COM "}"#,
        )
        .unwrap();
        assert_eq!(
            read_desktop_last_login_username(&agy_path).as_deref(),
            Some("user@example.com")
        );

        let jetski_path = dir.path().join("app_storage_jetski.json");
        std::fs::write(
            &jetski_path,
            r#"{"jetski.onboarding.lastLoginUsername": "legacy@example.com"}"#,
        )
        .unwrap();
        assert_eq!(
            read_desktop_last_login_username(&jetski_path).as_deref(),
            Some("legacy@example.com")
        );

        let both_path = dir.path().join("app_storage_both.json");
        std::fs::write(
            &both_path,
            r#"{"jetski.onboarding.lastLoginUsername": "legacy@example.com", "agy.onboarding.lastLoginUsername": "New@Example.COM"}"#,
        )
        .unwrap();
        assert_eq!(
            read_desktop_last_login_username(&both_path).as_deref(),
            Some("new@example.com")
        );

        let empty_path = dir.path().join("app_storage_empty.json");
        std::fs::write(
            &empty_path,
            r#"{"agy.onboarding.lastLoginUsername": "   ", "jetski.onboarding.lastLoginUsername": ""}"#,
        )
        .unwrap();
        assert_eq!(read_desktop_last_login_username(&empty_path), None);

        let missing_path = dir.path().join("does_not_exist.json");
        assert_eq!(read_desktop_last_login_username(&missing_path), None);
    }

    #[test]
    fn test_is_desktop_language_server_cmdline_v235() {
        let v235_cmd = "/snap/antigravity/80/opt/antigravity/resources/bin/language_server --standalone --override_ide_name antigravity --subclient_type hub --app_data_dir antigravity --read_host_bridge_token_from_stdin";
        assert!(is_desktop_language_server_cmdline(v235_cmd));

        let ide_cmd = "/snap/antigravity-ide/bin/language_server --subclient_type ide --app_data_dir antigravity-ide";
        assert!(!is_desktop_language_server_cmdline(ide_cmd));

        let non_ls_cmd = "/usr/bin/antigravity --app_data_dir antigravity";
        assert!(!is_desktop_language_server_cmdline(non_ls_cmd));
    }

    #[test]
    fn test_resolve_desktop_session_email_priority_and_normalization() {
        assert_eq!(
            resolve_desktop_session_email(
                " Live@Example.COM ",
                Some("keyring@example.com"),
                Some("storage@example.com"),
            )
            .as_deref(),
            Some("live@example.com")
        );
        assert_eq!(
            resolve_desktop_session_email(
                "   ",
                Some(" Keyring@Example.COM "),
                Some("storage@example.com"),
            )
            .as_deref(),
            Some("keyring@example.com")
        );
        assert_eq!(
            resolve_desktop_session_email("", Some("  "), Some(" Storage@Example.COM ")).as_deref(),
            Some("storage@example.com")
        );
        assert_eq!(resolve_desktop_session_email("", None, Some("  ")), None);
    }

    #[test]
    fn test_apply_desktop_session_metadata_preserves_port_and_csrf_when_none() {
        let mut profile = crate::storage::AccountProfile {
            email: "user@example.com".into(),
            name: Some("Existing Name".into()),
            tier: Some("Pro".into()),
            app_port: Some(43123),
            app_csrf_token: Some("existing-csrf".into()),
            ..Default::default()
        };

        let headless_session = DiscoveredSession {
            email: "user@example.com".into(),
            name: None,
            port: None,
            csrf_token: None,
            tier: None,
            surface: Some("Antigravity Desktop App".into()),
            pid: Some(1234),
        };
        sync::apply_desktop_session_metadata(&mut profile, &headless_session);
        assert_eq!(profile.name.as_deref(), Some("Existing Name"));
        assert_eq!(profile.tier.as_deref(), Some("Pro"));
        assert_eq!(profile.app_port, Some(43123));
        assert_eq!(profile.app_csrf_token.as_deref(), Some("existing-csrf"));

        let full_session = DiscoveredSession {
            email: "user@example.com".into(),
            name: Some("Updated Name".into()),
            port: Some(44000),
            csrf_token: Some("new-csrf".into()),
            tier: Some("Ultra".into()),
            surface: Some("Antigravity Desktop App".into()),
            pid: Some(1234),
        };
        sync::apply_desktop_session_metadata(&mut profile, &full_session);
        assert_eq!(profile.name.as_deref(), Some("Updated Name"));
        assert_eq!(profile.tier.as_deref(), Some("Ultra"));
        assert_eq!(profile.app_port, Some(44000));
        assert_eq!(profile.app_csrf_token.as_deref(), Some("new-csrf"));
    }
}
