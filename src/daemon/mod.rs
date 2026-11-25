//! Daemon, background notification, and auto-failover module.

#[allow(clippy::module_inception)]
pub mod daemon;
pub mod doctor;
pub mod failover;
pub mod models;
pub mod notifier;

pub use daemon::{
    DaemonError, DaemonRunOptions, FailoverRestart, default_token_history_file,
    record_token_history_snapshot, run_daemon, run_daemon_step, run_daemon_step_with_options,
    run_daemon_step_with_snapshot, run_daemon_with_options, run_daemon_with_snapshot,
};
pub use doctor::{build_geo_shield_diagnostic, compute_overall_status, run_doctor_suite};
pub use failover::{
    account_primary_quota, active_account, active_sessions, default_failover_state_file,
    evaluate_and_execute_failover, failover_status, find_best_failover_candidate,
    get_account_primary_quota, get_active_account, get_active_sessions, get_failover_status,
    load_failover_state, save_failover_state,
};
pub use models::{
    DaemonTickSnapshot, DiagnosticItem, DiagnosticStatus, DoctorReport, FailoverDecision,
    FailoverEvent, FailoverState,
};
pub use notifier::{
    send_desktop_notification, send_desktop_notification_with_actions,
    spawn_notification_action_listener,
};

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

    #[tokio::test]
    async fn test_run_daemon_with_options_defaults_and_shutdown() {
        let opts = DaemonRunOptions::new();
        assert_eq!(opts, DaemonRunOptions::default());
        assert_eq!(opts.interval_secs, 60);
        assert!(!opts.auto_switch);
        assert!((opts.threshold - 1.0).abs() < f64::EPSILON);
        assert!((opts.cooldown_seconds - 120.0).abs() < f64::EPSILON);
        assert_eq!(opts.restart, FailoverRestart::Restart);
        assert!(opts.restart.should_restart());
        assert!(!FailoverRestart::NoRestart.should_restart());

        let custom = DaemonRunOptions {
            interval_secs: 0,
            auto_switch: true,
            threshold: 5.0,
            cooldown_seconds: 30.0,
            restart: FailoverRestart::NoRestart,
        };
        let (tx, rx) = tokio::sync::watch::channel(true);
        drop(tx);
        let res = run_daemon_with_options(custom, Some(rx)).await;
        assert!(res.is_ok());
    }

    #[test]
    fn test_doctor_geo_shield_disallowed_produces_overall_fail() {
        let cfg = crate::shield::ShieldConfig {
            enabled: true,
            mode: crate::shield::ShieldMode::Restricted,
            ..Default::default()
        };
        let blocked_geo = crate::shield::EgressGeo {
            ip: "5.6.7.8".into(),
            country_code: "IR".into(),
            country_name: "Iran".into(),
            is_restricted: true,
            ..Default::default()
        };

        let geo_item = build_geo_shield_diagnostic(&cfg, &blocked_geo);
        assert_eq!(geo_item.status, DiagnosticStatus::Fail);

        let items = vec![
            DiagnosticItem {
                component: "Linux Secret Service".into(),
                status: DiagnosticStatus::Pass,
                details: "ok".into(),
            },
            geo_item,
        ];
        assert_eq!(compute_overall_status(&items), DiagnosticStatus::Fail);
    }

    #[tokio::test]
    async fn test_run_daemon_returns_daemon_error_and_tick_snapshot_publishes() {
        let (snap_tx, snap_rx) = tokio::sync::watch::channel::<Option<DaemonTickSnapshot>>(None);
        let (shut_tx, shut_rx) = tokio::sync::watch::channel(true);
        drop(shut_tx);

        let opts = DaemonRunOptions::new();
        let res: Result<(), DaemonError> =
            run_daemon_with_snapshot(opts, Some(shut_rx), Some(snap_tx)).await;
        assert!(res.is_ok());
        assert!(snap_rx.borrow().is_none());

        let (shut_tx2, shut_rx2) = tokio::sync::watch::channel(true);
        drop(shut_tx2);
        let res2: Result<(), DaemonError> = run_daemon(1, false, Some(shut_rx2)).await;
        assert!(res2.is_ok());
    }

    #[test]
    fn test_publish_tick_updates_broadcasts_sse_and_snapshot() {
        let mut rx = crate::web::server::subscribe_events();
        let (snap_tx, snap_rx) = tokio::sync::watch::channel::<Option<DaemonTickSnapshot>>(None);

        let mut quotas = HashMap::new();
        quotas.insert(
            "user@example.com".into(),
            crate::quota::AccountQuotaResult {
                email: "user@example.com".into(),
                name: "User".into(),
                tier: "Pro".into(),
                is_current_ide_session: true,
                is_current_desktop_session: false,
                groups: vec![],
                models: vec![],
                checked_at: 1234.0,
                is_cached: false,
                error: None,
            },
        );
        let geo = crate::shield::EgressGeo {
            ip: "1.2.3.4".into(),
            country_code: "US".into(),
            ..Default::default()
        };
        let balancer_res = crate::balancer::BalancerDecision {
            triggered: true,
            reason: "ROTATED".into(),
            message: "rotated".into(),
            dry_run: None,
            strategy: None,
            from_account: None,
            from_quota_pct: None,
            to_account: None,
            to_quota_pct: None,
            cooldown_remaining: None,
            error: None,
        };
        let failover_res = FailoverDecision {
            triggered: true,
            reason: "SUCCESS".into(),
            message: "switched".into(),
            dry_run: None,
            from_account: None,
            to_account: None,
            to_quota_pct: None,
            error: None,
        };

        daemon::publish_tick_updates(
            quotas,
            geo.clone(),
            &balancer_res,
            &failover_res,
            Some(&snap_tx),
        );

        let ev1 = rx.try_recv().expect("expected quota_updated SSE event");
        let ev2 = rx.try_recv().expect("expected balancer_rotated SSE event");
        let ev3 = rx
            .try_recv()
            .expect("expected failover_triggered SSE event");
        assert!(ev1.contains("\"type\":\"quota_updated\""));
        assert!(ev2.contains("\"type\":\"balancer_rotated\""));
        assert!(ev3.contains("\"type\":\"failover_triggered\""));

        let snapshot = snap_rx
            .borrow()
            .clone()
            .expect("expected snapshot to be published");
        assert!(snapshot.quotas.contains_key("user@example.com"));
        assert_eq!(snapshot.geo, geo);
        assert!(snapshot.balancer_triggered);
        assert!(snapshot.failover_triggered);
    }

    #[tokio::test]
    async fn test_notifier_signatures_compile() {
        let _fut = send_desktop_notification_with_actions(
            "Title",
            "Body",
            "normal",
            &[("switch", "Switch")],
        );
        let handle = spawn_notification_action_listener(|_action| {});
        handle.abort();
    }
}
