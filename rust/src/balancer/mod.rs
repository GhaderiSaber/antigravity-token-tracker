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
        assert_eq!(cfg.strategy, "watermark");
        assert_eq!(cfg.watermark_spread_pct, 20.0);
        assert_eq!(cfg.round_robin_interval_minutes, 60);
        assert_eq!(cfg.expiry_window_hours, 36);
        assert_eq!(cfg.min_quota_pct, 5.0);
        assert_eq!(cfg.cooldown_seconds, 300);
        assert!(cfg.auto_restart);
    }

    #[test]
    fn test_watermark_strategy_evaluation() {
        let active = PoolAccount {
            email: "active@example.com".into(),
            quota_pct: 35.0,
            is_exhausted: false,
            reset_seconds: 300000,
            is_active: true,
            is_eligible: true,
        };

        let candidate_low = PoolAccount {
            email: "low@example.com".into(),
            quota_pct: 45.0,
            is_exhausted: false,
            reset_seconds: 200000,
            is_active: false,
            is_eligible: true,
        };

        let candidate_high = PoolAccount {
            email: "high@example.com".into(),
            quota_pct: 80.0,
            is_exhausted: false,
            reset_seconds: 400000,
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
            reset_seconds: 200000,
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
            reset_seconds: 150000,
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
            reset_seconds: 100000,
            is_active: true,
            is_eligible: true,
        };
        let acc2 = PoolAccount {
            email: "acc2@example.com".into(),
            quota_pct: 70.0,
            is_exhausted: false,
            reset_seconds: 100000,
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
}
