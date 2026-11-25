pub mod lifecycle;
pub mod models;
#[allow(clippy::module_inception)]
pub mod storage;

pub use lifecycle::*;
pub use models::*;
pub use storage::*;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    #[test]
    fn test_format_countdown_and_status() {
        let fixed_now = Utc.with_ymd_and_hms(2026, 11, 20, 12, 0, 0).unwrap();

        // 1. More than a day
        let reset_ts = "2026-11-26T22:11:24Z";
        let (cd, secs) = format_countdown(reset_ts, fixed_now);
        assert!(cd.contains("6d"));
        assert!(secs > 500_000.0);

        // 2. A few hours
        let reset_5h = "2026-11-20T16:30:00Z";
        let (cd_5h, secs_5h) = format_countdown(reset_5h, fixed_now);
        assert_eq!(cd_5h, "4h 30m");
        assert_eq!(secs_5h, 16200.0);

        // 3. In the past / 0
        let past = "2026-11-20T10:00:00Z";
        let (cd_past, secs_past) = format_countdown(past, fixed_now);
        assert_eq!(cd_past, "Refreshed / Ready");
        assert_eq!(secs_past, 0.0);

        // 4. Bad string
        let (cd_bad, secs_bad) = format_countdown("invalid", fixed_now);
        assert_eq!(cd_bad, "N/A");
        assert_eq!(secs_bad, 0.0);

        // Status badges
        assert_eq!(get_status_badge(0.005).label, "EXHAUSTED / FINISHED");
        assert_eq!(get_status_badge(0.05).label, "CRITICAL");
        assert_eq!(get_status_badge(0.20).label, "LOW");
        assert_eq!(get_status_badge(0.40).label, "MODERATE");
        assert_eq!(get_status_badge(0.95).label, "HEALTHY");
    }

    #[test]
    fn test_storage_manager_crud_and_schema_compatibility() {
        let dir = tempdir().unwrap();
        let storage = StorageManager::new(dir.path());

        // Test loading non-existent accounts
        let empty_reg = storage.load_accounts().unwrap();
        assert!(empty_reg.accounts.is_empty());
        assert!(empty_reg.active_email.is_none());

        // Upsert first account
        let acc1 = AccountProfile {
            email: "Test1@Gmail.com".to_string(),
            name: Some("User One".to_string()),
            tier: Some("Google AI Pro".to_string()),
            refresh_token: Some("refresh_tok_1".to_string()),
            ..Default::default()
        };
        let saved1 = storage.upsert_account(acc1).unwrap();
        assert_eq!(saved1.email, "test1@gmail.com");

        // Verify retrieval
        let loaded1 = storage.get_account("test1@gmail.com").unwrap().unwrap();
        assert_eq!(loaded1.name.as_deref(), Some("User One"));
        assert_eq!(loaded1.refresh_token.as_deref(), Some("refresh_tok_1"));

        // Upsert update without erasing refresh_token
        let update_acc1 = AccountProfile {
            email: "test1@gmail.com".to_string(),
            name: Some("User One Renamed".to_string()),
            refresh_token: None, // Empty refresh token
            ..Default::default()
        };
        let updated1 = storage.upsert_account(update_acc1).unwrap();
        assert_eq!(updated1.name.as_deref(), Some("User One Renamed"));
        assert_eq!(updated1.refresh_token.as_deref(), Some("refresh_tok_1"));

        // Add second account
        let acc2 = AccountProfile {
            email: "test2@gmail.com".to_string(),
            name: Some("User Two".to_string()),
            ..Default::default()
        };
        storage.upsert_account(acc2).unwrap();

        let list = storage.list_accounts().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].email, "test1@gmail.com");
        assert_eq!(list[1].email, "test2@gmail.com");

        // Remove by 1-based index (alphabetical order: 1 = test1@gmail.com)
        let removed = storage.remove_account("1").unwrap().unwrap();
        assert_eq!(removed.email, "test1@gmail.com");
        assert_eq!(storage.list_accounts().unwrap().len(), 1);

        // Remove by partial name
        let removed2 = storage.remove_account("two").unwrap().unwrap();
        assert_eq!(removed2.email, "test2@gmail.com");
        assert_eq!(storage.list_accounts().unwrap().len(), 0);
    }

    #[test]
    fn test_v1_and_v2_json_deserialization() {
        // V1 format: flat map
        let v1_json = r#"{
            "user@test.com": {
                "email": "user@test.com",
                "name": "Legacy User",
                "tier": "Standard"
            }
        }"#;
        let reg_v1: AccountsRegistry = serde_json::from_str(v1_json).unwrap();
        assert!(reg_v1.active_email.is_none());
        assert_eq!(reg_v1.accounts.len(), 1);
        assert_eq!(
            reg_v1.accounts["user@test.com"].name.as_deref(),
            Some("Legacy User")
        );

        // V2 format: object with active_email
        let v2_json = r#"{
            "active_email": "active@test.com",
            "accounts": {
                "active@test.com": {
                    "email": "active@test.com",
                    "name": "Modern User"
                }
            }
        }"#;
        let reg_v2: AccountsRegistry = serde_json::from_str(v2_json).unwrap();
        assert_eq!(reg_v2.active_email.as_deref(), Some("active@test.com"));
        assert_eq!(
            reg_v2.accounts["active@test.com"].name.as_deref(),
            Some("Modern User")
        );
    }

    #[test]
    fn test_default_config_dir_uses_xdg_config_dir() {
        let expected = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from(".config"))
            .join("antigravity-token-tracker");
        assert_eq!(default_config_dir(), expected);
    }

    #[test]
    fn test_concurrent_upsert_accounts_preserves_all() {
        let dir = tempdir().unwrap();
        let worker_count = 16usize;

        std::thread::scope(|s| {
            for i in 0..worker_count {
                let path = dir.path();
                s.spawn(move || {
                    let storage = StorageManager::new(path);
                    let profile = AccountProfile {
                        email: format!("User{i}@Example.com"),
                        name: Some(format!("User {i}")),
                        refresh_token: Some(format!("1//refresh-{i}")),
                        ..Default::default()
                    };
                    storage.upsert_account(profile).unwrap();
                });
            }
        });

        let storage = StorageManager::new(dir.path());
        assert!(
            dir.path().join(".accounts.lock").exists(),
            "expected advisory lockfile .accounts.lock to exist"
        );
        let accounts = storage.list_accounts().unwrap();
        assert_eq!(
            accounts.len(),
            worker_count,
            "all concurrent upserts should be preserved"
        );
        for i in 0..worker_count {
            let email = format!("user{i}@example.com");
            let found = accounts
                .iter()
                .find(|a| a.email == email)
                .unwrap_or_else(|| panic!("missing upserted account {email}"));
            assert_eq!(found.name.as_deref(), Some(format!("User {i}").as_str()));
            assert_eq!(
                found.refresh_token.as_deref(),
                Some(format!("1//refresh-{i}").as_str())
            );
        }
    }

    #[test]
    fn test_concurrent_upsert_remove_and_load_consistency() {
        let dir = tempdir().unwrap();
        let storage = StorageManager::new(dir.path());
        for i in 0..4 {
            storage
                .upsert_account(AccountProfile {
                    email: format!("seed{i}@example.com"),
                    ..Default::default()
                })
                .unwrap();
        }

        std::thread::scope(|s| {
            for i in 0..8 {
                let path = dir.path();
                s.spawn(move || {
                    let mgr = StorageManager::new(path);
                    mgr.upsert_account(AccountProfile {
                        email: format!("added{i}@example.com"),
                        ..Default::default()
                    })
                    .unwrap();
                });
            }
            for i in 0..2 {
                let path = dir.path();
                s.spawn(move || {
                    let mgr = StorageManager::new(path);
                    let _ = mgr.remove_account(&format!("seed{i}@example.com")).unwrap();
                });
            }
            for _ in 0..4 {
                let path = dir.path();
                s.spawn(move || {
                    let mgr = StorageManager::new(path);
                    let _ = mgr.load_accounts().unwrap();
                });
            }
        });

        let accounts = storage.list_accounts().unwrap();
        assert_eq!(accounts.len(), 10);
        for i in 2..4 {
            assert!(
                accounts
                    .iter()
                    .any(|a| a.email == format!("seed{i}@example.com"))
            );
        }
        for i in 0..8 {
            assert!(
                accounts
                    .iter()
                    .any(|a| a.email == format!("added{i}@example.com"))
            );
        }
    }
}
