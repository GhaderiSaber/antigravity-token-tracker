//! Tray module.

pub mod applet;
pub mod icon;
pub mod lock;

pub use applet::*;
pub use icon::*;
pub use lock::*;

use crate::storage::StorageManager;
use ksni::TrayMethods;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub async fn run_tray(
    interval_secs: u64,
    lock_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        Err(TrayLockError::Io(e)) => return Err(e.into()),
    };

    // 2. Start background web server
    tokio::spawn(async move {
        let _ = crate::web::server::start_web_server(8765).await;
    });

    // 3. Initial state
    println!("[Tray] Initializing storage and discovering active sessions...");
    let storage = StorageManager::default();
    let _ = crate::auth::sync::sync_from_antigravity(&storage).await;
    println!("[Tray] Fetching account quotas...");
    let all_q = crate::quota::quota::fetch_all_accounts_quota(false).await;
    println!("[Tray] Verifying egress IP...");
    let geo = crate::shield::geo::get_ip_geo(false).await;
    let _ =
        crate::shield::shield::evaluate_and_enforce_shield(storage.config_dir(), Some(geo.clone()))
            .await;

    let registered = storage.load_accounts().unwrap_or_default();
    let accounts_list: Vec<crate::storage::models::AccountProfile> =
        registered.accounts.into_values().collect();

    let mut min_pct = 100.0f64;
    let mut is_exhausted = false;
    let mut active_summary = String::new();

    for (email, q) in &all_q {
        let is_active = q.is_current_desktop_session || q.is_current_ide_session;
        if is_active {
            let p = q
                .groups
                .iter()
                .filter_map(|g| g.weekly.as_ref().and_then(|w| w.remaining_percent))
                .fold(100.0f64, f64::min);
            if p < min_pct {
                min_pct = p;
            }
            if min_pct <= 0.0 {
                is_exhausted = true;
            }
            active_summary = format!("{email}: {min_pct:.1}% remaining");
        }
    }

    if active_summary.is_empty() {
        min_pct = 0.0;
        is_exhausted = true;
        active_summary = "No active session detected".to_string();
    }

    let (icon_data, icon_name) = icon::render_tray_icon(min_pct, is_exhausted);

    let shield_cfg = crate::shield::shield::load_shield_config(storage.config_dir());
    let bal_cfg = crate::balancer::balancer::load_balancer_config();

    let initial_state = TrayState {
        min_pct,
        is_exhausted,
        active_summary,
        icon_name,
        icon: icon_data,
        accounts: accounts_list,
        geo_summary: format!("{} ({})", geo.ip, geo.country_code),
        is_restricted: geo.is_restricted,
        is_shield_enabled: shield_cfg.enabled,
        is_balancer_enabled: bal_cfg.enabled,
        balancer_strategy: bal_cfg.strategy,
        backup_candidate: None,
    };

    let shared_state = Arc::new(Mutex::new(initial_state));
    let tray = AntigravityTray {
        state: shared_state.clone(),
    };

    // 4. Spawn tray service
    println!("[Tray] Registering StatusNotifierItem over DBus...");
    let service = tray.spawn().await?;

    println!("[Tray] Registered with desktop StatusNotifierWatcher.");

    // 5. Periodic update loop
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
    // Consume initial tick
    interval.tick().await;

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n[Tray] Stopped.");
                break;
            }
            _ = interval.tick() => {
                // Refresh data
                let _ = crate::auth::sync::sync_from_antigravity(&storage).await;
                let q_map = crate::quota::quota::fetch_all_accounts_quota(false).await;
                let geo_info = crate::shield::geo::get_ip_geo(false).await;
                let _ = crate::shield::shield::evaluate_and_enforce_shield(
                    storage.config_dir(),
                    Some(geo_info.clone()),
                ).await;

                let reg = storage.load_accounts().unwrap_or_default();
                let acc_list: Vec<crate::storage::models::AccountProfile> = reg.accounts.into_values().collect();

                let mut cur_min_pct = 100.0f64;
                let mut cur_exhausted = false;
                let mut summary = String::new();

                for (email, q) in &q_map {
                    let is_active = q.is_current_desktop_session || q.is_current_ide_session;
                    if is_active {
                        let p = q.groups.iter()
                            .filter_map(|g| g.weekly.as_ref().and_then(|w| w.remaining_percent))
                            .fold(100.0f64, f64::min);
                        if p < cur_min_pct {
                            cur_min_pct = p;
                        }
                        if cur_min_pct <= 0.0 {
                            cur_exhausted = true;
                        }
                        summary = format!("{email}: {cur_min_pct:.1}% remaining");
                    }
                }

                if summary.is_empty() {
                    cur_min_pct = 0.0;
                    cur_exhausted = true;
                    summary = "No active session detected".to_string();
                }

                let (ic, ic_name) = icon::render_tray_icon(cur_min_pct, cur_exhausted);
                let sh_cfg = crate::shield::shield::load_shield_config(storage.config_dir());
                let b_cfg = crate::balancer::balancer::load_balancer_config();

                {
                    let mut st = shared_state.lock().unwrap();
                    st.min_pct = cur_min_pct;
                    st.is_exhausted = cur_exhausted;
                    st.active_summary = summary;
                    st.icon_name = ic_name;
                    st.icon = ic;
                    st.accounts = acc_list;
                    st.geo_summary = format!("{} ({})", geo_info.ip, geo_info.country_code);
                    st.is_restricted = geo_info.is_restricted;
                    st.is_shield_enabled = sh_cfg.enabled;
                    st.is_balancer_enabled = b_cfg.enabled;
                    st.balancer_strategy = b_cfg.strategy;
                }

                // Notify tray host of layout/icon change
                let _ = service.update(|_tray: &mut AntigravityTray| {}).await;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_tray_icon() {
        let (icon, name) = icon::render_tray_icon(85.0, false);
        assert_eq!(icon.width, 48);
        assert_eq!(icon.height, 48);
        assert_eq!(icon.data.len(), 48 * 48 * 4);
        assert_eq!(name, "quota_85");

        let (exh_icon, exh_name) = icon::render_tray_icon(0.0, true);
        assert_eq!(exh_name, "quota_exhausted");
        assert_eq!(exh_icon.data.len(), 48 * 48 * 4);
    }

    #[test]
    fn test_tray_lock() {
        let temp_dir = std::env::temp_dir();
        let test_lock_file = temp_dir.join("test_agy_tray.lock");
        let _ = std::fs::remove_file(&test_lock_file);

        let lock1 = lock::TrayLock::acquire(Some(&test_lock_file));
        assert!(lock1.is_ok());

        let lock2 = lock::TrayLock::acquire(Some(&test_lock_file));
        assert!(matches!(lock2, Err(TrayLockError::AlreadyRunning(_))));

        drop(lock1);
        let lock3 = lock::TrayLock::acquire(Some(&test_lock_file));
        assert!(lock3.is_ok());
    }
}
