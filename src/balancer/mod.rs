//! Balancer module for Antigravity sessions and accounts.

#[allow(clippy::module_inception)]
pub mod balancer;
pub mod models;
pub mod switcher;

pub use balancer::*;
pub use models::*;
pub use switcher::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balancer_config_defaults() {
        let cfg = BalancerConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.strategy, BalancerStrategy::Watermark);
        assert_eq!(cfg.watermark_spread_pct, 20.0);
        assert_eq!(cfg.round_robin_interval_minutes, 60);
        assert_eq!(cfg.expiry_window_hours, 36);
        assert_eq!(cfg.min_quota_pct, 5.0);
        assert_eq!(cfg.cooldown_seconds, 300);
        assert!(cfg.auto_restart);
    }

    #[test]
    fn test_balancer_strategy_display_and_from_str() {
        let strategies = [
            (BalancerStrategy::Watermark, "watermark"),
            (BalancerStrategy::ExpiryFirst, "expiry_first"),
            (BalancerStrategy::RoundRobin, "round_robin"),
            (BalancerStrategy::Reactive, "reactive"),
        ];

        for (variant, name) in strategies {
            assert_eq!(variant.to_string(), name);
            assert_eq!(name.parse::<BalancerStrategy>().unwrap(), variant);
        }

        // Aliases / case-insensitive
        assert_eq!(
            "expiry".parse::<BalancerStrategy>().unwrap(),
            BalancerStrategy::ExpiryFirst
        );
        assert_eq!(
            "roundrobin".parse::<BalancerStrategy>().unwrap(),
            BalancerStrategy::RoundRobin
        );
        assert_eq!(
            "WATERMARK".parse::<BalancerStrategy>().unwrap(),
            BalancerStrategy::Watermark
        );
        assert_eq!(
            "expiry-first".parse::<BalancerStrategy>().unwrap(),
            BalancerStrategy::ExpiryFirst
        );

        // Invalid strategy
        assert!("invalid_strat".parse::<BalancerStrategy>().is_err());
    }

    #[test]
    fn test_balancer_strategy_serde() {
        let serialized = serde_json::to_string(&BalancerStrategy::ExpiryFirst).unwrap();
        assert_eq!(serialized, "\"expiry_first\"");

        let deserialized: BalancerStrategy = serde_json::from_str("\"round_robin\"").unwrap();
        assert_eq!(deserialized, BalancerStrategy::RoundRobin);

        // Config JSON round-trip
        let json = r#"{"enabled":true,"strategy":"reactive"}"#;
        let cfg: BalancerConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.strategy, BalancerStrategy::Reactive);
    }

    #[test]
    fn test_watermark_strategy_evaluation() {
        let active = PoolAccount {
            email: "active@example.com".into(),
            quota_pct: 35.0,
            is_exhausted: false,
            reset_seconds: 300_000,
            is_active: true,
            is_eligible: true,
        };

        let candidate_low = PoolAccount {
            email: "low@example.com".into(),
            quota_pct: 45.0,
            is_exhausted: false,
            reset_seconds: 200_000,
            is_active: false,
            is_eligible: true,
        };

        let candidate_high = PoolAccount {
            email: "high@example.com".into(),
            quota_pct: 80.0,
            is_exhausted: false,
            reset_seconds: 400_000,
            is_active: false,
            is_eligible: true,
        };

        // 1. With spread threshold 50%, candidate_high has 80 - 35 = 45% spread (< 50%), so no rotation
        let decision_no = evaluate_watermark_strategy(
            Some(&active),
            &[candidate_low.clone(), candidate_high.clone()],
            50.0,
        );
        assert!(decision_no.is_none());

        // 2. With spread threshold 20%, candidate_high has 80 - 35 = 45% spread (>= 20%), rotates to candidate_high
        let decision_yes = evaluate_watermark_strategy(
            Some(&active),
            &[candidate_low, candidate_high.clone()],
            20.0,
        );
        assert!(decision_yes.is_some());
        let (chosen, reason) = decision_yes.unwrap();
        assert_eq!(chosen.email, candidate_high.email);
        assert!(reason.contains("watermark threshold"));
    }

    #[test]
    fn test_expiry_first_strategy_evaluation() {
        let active = PoolAccount {
            email: "active@example.com".into(),
            quota_pct: 50.0,
            is_exhausted: false,
            reset_seconds: 200_000,
            is_active: true,
            is_eligible: true,
        };

        let expiring_in_2h = PoolAccount {
            email: "expiring@example.com".into(),
            quota_pct: 70.0,
            is_exhausted: false,
            reset_seconds: 7200, // 2 hours
            is_active: false,
            is_eligible: true,
        };

        let later_candidate = PoolAccount {
            email: "later@example.com".into(),
            quota_pct: 90.0,
            is_exhausted: false,
            reset_seconds: 150_000,
            is_active: false,
            is_eligible: true,
        };

        // Expiry window 12 hours
        let decision = evaluate_expiry_first_strategy(
            Some(&active),
            &[later_candidate, expiring_in_2h.clone()],
            12,
            5.0,
        );
        assert!(decision.is_some());
        let (chosen, reason) = decision.unwrap();
        assert_eq!(chosen.email, expiring_in_2h.email);
        assert!(reason.contains("resets in 2.0h"));
    }

    #[test]
    fn test_round_robin_strategy_evaluation() {
        let acc1 = PoolAccount {
            email: "acc1@example.com".into(),
            quota_pct: 60.0,
            is_exhausted: false,
            reset_seconds: 100_000,
            is_active: true,
            is_eligible: true,
        };
        let acc2 = PoolAccount {
            email: "acc2@example.com".into(),
            quota_pct: 70.0,
            is_exhausted: false,
            reset_seconds: 100_000,
            is_active: false,
            is_eligible: true,
        };

        let now = 10000.0;
        let last_rotation = 5000.0; // 5000s elapsed (> 60m = 3600s)

        let decision = evaluate_round_robin_strategy(
            Some(&acc1),
            &[acc1.clone(), acc2.clone()],
            60,
            last_rotation,
            now,
        );
        assert!(decision.is_some());
        let (chosen, reason) = decision.unwrap();
        assert_eq!(chosen.email, acc2.email);
        assert!(reason.contains("Round-robin rotation window reached"));
    }

    #[test]
    fn test_launch_antigravity_helper() {
        // launch_antigravity attempts to spawn /snap/bin/antigravity or antigravity with process_group(0).
        // It returns a boolean and must execute without panicking regardless of whether the binary exists.
        let result = launch_antigravity();
        let _ = result;
    }

    #[test]
    fn test_switch_surface_display_and_parse() {
        assert_eq!(
            "desktop".parse::<SwitchSurface>().unwrap(),
            SwitchSurface::Desktop
        );
        assert_eq!("ide".parse::<SwitchSurface>().unwrap(), SwitchSurface::Ide);
        assert_eq!(
            "both".parse::<SwitchSurface>().unwrap(),
            SwitchSurface::Both
        );
        assert_eq!(SwitchSurface::Both.to_string(), "both");
        assert_eq!(SwitchSurface::Desktop.to_string(), "desktop");
        assert_eq!(SwitchSurface::Ide.to_string(), "ide");

        // Case-insensitive & trimming
        assert_eq!(
            "  DESKTOP  ".parse::<SwitchSurface>().unwrap(),
            SwitchSurface::Desktop
        );
        assert_eq!(
            "Both".parse::<SwitchSurface>().unwrap(),
            SwitchSurface::Both
        );
        assert_eq!(
            "app".parse::<SwitchSurface>().unwrap(),
            SwitchSurface::Desktop
        );
        assert_eq!("all".parse::<SwitchSurface>().unwrap(), SwitchSurface::Both);

        // Invalid
        assert!("invalid_surface".parse::<SwitchSurface>().is_err());
    }

    #[test]
    fn test_switch_error_display() {
        let err_not_found = SwitchError::AccountNotFound("test@example.com".into());
        assert_eq!(
            err_not_found.to_string(),
            "Account not found: test@example.com"
        );

        let err_update = SwitchError::UpdateFailed("disk read-only".into());
        assert_eq!(
            err_update.to_string(),
            "Failed to update credentials: disk read-only"
        );

        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err_io = SwitchError::Io(io_err);
        assert!(err_io.to_string().contains("access denied"));
    }

    #[test]
    fn test_switch_surface_serde() {
        let serialized = serde_json::to_string(&SwitchSurface::Desktop).unwrap();
        assert_eq!(serialized, "\"desktop\"");

        let deserialized: SwitchSurface = serde_json::from_str("\"ide\"").unwrap();
        assert_eq!(deserialized, SwitchSurface::Ide);

        let deserialized_both: SwitchSurface = serde_json::from_str("\"both\"").unwrap();
        assert_eq!(deserialized_both, SwitchSurface::Both);
    }

    #[tokio::test]
    async fn test_switch_to_account_not_found() {
        let res = switch_to_account(
            "nonexistent_email_12345@example.com",
            false,
            SwitchSurface::Both,
        )
        .await;
        assert!(res.is_err());
        match res.unwrap_err() {
            SwitchError::AccountNotFound(msg) => {
                assert!(msg.contains("nonexistent_email_12345@example.com"));
            }
            other => panic!("Expected AccountNotFound, got: {other:?}"),
        }
    }

    #[test]
    fn test_switch_outcome_methods_display_and_serde() {
        let active = SwitchOutcome::AlreadyActive {
            email: "active@example.com".to_string(),
        };
        assert!(active.is_already_active());
        assert!(!active.is_switched());
        assert_eq!(active.target_email(), "active@example.com");
        assert_eq!(active.from_email(), None);
        assert_eq!(
            active.to_string(),
            "Account 'active@example.com' is already the active session."
        );

        let switched = SwitchOutcome::Switched {
            from: Some("old@example.com".to_string()),
            to: "new@example.com".to_string(),
            surface: SwitchSurface::Both,
            restarted: true,
        };
        assert!(!switched.is_already_active());
        assert!(switched.is_switched());
        assert_eq!(switched.target_email(), "new@example.com");
        assert_eq!(switched.from_email(), Some("old@example.com"));
        assert_eq!(
            switched.to_string(),
            "Successfully switched active session to 'new@example.com'."
        );

        // Serde roundtrip
        let json = serde_json::to_string(&switched).unwrap();
        assert!(json.contains("\"status\":\"switched\""));
        assert!(json.contains("\"to\":\"new@example.com\""));
        let parsed: SwitchOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, switched);

        // Conversion to SwitchResult
        let result: SwitchResult = switched.into();
        assert!(result.success);
        assert_eq!(result.to_email, "new@example.com");
        assert_eq!(result.from_email, Some("old@example.com".to_string()));
    }

    #[test]
    fn test_session_restore_error_display() {
        let err = SessionRestoreError::MissingCredentials("missing@example.com".to_string());
        assert_eq!(
            err.to_string(),
            "No credentials, saved session files, or refresh tokens found for 'missing@example.com'"
        );
    }

    #[test]
    fn test_update_app_storage_onboarding_sets_agy_and_jetski_keys() {
        let dir = tempfile::tempdir().unwrap();
        let storage_path = dir.path().join("app_storage.json");
        std::fs::write(
            &storage_path,
            r#"{
  "agy.onboarding.lastLoginUsername": "old@example.com",
  "agy.homeGreeting.next": 0
}"#,
        )
        .unwrap();

        switcher::update_app_storage_onboarding(&storage_path, "new@example.com");

        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&storage_path).unwrap()).unwrap();
        assert_eq!(
            parsed["agy.onboarding.lastLoginUsername"].as_str(),
            Some("new@example.com")
        );
        assert_eq!(
            parsed["jetski.onboarding.lastLoginUsername"].as_str(),
            Some("new@example.com")
        );
        assert_eq!(parsed["agy.homeGreeting.next"].as_i64(), Some(0));
    }

    #[test]
    fn test_build_restore_keyring_payload_syncs_newer_account_and_sets_id_token_email() {
        let old_keyring = r#"{
  "token": {
    "access_token": "old-access",
    "token_type": "Bearer",
    "refresh_token": "1//old-rt",
    "expiry": ""
  },
  "auth_method": "consumer",
  "writer_id": "writer-123"
}"#;
        let newer_acc = crate::storage::AccountProfile {
            email: "target@example.com".into(),
            access_token: Some("new-access".into()),
            refresh_token: Some("1//new-rt".into()),
            expires_at: Some(1_800_000_000.0),
            updated_at: Some(2000.0),
            ..Default::default()
        };

        let payload = switcher::build_restore_keyring_payload(
            "target@example.com",
            Some(old_keyring),
            Some(1000.0),
            Some(&newer_acc),
        )
        .expect("keyring payload should be built");
        let parsed: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            parsed["id_token_email"].as_str(),
            Some("target@example.com")
        );
        assert_eq!(parsed["auth_method"].as_str(), Some("consumer"));
        assert_eq!(parsed["writer_id"].as_str(), Some("writer-123"));
        assert_eq!(parsed["token"]["refresh_token"].as_str(), Some("1//new-rt"));
        assert_eq!(parsed["token"]["access_token"].as_str(), Some("new-access"));
        assert!(
            parsed["token"]["expiry"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.contains('T'))
        );

        // Fallback when keyring_token.json is missing
        let fallback_acc = crate::storage::AccountProfile {
            email: "fallback@example.com".into(),
            access_token: Some("fb-access".into()),
            refresh_token: Some("1//fb-rt".into()),
            ..Default::default()
        };
        let fb_payload = switcher::build_restore_keyring_payload(
            "fallback@example.com",
            None,
            None,
            Some(&fallback_acc),
        )
        .expect("fallback payload should be built");
        let fb_parsed: serde_json::Value = serde_json::from_str(&fb_payload).unwrap();
        assert_eq!(
            fb_parsed["id_token_email"].as_str(),
            Some("fallback@example.com")
        );
        assert_eq!(fb_parsed["auth_method"].as_str(), Some("consumer"));
        assert!(fb_parsed["token"].get("expiry").is_none());
    }

    #[tokio::test]
    async fn test_restore_desktop_session_in_dirs_propagates_keyring_errors_and_updates_storage() {
        let tmp = tempfile::tempdir().unwrap();
        let desktop_dir = tmp.path().join("live_desktop");
        let source_dir = tmp.path().join("snapshot/desktop");
        let keyring_file = tmp.path().join("snapshot/keyring_token.json");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::write(
            source_dir.join("app_storage.json"),
            r#"{"agy.onboarding.lastLoginUsername":"stale@example.com"}"#,
        )
        .unwrap();
        std::fs::write(
            &keyring_file,
            r#"{"token":{"access_token":"ya29.a","refresh_token":"1//a"},"auth_method":"consumer","id_token_email":"target@example.com"}"#,
        )
        .unwrap();

        let reg = crate::storage::AccountsRegistry::default();

        // 1. Keyring failure must propagate as Err(SessionRestoreError::Keyring)
        let err = switcher::restore_desktop_session_in_dirs(
            "target@example.com",
            &desktop_dir,
            &source_dir,
            &keyring_file,
            &reg,
            |_payload| async {
                Err(crate::auth::KeyringError::InvalidPayload(
                    "simulated dbus failure".into(),
                ))
            },
            || async { Ok(false) },
        )
        .await
        .expect_err("keyring write failure must not be swallowed");
        assert!(matches!(err, SessionRestoreError::Keyring(_)));

        // 2. Successful keyring write updates both onboarding keys and writes secret
        let captured = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
        let captured_clone = std::sync::Arc::clone(&captured);
        switcher::restore_desktop_session_in_dirs(
            "target@example.com",
            &desktop_dir,
            &source_dir,
            &keyring_file,
            &reg,
            move |payload| async move {
                *captured_clone.lock().unwrap() = Some(payload);
                Ok(())
            },
            || async { Ok(false) },
        )
        .await
        .expect("restore should succeed when keyring write succeeds");

        let secret = captured.lock().unwrap().clone().expect("secret captured");
        assert!(secret.contains("\"id_token_email\": \"target@example.com\""));

        let live_storage: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(desktop_dir.join("app_storage.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            live_storage["agy.onboarding.lastLoginUsername"].as_str(),
            Some("target@example.com")
        );
        assert_eq!(
            live_storage["jetski.onboarding.lastLoginUsername"].as_str(),
            Some("target@example.com")
        );
    }

    #[test]
    fn test_combine_surface_restore_results_policy() {
        // Desktop-only account with Surface::Both succeeds when Desktop succeeds & IDE has MissingCredentials
        let ok_both = switcher::combine_surface_restore_results(
            SwitchSurface::Both,
            Some(Ok(())),
            Some(Err(SessionRestoreError::MissingCredentials("x".into()))),
        )
        .unwrap();
        assert_eq!(ok_both, (true, false));

        // Keyring error on Desktop with Surface::Both must fail even if IDE succeeded
        let err_keyring = switcher::combine_surface_restore_results(
            SwitchSurface::Both,
            Some(Err(SessionRestoreError::Keyring("InvalidArgs".into()))),
            Some(Ok(())),
        )
        .unwrap_err();
        assert!(matches!(err_keyring, SessionRestoreError::Keyring(_)));

        // Both MissingCredentials fails
        let err_both = switcher::combine_surface_restore_results(
            SwitchSurface::Both,
            Some(Err(SessionRestoreError::MissingCredentials("x".into()))),
            Some(Err(SessionRestoreError::MissingCredentials("x".into()))),
        )
        .unwrap_err();
        assert!(matches!(
            err_both,
            SessionRestoreError::MissingCredentials(_)
        ));
    }
}
