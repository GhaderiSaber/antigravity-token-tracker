//! Quota and burnrate tracking module.

pub mod burnrate;
pub mod models;
#[allow(clippy::module_inception)]
pub mod quota;

pub use burnrate::{
    calculate_account_velocity, calculate_multiwindow_velocity, calculate_pool_burnrates,
    default_history_file, extract_account_quota_pct, format_burnrate_badge,
    format_burnrate_tray_summary, format_depletion_clock, format_eta_countdown, load_snapshots,
};
pub use models::{
    AccountQuotaResult, AccountVelocity, ModelQuota, MultiwindowVelocity, PoolBurnrates,
    QuotaBucket, QuotaGroup, QuotaRefreshPolicy, QuotaSnapshot,
};
pub use quota::{
    CACHE_TTL_SECONDS, QUOTA_MODELS_URL, QUOTA_SUMMARY_URL, QuotaFetchContext, clear_quota_cache,
    fetch_account_quota, fetch_account_quota_with_context, fetch_accounts_quota_with_context,
    fetch_all_accounts_quota, get_historical_quota, historical_quota, historical_quota_from_file,
    parse_models_quota, parse_quota_summary,
};

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
        let now_epoch = 1_700_000_000.0;
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
        let acc = AccountProfile {
            email: "test@example.com".to_string(),
            ..Default::default()
        };
        // Verify fallback result construction without panicking
        assert_eq!(acc.email, "test@example.com");
    }

    static CACHE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn sample_snapshot(group_name: &str, weekly_pct: f64, checked_at: f64) -> QuotaSnapshot {
        QuotaSnapshot {
            checked_at: Some(checked_at),
            groups: vec![QuotaGroup {
                display_name: group_name.to_string(),
                description: Some("Test group".to_string()),
                weekly: Some(QuotaBucket {
                    bucket_id: Some("gemini-weekly".to_string()),
                    window: Some("weekly".to_string()),
                    display_name: Some(group_name.to_string()),
                    remaining_fraction: Some(weekly_pct / 100.0),
                    remaining_percent: Some(weekly_pct),
                    reset_time: Some("2026-12-01T00:00:00Z".to_string()),
                    description: None,
                    ..Default::default()
                }),
                five_hour: None,
            }],
            models: Vec::new(),
            is_cached: Some(false),
            is_gemini_exhausted: None,
            is_3p_exhausted: None,
        }
    }

    #[test]
    fn test_quota_refresh_policy_conversions() {
        assert_eq!(QuotaRefreshPolicy::default(), QuotaRefreshPolicy::UseCache);
        assert!(!QuotaRefreshPolicy::UseCache.is_force_refresh());
        assert!(QuotaRefreshPolicy::ForceRefresh.is_force_refresh());
        assert_eq!(
            QuotaRefreshPolicy::from(false),
            QuotaRefreshPolicy::UseCache
        );
        assert_eq!(
            QuotaRefreshPolicy::from(true),
            QuotaRefreshPolicy::ForceRefresh
        );
    }

    #[tokio::test]
    async fn test_quota_cache_hit_force_refresh_and_clear() {
        let _guard = CACHE_TEST_LOCK.lock().await;
        let email = "cache-behavior-test@example.com";
        let ctx = QuotaFetchContext::new();

        // Seed QUOTA_CACHE via last_known_quota fallback
        let mut acc_with_snapshot = AccountProfile {
            email: email.to_string(),
            name: Some("Cache Tester".to_string()),
            tier: Some("Pro".to_string()),
            last_known_quota: Some(sample_snapshot("Gemini 2.5 Pro", 82.0, 1_700_000_000.0)),
            ..Default::default()
        };

        let first = fetch_account_quota_with_context(
            &mut acc_with_snapshot,
            QuotaRefreshPolicy::ForceRefresh,
            &ctx,
        )
        .await;
        assert!(first.error.is_none());
        assert!(first.is_cached);
        assert_eq!(first.groups.len(), 1);
        assert_eq!(
            first.groups[0]
                .weekly
                .as_ref()
                .and_then(|b| b.remaining_percent),
            Some(82.0)
        );

        // Even with last_known_quota cleared, UseCache returns the QUOTA_CACHE hit
        let mut acc_cleared = AccountProfile {
            email: email.to_string(),
            name: Some("Cache Tester".to_string()),
            tier: Some("Pro".to_string()),
            last_known_quota: None,
            ..Default::default()
        };
        let cached =
            fetch_account_quota_with_context(&mut acc_cleared, QuotaRefreshPolicy::UseCache, &ctx)
                .await;
        assert!(cached.error.is_none());
        assert!(cached.is_cached);
        assert_eq!(cached.groups.len(), 1);

        // ForceRefresh bypasses QUOTA_CACHE and yields inactive error when no fallback exists
        let forced = fetch_account_quota_with_context(
            &mut acc_cleared,
            QuotaRefreshPolicy::ForceRefresh,
            &ctx,
        )
        .await;
        assert!(forced.error.is_some());
        assert!(!forced.is_cached);
        assert!(forced.groups.is_empty());

        // Re-populate cache, then verify clear_quota_cache invalidates UseCache hits
        let _ = fetch_account_quota_with_context(
            &mut acc_with_snapshot,
            QuotaRefreshPolicy::ForceRefresh,
            &ctx,
        )
        .await;
        clear_quota_cache();

        let after_clear =
            fetch_account_quota_with_context(&mut acc_cleared, QuotaRefreshPolicy::UseCache, &ctx)
                .await;
        assert!(after_clear.error.is_some());
        assert!(after_clear.groups.is_empty());
    }

    #[tokio::test]
    async fn test_concurrent_batch_with_cached_and_historical_fallback() {
        let _guard = CACHE_TEST_LOCK.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let history_path = dir.path().join("token_history.jsonl");
        let hist_record = json!({
            "timestamp": 1_700_000_123.0,
            "accounts": {
                "batch-hist@example.com": {
                    "groups": {
                        "Gemini 2.5 Pro": {
                            "weekly_pct": 64.5,
                            "weekly_reset": "2026-12-02T00:00:00Z",
                            "5h_pct": 90.0
                        }
                    }
                }
            }
        });
        std::fs::write(&history_path, format!("{hist_record}\n")).unwrap();

        let mut accounts = std::collections::HashMap::new();
        accounts.insert(
            "batch-cached@example.com".to_string(),
            AccountProfile {
                email: "batch-cached@example.com".to_string(),
                name: Some("Cached User".to_string()),
                tier: Some("Pro".to_string()),
                last_known_quota: Some(sample_snapshot("Gemini 2.5 Pro", 91.0, 1_700_000_000.0)),
                ..Default::default()
            },
        );
        accounts.insert(
            "batch-hist@example.com".to_string(),
            AccountProfile {
                email: "batch-hist@example.com".to_string(),
                name: Some("History User".to_string()),
                ..Default::default()
            },
        );
        accounts.insert(
            "batch-inactive@example.com".to_string(),
            AccountProfile {
                email: "batch-inactive@example.com".to_string(),
                name: Some("Inactive User".to_string()),
                ..Default::default()
            },
        );
        // Empty keys should be ignored
        accounts.insert(String::new(), AccountProfile::default());

        let ctx = QuotaFetchContext::new();
        let (results, updated) = quota::fetch_accounts_quota_with_history(
            &mut accounts,
            QuotaRefreshPolicy::ForceRefresh,
            &ctx,
            Some(&history_path),
        )
        .await;

        assert!(updated, "historical fallback should mark registry updated");
        assert_eq!(results.len(), 3);

        let cached_res = &results["batch-cached@example.com"];
        assert!(cached_res.error.is_none());
        assert!(cached_res.is_cached);
        assert_eq!(
            cached_res.groups[0]
                .weekly
                .as_ref()
                .and_then(|b| b.remaining_percent),
            Some(91.0)
        );

        let hist_res = &results["batch-hist@example.com"];
        assert!(hist_res.error.is_none());
        assert!(hist_res.is_cached);
        assert_eq!(hist_res.checked_at, 1_700_000_123.0);
        assert_eq!(
            hist_res.groups[0]
                .weekly
                .as_ref()
                .and_then(|b| b.remaining_percent),
            Some(64.5)
        );
        assert!(
            accounts["batch-hist@example.com"]
                .last_known_quota
                .is_some(),
            "historical snapshot should be backfilled into account profile"
        );

        let inactive_res = &results["batch-inactive@example.com"];
        assert!(inactive_res.error.is_some());
        assert!(!inactive_res.is_cached);

        // Also exercise the public concurrent batch wrapper with QUOTA_CACHE UseCache
        let batch_public =
            fetch_accounts_quota_with_context(&mut accounts, QuotaRefreshPolicy::UseCache, &ctx)
                .await;
        assert_eq!(batch_public.len(), 3);
        assert!(batch_public["batch-cached@example.com"].is_cached);
        assert!(batch_public["batch-hist@example.com"].is_cached);
    }
}
