//! System tray indicator service, state builder, and lifecycle runner.

pub mod applet;
pub mod icon;
pub mod lock;

pub use applet::{AntigravityTray, TrayCommand, TrayState, switchable_accounts};
pub use icon::{QuotaIndicator, default_icon_cache_dir, ensure_cached_icon_png, render_tray_icon};
pub use lock::{TrayError, TrayLock, TrayLockError, default_lock_path};

use crate::balancer::BalancerStrategy;
use crate::quota::{AccountQuotaResult, QuotaRefreshPolicy};
use crate::shield::models::EgressGeo;
use crate::storage::StorageManager;
use crate::storage::models::AccountProfile;
use ksni::TrayMethods;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

fn select_active_session_summary(
    quotas: &HashMap<String, AccountQuotaResult>,
) -> (Option<String>, QuotaIndicator, String) {
    let mut active_entries: Vec<(&String, &AccountQuotaResult)> = quotas
        .iter()
        .filter(|(_, q)| q.is_current_desktop_session || q.is_current_ide_session)
        .collect();

    if active_entries.is_empty() {
        return (
            None,
            QuotaIndicator::Exhausted,
            "No active session detected".to_string(),
        );
    }

    // Prefer desktop session over IDE session, then break ties deterministically by email.
    active_entries.sort_by(|(email_a, qa), (email_b, qb)| {
        (!qa.is_current_desktop_session)
            .cmp(&(!qb.is_current_desktop_session))
            .then_with(|| email_a.cmp(email_b))
    });

    let primary_email = active_entries[0].0.clone();
    let min_pct = active_entries
        .iter()
        .flat_map(|(_, q)| q.groups.iter())
        .filter_map(|g| g.weekly.as_ref().and_then(|w| w.remaining_percent))
        .fold(100.0_f64, f64::min);

    let indicator = QuotaIndicator::from_percent(min_pct);
    let summary = format!("{primary_email}: {min_pct:.1}% remaining");
    (Some(primary_email), indicator, summary)
}

/// Inputs used by [`build_tray_state_from_data`] to construct a [`TrayState`].
#[derive(Debug, Clone)]
pub struct TrayStateInput<'a> {
    /// Fetched quota snapshots keyed by account email.
    pub quotas: &'a HashMap<String, AccountQuotaResult>,
    /// Registered accounts to expose in the tray switch submenu.
    pub accounts: Vec<AccountProfile>,
    /// Current egress IP and Geo-Shield compliance metadata.
    pub geo_info: &'a EgressGeo,
    /// Whether IP Killswitch Shield protection is enabled.
    pub is_shield_enabled: bool,
    /// Whether multi-account Auto-Balancer rotation is enabled.
    pub is_balancer_enabled: bool,
    /// Active Auto-Balancer rotation strategy.
    pub balancer_strategy: BalancerStrategy,
    /// Best ready backup candidate `(email, remaining_percent)`, if any.
    pub backup_candidate: Option<(String, f64)>,
    /// Directory where rendered tray PNG icons are cached.
    pub icon_cache_dir: &'a Path,
}

/// Builds a [`TrayState`] snapshot from fetched quota, account, geo, and policy inputs.
///
/// Renders the corresponding tray icon in memory and ensures its PNG representation
/// exists in `input.icon_cache_dir` on cache miss.
#[must_use]
pub fn build_tray_state_from_data(input: TrayStateInput<'_>) -> TrayState {
    let mut accounts = input.accounts;
    accounts.sort_by(|a, b| a.email.cmp(&b.email));

    let (active_email, indicator, active_summary) = select_active_session_summary(input.quotas);
    let (icon, icon_name) = render_tray_icon(indicator);
    if let Err(err) = ensure_cached_icon_png(indicator, input.icon_cache_dir) {
        eprintln!("[Tray] Warning: failed to cache tray icon PNG: {err}");
    }

    let geo_summary = if input.geo_info.country_code.is_empty() {
        input.geo_info.ip.clone()
    } else {
        format!("{} ({})", input.geo_info.ip, input.geo_info.country_code)
    };

    let account_quotas = input
        .quotas
        .iter()
        .map(|(email, q)| {
            let (pct, _) = crate::daemon::failover::account_primary_quota(q);
            (email.trim().to_lowercase(), pct)
        })
        .collect();

    TrayState {
        indicator,
        active_email,
        active_summary,
        icon_name,
        icon_data: Arc::new(icon.data),
        icon_width: icon.width,
        icon_height: icon.height,
        accounts,
        geo_summary,
        is_restricted: input.geo_info.is_restricted,
        is_shield_enabled: input.is_shield_enabled,
        is_balancer_enabled: input.is_balancer_enabled,
        balancer_strategy: input.balancer_strategy,
        backup_candidate: input.backup_candidate,
        account_quotas,
    }
}

fn build_tray_state_from_quotas_and_geo(
    quotas: &HashMap<String, AccountQuotaResult>,
    geo_info: &EgressGeo,
    storage: &StorageManager,
    icon_cache_dir: &Path,
) -> TrayState {
    let reg = storage.load_accounts().unwrap_or_default();
    let acc_list: Vec<AccountProfile> = reg.accounts.into_values().collect();
    let sh_cfg = crate::shield::shield::load_shield_config(storage.config_dir());
    let b_cfg = crate::balancer::balancer::load_balancer_config();

    let sessions = crate::daemon::failover::active_sessions(quotas);
    let exclude: Vec<String> = sessions.values().map(|(e, _)| e.clone()).collect();
    let active_email = crate::daemon::failover::active_account(quotas).map(|(e, _)| e);
    let backup_candidate = crate::daemon::failover::find_best_failover_candidate(
        quotas,
        active_email.as_deref(),
        &exclude,
        5.0,
    );

    build_tray_state_from_data(TrayStateInput {
        quotas,
        accounts: acc_list,
        geo_info,
        is_shield_enabled: sh_cfg.enabled,
        is_balancer_enabled: b_cfg.enabled,
        balancer_strategy: b_cfg.strategy,
        backup_candidate,
        icon_cache_dir,
    })
}

fn build_tray_state_from_snapshot(
    snapshot: &crate::daemon::DaemonTickSnapshot,
    storage: &StorageManager,
    icon_cache_dir: &Path,
) -> TrayState {
    build_tray_state_from_quotas_and_geo(&snapshot.quotas, &snapshot.geo, storage, icon_cache_dir)
}

async fn apply_feed_snapshot(
    snapshot: &crate::daemon::DaemonTickSnapshot,
    storage: &StorageManager,
    shared_state: &Arc<Mutex<TrayState>>,
    service: &ksni::Handle<AntigravityTray>,
) {
    let new_state = build_tray_state_from_snapshot(snapshot, storage, &default_icon_cache_dir());
    {
        let mut st = shared_state
            .lock()
            .expect("tray shared state lock poisoned");
        *st = new_state;
    }
    let _ = service.update(|_tray: &mut AntigravityTray| {}).await;
}

async fn refresh_tray_state(
    storage: &StorageManager,
    shared_state: &Arc<Mutex<TrayState>>,
    service: &ksni::Handle<AntigravityTray>,
    policy: QuotaRefreshPolicy,
) {
    let _ = crate::auth::sync::sync_from_antigravity(storage).await;
    let q_map = crate::quota::quota::fetch_all_accounts_quota(policy).await;
    let geo_info = crate::shield::geo::get_ip_geo(false).await;
    let _ = crate::shield::shield::evaluate_and_enforce_shield(
        storage.config_dir(),
        Some(geo_info.clone()),
    )
    .await;

    let new_state =
        build_tray_state_from_quotas_and_geo(&q_map, &geo_info, storage, &default_icon_cache_dir());

    crate::web::server::broadcast_event(
        "quota_updated",
        serde_json::json!({
            "accounts": &q_map,
            "geo": &geo_info,
        }),
    );

    {
        let mut st = shared_state
            .lock()
            .expect("tray shared state lock poisoned");
        *st = new_state;
    }

    let _ = service.update(|_tray: &mut AntigravityTray| {}).await;
}

fn toggle_shield_and_broadcast(
    storage: &StorageManager,
    shared_state: &Arc<Mutex<TrayState>>,
) -> bool {
    let mut cfg = crate::shield::shield::load_shield_config(storage.config_dir());
    cfg.enabled = !cfg.enabled;
    if let Err(err) = crate::shield::shield::save_shield_config(storage.config_dir(), &cfg) {
        eprintln!("[Tray] Failed to save shield config: {err}");
    }
    {
        let mut st = shared_state
            .lock()
            .expect("tray shared state lock poisoned");
        st.is_shield_enabled = cfg.enabled;
    }
    crate::web::server::broadcast_event(
        "shield_updated",
        serde_json::json!({ "enabled": cfg.enabled, "mode": cfg.mode }),
    );
    cfg.enabled
}

fn toggle_balancer_and_broadcast(shared_state: &Arc<Mutex<TrayState>>) -> bool {
    let mut cfg = crate::balancer::balancer::load_balancer_config();
    cfg.enabled = !cfg.enabled;
    if let Err(err) = crate::balancer::balancer::save_balancer_config(&cfg) {
        eprintln!("[Tray] Failed to save balancer config: {err}");
    }
    {
        let mut st = shared_state
            .lock()
            .expect("tray shared state lock poisoned");
        st.is_balancer_enabled = cfg.enabled;
    }
    crate::web::server::broadcast_event(
        "balancer_rotated",
        serde_json::json!({ "enabled": cfg.enabled, "strategy": cfg.strategy }),
    );
    cfg.enabled
}

fn route_notification_action(
    action: &str,
    shared_state: &Arc<Mutex<TrayState>>,
    cmd_tx: &UnboundedSender<TrayCommand>,
) {
    match action {
        "switch_best" => {
            let target = shared_state
                .lock()
                .expect("tray shared state lock poisoned")
                .backup_candidate
                .as_ref()
                .map(|(email, _)| email.clone());
            if let Some(email) = target {
                let _ = cmd_tx.send(TrayCommand::SwitchAccount(email));
            }
        }
        "open_dashboard" => {
            let _ = cmd_tx.send(TrayCommand::OpenDashboard);
        }
        _ => {}
    }
}

async fn handle_tray_command(
    cmd: TrayCommand,
    storage: &StorageManager,
    shared_state: &Arc<Mutex<TrayState>>,
    service: &ksni::Handle<AntigravityTray>,
    cmd_tx: &UnboundedSender<TrayCommand>,
) {
    match cmd {
        TrayCommand::OpenDashboard => {
            tokio::task::spawn_blocking(|| {
                if let Err(err) = open::that("http://localhost:8765") {
                    eprintln!("[Tray] Failed to open browser: {err}");
                }
            });
        }
        TrayCommand::ToggleShield => {
            let enabled = toggle_shield_and_broadcast(storage, shared_state);
            let _ = service.update(|_tray: &mut AntigravityTray| {}).await;
            let desc = if enabled {
                "ACTIVATED (Auto-terminates Antigravity if VPN drops)"
            } else {
                "DISABLED"
            };
            tokio::spawn(async move {
                let _ = crate::daemon::notifier::send_desktop_notification(
                    "🛡️ IP Shield Protection",
                    &format!("IP Killswitch Shield is now {desc}."),
                    "normal",
                )
                .await;
            });
        }
        TrayCommand::ToggleBalancer => {
            let enabled = toggle_balancer_and_broadcast(shared_state);
            let _ = service.update(|_tray: &mut AntigravityTray| {}).await;
            let desc = if enabled { "ENABLED" } else { "DISABLED" };
            tokio::spawn(async move {
                let _ = crate::daemon::notifier::send_desktop_notification(
                    "🔄 Auto-Balancer",
                    &format!("Multi-account pool rotation is now {desc}."),
                    "normal",
                )
                .await;
            });
        }
        TrayCommand::SwitchAccount(target) => {
            let tx = cmd_tx.clone();
            tokio::spawn(async move {
                let (ok, msg) = match crate::balancer::switcher::switch_to_account(
                    &target,
                    true,
                    crate::balancer::switcher::SwitchSurface::Both,
                )
                .await
                {
                    Ok(m) => {
                        crate::web::server::broadcast_event(
                            "account_switched",
                            serde_json::json!({ "target": target, "surface": "both" }),
                        );
                        (true, m.to_string())
                    }
                    Err(e) => (false, e.to_string()),
                };
                let title = if ok {
                    "✓ Account Switched"
                } else {
                    "❌ Switch Failed"
                };
                let _ = crate::daemon::notifier::send_desktop_notification(
                    title,
                    &msg,
                    if ok { "normal" } else { "critical" },
                )
                .await;
                let _ = tx.send(TrayCommand::RefreshQuotas);
            });
        }
        TrayCommand::RefreshQuotas => {
            refresh_tray_state(
                storage,
                shared_state,
                service,
                QuotaRefreshPolicy::ForceRefresh,
            )
            .await;
        }
        TrayCommand::Quit => {}
    }
}

async fn wait_for_shutdown(shutdown: &mut Option<tokio::sync::watch::Receiver<bool>>) {
    if let Some(rx) = shutdown {
        if *rx.borrow() {
            return;
        }
        let _ = rx.changed().await;
    } else {
        std::future::pending::<()>().await;
    }
}

async fn wait_for_feed_snapshot(
    feed: &mut Option<tokio::sync::watch::Receiver<Option<crate::daemon::DaemonTickSnapshot>>>,
) -> crate::daemon::DaemonTickSnapshot {
    loop {
        let Some(rx) = feed.as_mut() else {
            std::future::pending::<()>().await;
            unreachable!();
        };
        if rx.changed().await.is_err() {
            // Sender dropped; park this branch instead of busy-looping or starting duplicate polling.
            *feed = None;
            continue;
        }
        if let Some(snapshot) = rx.borrow_and_update().clone() {
            return snapshot;
        }
    }
}

async fn wait_for_interval_tick(interval: &mut Option<tokio::time::Interval>) {
    if let Some(iv) = interval.as_mut() {
        iv.tick().await;
    } else {
        std::future::pending::<()>().await;
    }
}

/// Starts the single-instance system tray indicator, background web dashboard,
/// and periodic quota/shield refresh loop.
///
/// Delegates to [`run_tray_with_feed`] with `feed = None`.
///
/// # Errors
///
/// Returns [`TrayError::Lock`] if acquiring the tray lockfile fails with an I/O error,
/// [`TrayError::Dbus`] if registering the `ksni` StatusNotifierItem service fails, or
/// [`TrayError::Io`] if installing OS signal handlers fails.
pub async fn run_tray(
    interval_secs: u64,
    lock_path: Option<&Path>,
    shutdown: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<(), TrayError> {
    run_tray_with_feed(interval_secs, lock_path, shutdown, None).await
}

/// Starts the single-instance system tray indicator, background web dashboard,
/// and either a daemon snapshot feed or a periodic quota/shield refresh loop.
///
/// When `feed` is `Some(rx)`, skips duplicate periodic polling and updates [`TrayState`]
/// whenever `rx.changed()` yields `Some(snapshot)`. Also spawns the desktop notification
/// action listener and aborts it on clean shutdown.
///
/// Exits cleanly on `SIGINT`, `SIGTERM`, a `true` signal on `shutdown`, or
/// [`TrayCommand::Quit`], shutting down the D-Bus tray service before releasing [`TrayLock`].
///
/// # Errors
///
/// Returns [`TrayError::Lock`] if acquiring the tray lockfile fails with an I/O error,
/// [`TrayError::Dbus`] if registering the `ksni` StatusNotifierItem service fails, or
/// [`TrayError::Io`] if installing OS signal handlers fails.
pub async fn run_tray_with_feed(
    interval_secs: u64,
    lock_path: Option<&Path>,
    mut shutdown: Option<tokio::sync::watch::Receiver<bool>>,
    mut feed: Option<tokio::sync::watch::Receiver<Option<crate::daemon::DaemonTickSnapshot>>>,
) -> Result<(), TrayError> {
    if shutdown.as_ref().is_some_and(|rx| *rx.borrow()) {
        return Ok(());
    }

    // 1. Singleton lock
    let _lock = match TrayLock::acquire(lock_path) {
        Ok(l) => l,
        Err(TrayLockError::AlreadyRunning(existing_pid)) => {
            let pid_str = existing_pid
                .map(|p| format!(" (PID {p})"))
                .unwrap_or_default();
            println!(
                "[Tray] Another tray indicator instance is already active{pid_str}. Exiting duplicate."
            );
            return Ok(());
        }
        Err(TrayLockError::Io(e)) => return Err(TrayLockError::Io(e).into()),
    };

    // 2. Start background web server
    tokio::spawn(async move {
        if let Err(err) = crate::web::server::start_web_server(8765).await {
            eprintln!("[Tray] Web server failed on port 8765: {err}");
        }
    });

    // 3. Instant Initial State from Local Storage (zero network blocking)
    let storage = StorageManager::default();
    let registered = storage.load_accounts().unwrap_or_default();
    let mut accounts_list: Vec<AccountProfile> = registered.accounts.into_values().collect();
    accounts_list.sort_by(|a, b| a.email.cmp(&b.email));

    let active_email = registered.active_email.clone().or_else(|| {
        accounts_list
            .iter()
            .find(|acc| {
                acc.is_current_desktop_session == Some(true)
                    || acc.is_current_ide_session == Some(true)
            })
            .map(|acc| acc.email.clone())
    });

    let shield_cfg = crate::shield::shield::load_shield_config(storage.config_dir());
    let bal_cfg = crate::balancer::balancer::load_balancer_config();

    let initial_indicator = QuotaIndicator::Available(100.0);
    let (icon, icon_name) = render_tray_icon(initial_indicator);
    if let Err(err) = ensure_cached_icon_png(initial_indicator, &default_icon_cache_dir()) {
        eprintln!("[Tray] Warning: failed to cache initial tray icon PNG: {err}");
    }

    let initial_state = TrayState {
        indicator: initial_indicator,
        active_email,
        active_summary: "Initializing quota monitor...".to_string(),
        icon_name,
        icon_data: Arc::new(icon.data),
        icon_width: icon.width,
        icon_height: icon.height,
        accounts: accounts_list,
        geo_summary: String::new(),
        is_restricted: false,
        is_shield_enabled: shield_cfg.enabled,
        is_balancer_enabled: bal_cfg.enabled,
        balancer_strategy: bal_cfg.strategy,
        backup_candidate: None,
        account_quotas: HashMap::new(),
    };

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<TrayCommand>();
    let shared_state = Arc::new(Mutex::new(initial_state));
    let tray = AntigravityTray {
        state: Arc::clone(&shared_state),
        cmd_tx: cmd_tx.clone(),
    };

    // 4. Spawn tray service immediately over DBus (assume_sni_available ensures resilience
    // if the desktop StatusNotifierWatcher starts slightly after or temporarily disconnects)
    println!("[Tray] Registering StatusNotifierItem over DBus...");
    let service = tray.assume_sni_available(true).spawn().await?;
    println!("[Tray] Registered StatusNotifierItem service.");

    // 5. Event loop driven either by daemon snapshot feed or standalone periodic polling
    let mut interval = feed
        .is_none()
        .then(|| tokio::time::interval(Duration::from_secs(interval_secs.max(1))));
    #[cfg(unix)]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    let action_listener_handle = {
        let action_cmd_tx = cmd_tx.clone();
        let action_state = Arc::clone(&shared_state);
        crate::daemon::notifier::spawn_notification_action_listener(move |action| {
            route_notification_action(&action, &action_state, &action_cmd_tx);
        })
    };

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n[Tray] Stopped (SIGINT).");
                break;
            }
            _ = async {
                #[cfg(unix)]
                {
                    sigterm.recv().await;
                }
                #[cfg(not(unix))]
                {
                    std::future::pending::<()>().await;
                }
            } => {
                println!("\n[Tray] Stopped (SIGTERM).");
                break;
            }
            () = wait_for_shutdown(&mut shutdown) => {
                println!("\n[Tray] Stopped (shutdown signal).");
                break;
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(TrayCommand::Quit) | None => {
                        println!("\n[Tray] Stopped (Quit).");
                        break;
                    }
                    Some(other_cmd) => {
                        handle_tray_command(
                            other_cmd,
                            &storage,
                            &shared_state,
                            &service,
                            &cmd_tx,
                        )
                        .await;
                    }
                }
            }
            snapshot = wait_for_feed_snapshot(&mut feed) => {
                apply_feed_snapshot(&snapshot, &storage, &shared_state, &service).await;
            }
            () = wait_for_interval_tick(&mut interval) => {
                refresh_tray_state(
                    &storage,
                    &shared_state,
                    &service,
                    QuotaRefreshPolicy::ForceRefresh,
                )
                .await;
            }
        }
    }

    action_listener_handle.abort();
    service.shutdown().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_tray_icon() {
        let (icon, name) = icon::render_tray_icon(icon::QuotaIndicator::Available(85.0));
        assert_eq!(icon.width, 48);
        assert_eq!(icon.height, 48);
        assert_eq!(icon.data.len(), 48 * 48 * 4);
        assert_eq!(name, "quota_85");

        let (exh_icon, exh_name) = icon::render_tray_icon(icon::QuotaIndicator::Exhausted);
        assert_eq!(exh_name, "quota_exhausted");
        assert_eq!(exh_icon.data.len(), 48 * 48 * 4);
    }

    #[test]
    fn test_tray_lock() {
        let dir = tempfile::tempdir().unwrap();
        let test_lock_file = dir.path().join("test_agy_tray.lock");

        let lock1 = lock::TrayLock::acquire(Some(&test_lock_file));
        assert!(lock1.is_ok());

        let lock2 = lock::TrayLock::acquire(Some(&test_lock_file));
        assert!(matches!(lock2, Err(TrayLockError::AlreadyRunning(_))));

        drop(lock1);
        let lock3 = lock::TrayLock::acquire(Some(&test_lock_file));
        assert!(lock3.is_ok());
    }

    #[test]
    fn test_build_tray_state_populates_active_and_backup_candidate() {
        let mut q_map = HashMap::new();
        q_map.insert(
            "active@example.com".to_string(),
            AccountQuotaResult {
                email: "active@example.com".into(),
                name: "Active".into(),
                tier: "Pro".into(),
                is_current_desktop_session: true,
                is_current_ide_session: false,
                groups: vec![crate::storage::models::QuotaGroup {
                    display_name: "Gemini".into(),
                    weekly: Some(crate::storage::models::QuotaBucket {
                        remaining_percent: Some(42.5),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                models: vec![],
                checked_at: 1000.0,
                is_cached: false,
                error: None,
            },
        );
        q_map.insert(
            "backup@example.com".to_string(),
            AccountQuotaResult {
                email: "backup@example.com".into(),
                name: "Backup".into(),
                tier: "Pro".into(),
                is_current_desktop_session: false,
                is_current_ide_session: false,
                groups: vec![crate::storage::models::QuotaGroup {
                    display_name: "Gemini".into(),
                    weekly: Some(crate::storage::models::QuotaBucket {
                        remaining_percent: Some(95.0),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                models: vec![],
                checked_at: 1000.0,
                is_cached: false,
                error: None,
            },
        );

        let dir = tempfile::tempdir().unwrap();
        let state = build_tray_state_from_data(TrayStateInput {
            quotas: &q_map,
            accounts: vec![],
            geo_info: &EgressGeo::default(),
            is_shield_enabled: false,
            is_balancer_enabled: false,
            balancer_strategy: BalancerStrategy::ExpiryFirst,
            backup_candidate: Some(("backup@example.com".into(), 95.0)),
            icon_cache_dir: dir.path(),
        });
        assert_eq!(state.indicator, QuotaIndicator::Available(42.5));
        assert_eq!(state.active_email.as_deref(), Some("active@example.com"));
        assert_eq!(
            state.backup_candidate,
            Some(("backup@example.com".to_string(), 95.0))
        );
    }

    #[test]
    fn test_build_tray_state_no_active_session_maps_to_exhausted() {
        let q_map = HashMap::new();
        let dir = tempfile::tempdir().unwrap();
        let state = build_tray_state_from_data(TrayStateInput {
            quotas: &q_map,
            accounts: vec![],
            geo_info: &EgressGeo::default(),
            is_shield_enabled: true,
            is_balancer_enabled: true,
            balancer_strategy: BalancerStrategy::Watermark,
            backup_candidate: None,
            icon_cache_dir: dir.path(),
        });
        assert_eq!(state.indicator, QuotaIndicator::Exhausted);
        assert_eq!(state.active_email, None);
        assert_eq!(state.active_summary, "No active session detected");
        assert!(state.is_shield_enabled);
        assert!(state.is_balancer_enabled);
    }

    #[test]
    fn test_switch_submenu_excludes_only_current_session_not_legacy_active_flag() {
        let accounts = vec![
            AccountProfile {
                email: "current@example.com".into(),
                active: Some(true),
                is_current_desktop_session: Some(true),
                ..Default::default()
            },
            AccountProfile {
                email: "other@example.com".into(),
                active: Some(true),
                is_current_desktop_session: Some(false),
                ..Default::default()
            },
        ];
        let candidates = switchable_accounts(&accounts, Some("CURRENT@example.com"));
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].email, "other@example.com");
    }

    #[test]
    fn test_tray_menu_callbacks_emit_commands_without_blocking() {
        use ksni::{MenuItem, Tray};

        let dir = tempfile::tempdir().unwrap();
        let accounts = vec![
            AccountProfile {
                email: "active@example.com".into(),
                active: Some(true),
                is_current_desktop_session: Some(true),
                ..Default::default()
            },
            AccountProfile {
                email: "other@example.com".into(),
                active: Some(true),
                is_current_desktop_session: Some(false),
                ..Default::default()
            },
        ];
        let mut q_map = HashMap::new();
        q_map.insert(
            "active@example.com".to_string(),
            AccountQuotaResult {
                email: "active@example.com".into(),
                name: "Active".into(),
                tier: "Pro".into(),
                is_current_desktop_session: true,
                is_current_ide_session: false,
                groups: vec![crate::storage::models::QuotaGroup {
                    display_name: "Gemini".into(),
                    weekly: Some(crate::storage::models::QuotaBucket {
                        remaining_percent: Some(60.0),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                models: vec![],
                checked_at: 1000.0,
                is_cached: false,
                error: None,
            },
        );
        let state = build_tray_state_from_data(TrayStateInput {
            quotas: &q_map,
            accounts,
            geo_info: &EgressGeo::default(),
            is_shield_enabled: false,
            is_balancer_enabled: false,
            balancer_strategy: BalancerStrategy::ExpiryFirst,
            backup_candidate: Some(("other@example.com".into(), 90.0)),
            icon_cache_dir: dir.path(),
        });

        let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut tray = AntigravityTray {
            state: Arc::new(Mutex::new(state)),
            cmd_tx,
        };

        tray.activate(0, 0);
        assert_eq!(cmd_rx.try_recv().unwrap(), TrayCommand::OpenDashboard);

        let menu = tray.menu();
        for item in menu {
            match item {
                MenuItem::Standard(std_item) => {
                    if std_item.label.contains("Open Web Dashboard") {
                        (std_item.activate)(&mut tray);
                        assert_eq!(cmd_rx.try_recv().unwrap(), TrayCommand::OpenDashboard);
                    } else if std_item.label.contains("Shield") {
                        (std_item.activate)(&mut tray);
                        assert_eq!(cmd_rx.try_recv().unwrap(), TrayCommand::ToggleShield);
                    } else if std_item.label.contains("Auto-Balancer") {
                        (std_item.activate)(&mut tray);
                        assert_eq!(cmd_rx.try_recv().unwrap(), TrayCommand::ToggleBalancer);
                    } else if std_item.label.contains("Refresh Quotas") {
                        (std_item.activate)(&mut tray);
                        assert_eq!(cmd_rx.try_recv().unwrap(), TrayCommand::RefreshQuotas);
                    } else if std_item.label.contains("Quit Applet") {
                        (std_item.activate)(&mut tray);
                        assert_eq!(cmd_rx.try_recv().unwrap(), TrayCommand::Quit);
                    }
                }
                MenuItem::SubMenu(sub) => {
                    for sub_item in sub.submenu {
                        if let MenuItem::Standard(std_item) = sub_item
                            && std_item.enabled
                        {
                            (std_item.activate)(&mut tray);
                            assert_eq!(
                                cmd_rx.try_recv().unwrap(),
                                TrayCommand::SwitchAccount("other@example.com".into())
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    #[tokio::test]
    async fn test_run_tray_graceful_shutdown_via_watch() {
        let dir = tempfile::tempdir().unwrap();
        let lock_file = dir.path().join("tray.lock");
        let (tx, rx) = tokio::sync::watch::channel(true);
        drop(tx);
        let res = run_tray(60, Some(&lock_file), Some(rx)).await;
        assert!(res.is_ok() || matches!(res, Err(TrayError::Dbus(_)) | Err(TrayError::Lock(_))));
    }

    #[test]
    fn test_build_tray_state_populates_account_quotas_and_renders_submenu_labels() {
        use ksni::{MenuItem, Tray};

        let mut q_map = HashMap::new();
        q_map.insert(
            "Active@Example.COM".to_string(),
            AccountQuotaResult {
                email: "Active@Example.COM".into(),
                name: "Active".into(),
                tier: "Pro".into(),
                is_current_desktop_session: true,
                is_current_ide_session: false,
                groups: vec![crate::storage::models::QuotaGroup {
                    display_name: "Gemini 2.5 Pro".into(),
                    weekly: Some(crate::storage::models::QuotaBucket {
                        remaining_percent: Some(55.0),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                models: vec![],
                checked_at: 1000.0,
                is_cached: false,
                error: None,
            },
        );
        q_map.insert(
            "Standby@Example.COM".to_string(),
            AccountQuotaResult {
                email: "Standby@Example.COM".into(),
                name: "Standby".into(),
                tier: "Pro".into(),
                is_current_desktop_session: false,
                is_current_ide_session: false,
                groups: vec![crate::storage::models::QuotaGroup {
                    display_name: "Gemini 2.5 Pro".into(),
                    weekly: Some(crate::storage::models::QuotaBucket {
                        remaining_percent: Some(91.2),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                models: vec![],
                checked_at: 1000.0,
                is_cached: false,
                error: None,
            },
        );

        let dir = tempfile::tempdir().unwrap();
        let accounts = vec![
            AccountProfile {
                email: "Active@Example.COM".into(),
                is_current_desktop_session: Some(true),
                ..Default::default()
            },
            AccountProfile {
                email: "Standby@Example.COM".into(),
                ..Default::default()
            },
            AccountProfile {
                email: "unpolled@example.com".into(),
                ..Default::default()
            },
        ];

        let state = build_tray_state_from_data(TrayStateInput {
            quotas: &q_map,
            accounts,
            geo_info: &EgressGeo::default(),
            is_shield_enabled: false,
            is_balancer_enabled: false,
            balancer_strategy: BalancerStrategy::ExpiryFirst,
            backup_candidate: Some(("Standby@Example.COM".into(), 91.2)),
            icon_cache_dir: dir.path(),
        });

        assert_eq!(state.account_quotas.get("active@example.com"), Some(&55.0));
        assert_eq!(state.account_quotas.get("standby@example.com"), Some(&91.2));

        let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let tray = AntigravityTray {
            state: Arc::new(Mutex::new(state)),
            cmd_tx,
        };
        let labels: Vec<String> = tray
            .menu()
            .into_iter()
            .find_map(|item| match item {
                MenuItem::SubMenu(sub) if sub.label == "🔄 Switch Account" => Some(
                    sub.submenu
                        .into_iter()
                        .filter_map(|sub_item| match sub_item {
                            MenuItem::Standard(std_item) => Some(std_item.label),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => None,
            })
            .expect("expected Switch Account submenu");

        assert_eq!(
            labels,
            vec![
                "⚡ Switch to Standby@Example.COM (91.2%)",
                "⚡ Switch to unpolled@example.com",
            ]
        );
    }

    #[test]
    fn test_daemon_tick_snapshot_maps_to_tray_state() {
        let dir = tempfile::tempdir().unwrap();
        let storage = StorageManager::new(dir.path());
        let mut reg = crate::storage::models::AccountsRegistry::default();
        reg.accounts.insert(
            "active@example.com".into(),
            AccountProfile {
                email: "active@example.com".into(),
                is_current_desktop_session: Some(true),
                ..Default::default()
            },
        );
        reg.accounts.insert(
            "standby@example.com".into(),
            AccountProfile {
                email: "standby@example.com".into(),
                ..Default::default()
            },
        );
        storage.save_accounts(&reg).unwrap();

        let shield_cfg = crate::shield::models::ShieldConfig {
            enabled: true,
            ..Default::default()
        };
        crate::shield::shield::save_shield_config(storage.config_dir(), &shield_cfg).unwrap();

        let mut quotas = HashMap::new();
        quotas.insert(
            "active@example.com".to_string(),
            AccountQuotaResult {
                email: "active@example.com".into(),
                name: "Active".into(),
                tier: "Pro".into(),
                is_current_desktop_session: true,
                is_current_ide_session: false,
                groups: vec![crate::storage::models::QuotaGroup {
                    display_name: "Gemini 2.5 Pro".into(),
                    weekly: Some(crate::storage::models::QuotaBucket {
                        remaining_percent: Some(33.5),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                models: vec![],
                checked_at: 1000.0,
                is_cached: false,
                error: None,
            },
        );
        quotas.insert(
            "standby@example.com".to_string(),
            AccountQuotaResult {
                email: "standby@example.com".into(),
                name: "Standby".into(),
                tier: "Pro".into(),
                is_current_desktop_session: false,
                is_current_ide_session: false,
                groups: vec![crate::storage::models::QuotaGroup {
                    display_name: "Gemini 2.5 Pro".into(),
                    weekly: Some(crate::storage::models::QuotaBucket {
                        remaining_percent: Some(88.0),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                models: vec![],
                checked_at: 1000.0,
                is_cached: false,
                error: None,
            },
        );

        let snapshot = crate::daemon::DaemonTickSnapshot {
            quotas,
            geo: EgressGeo {
                ip: "1.2.3.4".into(),
                country_code: "US".into(),
                is_restricted: false,
                ..Default::default()
            },
            balancer_triggered: true,
            failover_triggered: false,
        };

        let icon_dir = dir.path().join("icons");
        let state = build_tray_state_from_snapshot(&snapshot, &storage, &icon_dir);

        assert_eq!(state.indicator, QuotaIndicator::Available(33.5));
        assert_eq!(state.active_email.as_deref(), Some("active@example.com"));
        assert_eq!(state.geo_summary, "1.2.3.4 (US)");
        assert!(!state.is_restricted);
        assert!(state.is_shield_enabled);
        assert_eq!(state.accounts.len(), 2);
        assert_eq!(state.account_quotas.get("active@example.com"), Some(&33.5));
        assert_eq!(state.account_quotas.get("standby@example.com"), Some(&88.0));
    }

    #[test]
    fn test_toggle_shield_emits_sse_event() {
        let dir = tempfile::tempdir().unwrap();
        let storage = StorageManager::new(dir.path());
        crate::shield::shield::save_shield_config(
            storage.config_dir(),
            &crate::shield::models::ShieldConfig {
                enabled: false,
                ..Default::default()
            },
        )
        .unwrap();
        let mut rx = crate::web::server::subscribe_events();

        let icon_dir = dir.path().join("icons");
        let state = build_tray_state_from_data(TrayStateInput {
            quotas: &HashMap::new(),
            accounts: vec![],
            geo_info: &EgressGeo::default(),
            is_shield_enabled: false,
            is_balancer_enabled: false,
            balancer_strategy: BalancerStrategy::ExpiryFirst,
            backup_candidate: None,
            icon_cache_dir: &icon_dir,
        });
        let shared_state = Arc::new(Mutex::new(state));

        let enabled = toggle_shield_and_broadcast(&storage, &shared_state);
        assert!(enabled);
        assert!(shared_state.lock().unwrap().is_shield_enabled);

        let mut found_shield_event = false;
        while let Ok(raw) = rx.try_recv() {
            if raw.contains("\"type\":\"shield_updated\"") && raw.contains("\"enabled\":true") {
                found_shield_event = true;
                break;
            }
        }
        assert!(found_shield_event, "expected shield_updated SSE event");
    }

    #[test]
    fn test_route_notification_action_handles_switch_best_and_open_dashboard() {
        let dir = tempfile::tempdir().unwrap();
        let state = build_tray_state_from_data(TrayStateInput {
            quotas: &HashMap::new(),
            accounts: vec![],
            geo_info: &EgressGeo::default(),
            is_shield_enabled: false,
            is_balancer_enabled: false,
            balancer_strategy: BalancerStrategy::ExpiryFirst,
            backup_candidate: Some(("best@example.com".into(), 94.0)),
            icon_cache_dir: dir.path(),
        });
        let shared_state = Arc::new(Mutex::new(state));
        let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();

        route_notification_action("switch_best", &shared_state, &cmd_tx);
        assert_eq!(
            cmd_rx.try_recv().unwrap(),
            TrayCommand::SwitchAccount("best@example.com".into())
        );

        route_notification_action("open_dashboard", &shared_state, &cmd_tx);
        assert_eq!(cmd_rx.try_recv().unwrap(), TrayCommand::OpenDashboard);

        route_notification_action("unknown_action", &shared_state, &cmd_tx);
        assert!(cmd_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_run_tray_with_feed_graceful_shutdown_via_watch() {
        let dir = tempfile::tempdir().unwrap();
        let lock_file = dir.path().join("tray_feed.lock");
        let (shut_tx, shut_rx) = tokio::sync::watch::channel(true);
        let (_snap_tx, snap_rx) =
            tokio::sync::watch::channel::<Option<crate::daemon::DaemonTickSnapshot>>(None);
        drop(shut_tx);

        let res = run_tray_with_feed(60, Some(&lock_file), Some(shut_rx), Some(snap_rx)).await;
        assert!(res.is_ok() || matches!(res, Err(TrayError::Dbus(_)) | Err(TrayError::Lock(_))));
    }
}
