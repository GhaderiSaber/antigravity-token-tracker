//! Daemon, background notification, and auto-failover module.

#[allow(clippy::module_inception)]
pub mod daemon;
pub mod doctor;
pub mod failover;
pub mod models;
pub mod notifier;

pub use daemon::*;
pub use doctor::*;
pub use failover::*;
pub use models::*;
pub use notifier::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::models::{QuotaBucket, QuotaGroup};
    use std::collections::HashMap;
    use tempfile::tempdir;

    #[test]
    fn test_failover_state_defaults() {
        let state = FailoverState::default();
        assert_eq!(state.last_failover_timestamp, None);
        assert!(state.history.is_empty());
    }

    #[test]
    fn test_get_account_primary_quota() {
        let quota = crate::quota::AccountQuotaResult {
            email: "test@example.com".into(),
            name: "Test".into(),
            tier: "Pro".into(),
            is_current_ide_session: true,
            is_current_desktop_session: false,
            groups: vec![QuotaGroup {
                display_name: "Gemini 2.5 Pro".into(),
                description: None,
                weekly: Some(QuotaBucket {
                    remaining_percent: Some(78.5),
                    is_exhausted: Some(false),
                    ..Default::default()
                }),
                five_hour: None,
            }],
            models: vec![],
            checked_at: 1000.0,
            is_cached: false,
            error: None,
        };

        let (pct, exh) = get_account_primary_quota(&quota);
        assert_eq!(pct, 78.5);
        assert!(!exh);
    }

    #[test]
    fn test_record_token_history_snapshot() {
        let dir = tempdir().unwrap();
        let hist_file = dir.path().join("token_history.jsonl");

        let mut quotas = HashMap::new();
        quotas.insert(
            "test@example.com".into(),
            crate::quota::AccountQuotaResult {
                email: "test@example.com".into(),
                name: "Test".into(),
                tier: "Pro".into(),
                is_current_ide_session: true,
                is_current_desktop_session: false,
                groups: vec![QuotaGroup {
                    display_name: "Gemini 2.5 Pro".into(),
                    description: None,
                    weekly: Some(QuotaBucket {
                        remaining_percent: Some(65.0),
                        reset_time: Some("2026-11-28T00:00:00Z".into()),
                        ..Default::default()
                    }),
                    five_hour: Some(QuotaBucket {
                        remaining_percent: Some(95.0),
                        ..Default::default()
                    }),
                }],
                models: vec![],
                checked_at: 1000.0,
                is_cached: false,
                error: None,
            },
        );

        let res = record_token_history_snapshot(&quotas, Some(&hist_file));
        assert!(res.is_ok());
        assert!(hist_file.is_file());

        let content = std::fs::read_to_string(&hist_file).unwrap();
        assert!(content.contains("test@example.com"));
        assert!(content.contains("Gemini 2.5 Pro"));
        assert!(content.contains("\"weekly_pct\":65.0"));
        assert!(content.contains("\"5h_pct\":95.0"));
    }

    #[tokio::test]
    async fn test_run_daemon_loop_and_graceful_shutdown() {
        let (tx, rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn(async move { run_daemon(1, false, Some(rx)).await });
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        tx.send(true).unwrap();
        let res = handle.await.unwrap();
        assert!(res.is_ok());
    }
}
