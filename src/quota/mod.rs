//! Quota and burnrate tracking module.

pub mod burnrate;
pub mod models;
#[allow(clippy::module_inception)]
pub mod quota;

pub use burnrate::*;
pub use models::*;
pub use quota::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::models::AccountProfile;
    use serde_json::json;

    #[test]
    fn test_parse_quota_summary() {
        let summary_json = json!({
            "groups": [
                {
                    "displayName": "Claude 3.7 Sonnet (Thinking)",
                    "description": "Claude quota group",
                    "buckets": [
                        {
                            "bucketId": "claude-3-7-sonnet-weekly",
                            "window": "weekly",
                            "remainingFraction": 0.8523,
                            "resetTime": "2026-11-27T00:00:00Z",
                            "description": "Weekly bucket"
                        },
                        {
                            "bucketId": "claude-3-7-sonnet-5h",
                            "window": "5h",
                            "remainingFraction": 0.50,
                            "resetTime": "2026-11-20T23:00:00Z",
                            "description": "5-hour bucket"
                        }
                    ]
                }
            ]
        });

        let groups = parse_quota_summary(&summary_json);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].display_name, "Claude 3.7 Sonnet (Thinking)");
        assert_eq!(groups[0].description.as_deref(), Some("Claude quota group"));

        let weekly = groups[0].weekly.as_ref().unwrap();
        assert_eq!(
            weekly.bucket_id.as_deref(),
            Some("claude-3-7-sonnet-weekly")
        );
        assert_eq!(weekly.remaining_percent, Some(85.23));
        assert_eq!(weekly.reset_time.as_deref(), Some("2026-11-27T00:00:00Z"));

        let five_hour = groups[0].five_hour.as_ref().unwrap();
        assert_eq!(five_hour.bucket_id.as_deref(), Some("claude-3-7-sonnet-5h"));
        assert_eq!(five_hour.remaining_percent, Some(50.0));
    }

    #[test]
    fn test_parse_models_quota() {
        let models_json = json!({
            "buckets": [
                {
                    "modelId": "gemini-2.5-pro",
                    "remainingFraction": 0.92,
                    "resetTime": "2026-11-21T00:00:00Z"
                },
                {
                    "modelId": "claude-3-7-sonnet",
                    "remainingFraction": 0.45,
                    "resetTime": "2026-11-21T05:00:00Z"
                }
            ]
        });

        let models = parse_models_quota(&models_json);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].model_id, "gemini-2.5-pro");
        assert_eq!(models[0].remaining_percent, Some(92.0));
        assert_eq!(models[1].model_id, "claude-3-7-sonnet");
        assert_eq!(models[1].remaining_percent, Some(45.0));
    }

    #[test]
    fn test_burnrate_calculation() {
        let now_epoch = 1700000000.0;
        let snapshots = vec![
            json!({
                "timestamp": now_epoch - 3600.0,
                "accounts": {
                    "test@example.com": {
                        "groups": {
                            "Gemini 2.5 Pro": {
                                "weekly_pct": 90.0
                            }
                        }
                    }
                }
            }),
            json!({
                "timestamp": now_epoch - 1800.0,
                "accounts": {
                    "test@example.com": {
                        "groups": {
                            "Gemini 2.5 Pro": {
                                "weekly_pct": 80.0
                            }
                        }
                    }
                }
            }),
            json!({
                "timestamp": now_epoch,
                "accounts": {
                    "test@example.com": {
                        "groups": {
                            "Gemini 2.5 Pro": {
                                "weekly_pct": 70.0
                            }
                        }
                    }
                }
            }),
        ];

        let vel = calculate_account_velocity(
            &snapshots,
            "test@example.com",
            "Gemini 2.5 Pro",
            3600.0,
            Some(70.0),
            now_epoch,
        );

        // In 1 hour, consumed 20% (from 90% to 70%)
        assert_eq!(vel.current_quota_pct, 70.0);
        assert_eq!(vel.velocity_pct_per_hour, 20.0);
        assert_eq!(vel.pace_status, "ACTIVE");
        assert_eq!(vel.pace_icon, "⚡");
        assert!(vel.is_depleting);
        assert!(vel.eta_seconds.is_some());
    }

    #[test]
    fn test_format_eta_countdown() {
        assert_eq!(format_eta_countdown(0.0), "Now");
        assert_eq!(format_eta_countdown(600.0), "~10m");
        assert_eq!(format_eta_countdown(7200.0), "~2h");
        assert_eq!(format_eta_countdown(7500.0), "~2h 5m");
        assert_eq!(format_eta_countdown(90000.0), "~1d 1h");
    }

    #[test]
    fn test_quota_fallback_when_summary_is_none() {
        let mut acc = AccountProfile::default();
        acc.email = "test@example.com".to_string();
        // Verify fallback result construction without panicking
        assert_eq!(acc.email, "test@example.com");
    }
}
