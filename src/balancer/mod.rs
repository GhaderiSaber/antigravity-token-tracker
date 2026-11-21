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
}
