use crate::balancer::BalancerStrategy;
use ksni::menu::*;
use ksni::{Icon, MenuItem, ToolTip, Tray};
use std::sync::{Arc, Mutex};

pub struct TrayState {
    pub min_pct: f64,
    pub is_exhausted: bool,
    pub active_summary: String,
    pub icon_name: String,
    pub icon: Icon,
    pub accounts: Vec<crate::storage::models::AccountProfile>,
    pub geo_summary: String,
    pub is_restricted: bool,
    pub is_shield_enabled: bool,
    pub is_balancer_enabled: bool,
    pub balancer_strategy: BalancerStrategy,
    pub backup_candidate: Option<(String, f64)>,
}

pub struct AntigravityTray {
    pub state: Arc<Mutex<TrayState>>,
}

impl Tray for AntigravityTray {
    fn id(&self) -> String {
        "antigravity-token-tracker".into()
    }

    fn title(&self) -> String {
        let st = self.state.lock().unwrap();
        if st.is_exhausted {
            "Antigravity: Quota Exhausted".into()
        } else {
            format!("Antigravity: {:.1}%", st.min_pct)
        }
    }

    fn icon_name(&self) -> String {
        let st = self.state.lock().unwrap();
        st.icon_name.clone()
    }

    fn icon_theme_path(&self) -> String {
        crate::tray::icon::default_icon_cache_dir()
            .to_string_lossy()
            .to_string()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        let st = self.state.lock().unwrap();
        vec![Icon {
            width: st.icon.width,
            height: st.icon.height,
            data: st.icon.data.clone(),
        }]
    }

    fn tool_tip(&self) -> ToolTip {
        let st = self.state.lock().unwrap();
        ToolTip {
            title: "Antigravity Token Tracker".into(),
            description: st.active_summary.clone(),
            icon_name: st.icon_name.clone(),
            icon_pixmap: vec![Icon {
                width: st.icon.width,
                height: st.icon.height,
                data: st.icon.data.clone(),
            }],
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = open::that("http://localhost:8765");
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let st = self.state.lock().unwrap();
        let mut items = Vec::new();

        // 1. Open Web Dashboard
        items.push(
            StandardItem {
                label: "🌐 Open Web Dashboard".into(),
                activate: Box::new(|_| {
                    let _ = open::that("http://localhost:8765");
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(MenuItem::Separator);

        // 2. Active Session summary
        let summary_label = if st.active_summary.is_empty() {
            "● No active session detected".to_string()
        } else {
            format!("● {}", st.active_summary)
        };
        items.push(
            StandardItem {
                label: summary_label,
                activate: Box::new(|_| {
                    let _ = open::that("http://localhost:8765");
                }),
                ..Default::default()
            }
            .into(),
        );

        // 3. Switch Account Submenu
        let mut switch_sub = Vec::new();
        for acc in &st.accounts {
            if acc.active == Some(true) {
                continue;
            }
            let email = acc.email.clone();
            let email_capture = email.clone();
            switch_sub.push(
                StandardItem {
                    label: format!("⚡ Switch to {email}"),
                    activate: Box::new(move |_| {
                        let target = email_capture.clone();
                        tokio::spawn(async move {
                            let (ok, msg) = match crate::balancer::switcher::switch_to_account(
                                &target,
                                true,
                                crate::balancer::switcher::SwitchSurface::Both,
                            )
                            .await
                            {
                                Ok(m) => (true, m),
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
                        });
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }

        if switch_sub.is_empty() {
            switch_sub.push(
                StandardItem {
                    label: "No other accounts registered".into(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }

        items.push(
            SubMenu {
                label: "🔄 Switch Account".into(),
                submenu: switch_sub,
                ..Default::default()
            }
            .into(),
        );

        items.push(MenuItem::Separator);

        // 4. IP Geolocation & Killswitch Shield
        let shield_icon = if st.is_shield_enabled {
            "🛡️"
        } else {
            "⚪"
        };
        let shield_state = if st.is_shield_enabled { "ON" } else { "OFF" };
        let shield_label = if st.is_restricted {
            format!("⚠️ RESTRICTED: {} (VPN Required!)", st.geo_summary)
        } else if !st.geo_summary.is_empty() {
            format!(
                "{} IP: {} [Shield: {}]",
                shield_icon, st.geo_summary, shield_state
            )
        } else {
            format!("{shield_icon} IP Killswitch Shield: [{shield_state}]")
        };

        items.push(
            StandardItem {
                label: shield_label,
                activate: Box::new(|_| {
                    let storage = crate::storage::StorageManager::default();
                    let mut cfg = crate::shield::shield::load_shield_config(storage.config_dir());
                    cfg.enabled = !cfg.enabled;
                    let _ = crate::shield::shield::save_shield_config(storage.config_dir(), &cfg);
                    let desc = if cfg.enabled {
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
                }),
                ..Default::default()
            }
            .into(),
        );

        // 5. Failover Candidate / Auto-Balancer
        if let Some((ref cand_email, cand_pct)) = st.backup_candidate {
            let cand = cand_email.clone();
            items.push(
                StandardItem {
                    label: format!("⚡ Failover: Auto-Switch ({cand} {cand_pct:.1}%)"),
                    activate: Box::new(move |_| {
                        let target = cand.clone();
                        tokio::spawn(async move {
                            let (ok, msg) = match crate::balancer::switcher::switch_to_account(
                                &target,
                                true,
                                crate::balancer::switcher::SwitchSurface::Both,
                            )
                            .await
                            {
                                Ok(m) => (true, m),
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
                        });
                    }),
                    ..Default::default()
                }
                .into(),
            );
        } else {
            items.push(
                StandardItem {
                    label: "🛡️ Failover: Standby None available".into(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }

        let bal_icon = if st.is_balancer_enabled {
            "🔄"
        } else {
            "⚪"
        };
        let bal_state = if st.is_balancer_enabled { "ON" } else { "OFF" };
        let bal_strat = &st.balancer_strategy;
        items.push(
            StandardItem {
                label: format!("{bal_icon} Auto-Balancer: [{bal_state} - {bal_strat}]"),
                activate: Box::new(|_| {
                    let mut cfg = crate::balancer::balancer::load_balancer_config();
                    cfg.enabled = !cfg.enabled;
                    let _ = crate::balancer::balancer::save_balancer_config(&cfg);
                    let desc = if cfg.enabled { "ENABLED" } else { "DISABLED" };
                    tokio::spawn(async move {
                        let _ = crate::daemon::notifier::send_desktop_notification(
                            "🔄 Auto-Balancer",
                            &format!("Multi-account pool rotation is now {desc}."),
                            "normal",
                        )
                        .await;
                    });
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(MenuItem::Separator);

        // 6. Refresh Quotas
        items.push(
            StandardItem {
                label: "↻ Refresh Quotas".into(),
                activate: Box::new(|_| {
                    tokio::spawn(async {
                        let _ = crate::quota::quota::fetch_all_accounts_quota(true).await;
                    });
                }),
                ..Default::default()
            }
            .into(),
        );

        // 7. Quit
        items.push(
            StandardItem {
                label: "❌ Quit Applet".into(),
                activate: Box::new(|_| {
                    std::process::exit(0);
                }),
                ..Default::default()
            }
            .into(),
        );

        items
    }
}
