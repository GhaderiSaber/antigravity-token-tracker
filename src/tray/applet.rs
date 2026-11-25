//! System tray menu, state snapshot, and command channel definitions.

use crate::balancer::BalancerStrategy;
use crate::storage::models::AccountProfile;
use crate::tray::icon::{QuotaIndicator, default_icon_cache_dir};
use ksni::menu::{StandardItem, SubMenu};
use ksni::{Icon, MenuItem, OfflineReason, ToolTip, Tray};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

/// Commands emitted by tray menu callbacks and icon activation for async handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayCommand {
    /// Force-refresh account quotas and update the tray indicator immediately.
    RefreshQuotas,
    /// Toggle IP Killswitch Shield on or off and persist configuration.
    ToggleShield,
    /// Toggle Multi-Account Auto-Balancer on or off and persist configuration.
    ToggleBalancer,
    /// Switch the active Antigravity session to the given account email.
    SwitchAccount(String),
    /// Open the local web dashboard in the user's default browser.
    OpenDashboard,
    /// Exit the tray event loop cleanly so D-Bus and lock resources tear down.
    Quit,
}

/// Shared snapshot of tray indicator status and menu data rendered by [`AntigravityTray`].
#[derive(Debug, Clone)]
pub struct TrayState {
    /// Current quota indicator derived from active session usage.
    pub indicator: QuotaIndicator,
    /// Email address of the currently active session, if any.
    pub active_email: Option<String>,
    /// Human-readable active session status shown in the tooltip and menu header.
    pub active_summary: String,
    /// Freedesktop icon theme name matching [`Self::indicator`].
    pub icon_name: String,
    /// Raw ARGB32 icon bytes shared across tooltip and pixmap queries.
    pub icon_data: Arc<Vec<u8>>,
    /// Width of [`Self::icon_data`] in pixels.
    pub icon_width: i32,
    /// Height of [`Self::icon_data`] in pixels.
    pub icon_height: i32,
    /// Registered accounts available to display in the switch submenu.
    pub accounts: Vec<AccountProfile>,
    /// Formatted egress IP and country code summary.
    pub geo_summary: String,
    /// Whether the current egress IP is in a restricted region.
    pub is_restricted: bool,
    /// Whether the IP Killswitch Shield is enabled.
    pub is_shield_enabled: bool,
    /// Whether the Multi-Account Auto-Balancer is enabled.
    pub is_balancer_enabled: bool,
    /// Configured rotation strategy for the Auto-Balancer.
    pub balancer_strategy: BalancerStrategy,
    /// Highest-quota standby failover candidate `(email, remaining_percent)`, if any.
    pub backup_candidate: Option<(String, f64)>,
    /// Primary weekly quota percentage keyed by lowercase account email.
    pub account_quotas: HashMap<String, f64>,
}

/// Filters registered accounts to those eligible for manual switching.
///
/// Excludes accounts matching `active_email` (case-insensitive, trimmed) and accounts
/// currently marked as an active desktop or IDE session. Legacy `active == Some(true)`
/// flags are intentionally ignored so stale registry entries do not hide valid targets.
#[must_use]
pub fn switchable_accounts<'a>(
    accounts: &'a [AccountProfile],
    active_email: Option<&str>,
) -> Vec<&'a AccountProfile> {
    let normalized_active = active_email
        .map(|email| email.trim().to_lowercase())
        .filter(|email| !email.is_empty());

    accounts
        .iter()
        .filter(|acc| {
            let matches_active_email = normalized_active
                .as_deref()
                .is_some_and(|active| acc.email.trim().to_lowercase() == active);
            let is_desktop = acc.is_current_desktop_session == Some(true);
            let is_ide = acc.is_current_ide_session == Some(true);
            !matches_active_email && !is_desktop && !is_ide
        })
        .collect()
}

/// `ksni` StatusNotifierItem tray implementation backed by shared [`TrayState`]
/// and a non-blocking [`TrayCommand`] channel.
#[derive(Debug)]
pub struct AntigravityTray {
    /// Shared snapshot of tray status and menu data.
    pub state: Arc<Mutex<TrayState>>,
    /// Channel sender used by synchronous D-Bus callbacks to dispatch async actions.
    pub cmd_tx: UnboundedSender<TrayCommand>,
}

impl AntigravityTray {
    fn snapshot(&self) -> TrayState {
        self.state.lock().expect("tray state lock poisoned").clone()
    }

    fn send_cmd(&self, cmd: TrayCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}

impl Tray for AntigravityTray {
    fn id(&self) -> String {
        "antigravity-token-tracker".into()
    }

    fn title(&self) -> String {
        let st = self.state.lock().expect("tray state lock poisoned");
        st.indicator.title_text()
    }

    fn icon_name(&self) -> String {
        let st = self.state.lock().expect("tray state lock poisoned");
        st.icon_name.clone()
    }

    fn icon_theme_path(&self) -> String {
        default_icon_cache_dir().to_string_lossy().into_owned()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        let (width, height, data) = {
            let st = self.state.lock().expect("tray state lock poisoned");
            (st.icon_width, st.icon_height, Arc::clone(&st.icon_data))
        };
        vec![Icon {
            width,
            height,
            data: (*data).clone(),
        }]
    }

    fn tool_tip(&self) -> ToolTip {
        let (description, icon_name, width, height, data) = {
            let st = self.state.lock().expect("tray state lock poisoned");
            (
                st.active_summary.clone(),
                st.icon_name.clone(),
                st.icon_width,
                st.icon_height,
                Arc::clone(&st.icon_data),
            )
        };
        ToolTip {
            title: "Antigravity Token Tracker".into(),
            description,
            icon_name,
            icon_pixmap: vec![Icon {
                width,
                height,
                data: (*data).clone(),
            }],
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send_cmd(TrayCommand::OpenDashboard);
    }

    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        eprintln!(
            "[Tray] Warning: StatusNotifierWatcher went offline ({reason:?}); waiting for reconnect."
        );
        true
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let st = self.snapshot();
        let mut items = Vec::new();

        // 1. Open Web Dashboard
        items.push(
            StandardItem {
                label: "🌐 Open Web Dashboard".into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.send_cmd(TrayCommand::OpenDashboard);
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
                activate: Box::new(|tray: &mut Self| {
                    tray.send_cmd(TrayCommand::OpenDashboard);
                }),
                ..Default::default()
            }
            .into(),
        );

        // 3. Switch Account Submenu
        let mut switch_sub: Vec<MenuItem<Self>> =
            switchable_accounts(&st.accounts, st.active_email.as_deref())
                .into_iter()
                .map(|acc| {
                    let email = acc.email.clone();
                    let label = match st.account_quotas.get(&email.trim().to_lowercase()) {
                        Some(pct) => format!("⚡ Switch to {email} ({pct:.1}%)"),
                        None => format!("⚡ Switch to {email}"),
                    };
                    StandardItem {
                        label,
                        activate: Box::new(move |tray: &mut Self| {
                            tray.send_cmd(TrayCommand::SwitchAccount(email.clone()));
                        }),
                        ..Default::default()
                    }
                    .into()
                })
                .collect();

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
                "{shield_icon} IP: {} [Shield: {shield_state}]",
                st.geo_summary
            )
        } else {
            format!("{shield_icon} IP Killswitch Shield: [{shield_state}]")
        };

        items.push(
            StandardItem {
                label: shield_label,
                activate: Box::new(|tray: &mut Self| {
                    tray.send_cmd(TrayCommand::ToggleShield);
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
                    activate: Box::new(move |tray: &mut Self| {
                        tray.send_cmd(TrayCommand::SwitchAccount(cand.clone()));
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
                activate: Box::new(|tray: &mut Self| {
                    tray.send_cmd(TrayCommand::ToggleBalancer);
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
                activate: Box::new(|tray: &mut Self| {
                    tray.send_cmd(TrayCommand::RefreshQuotas);
                }),
                ..Default::default()
            }
            .into(),
        );

        // 7. Quit
        items.push(
            StandardItem {
                label: "❌ Quit Applet".into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.send_cmd(TrayCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        );

        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_switch_submenu_labels_include_quota_percent_when_available_and_omit_otherwise() {
        let mut account_quotas = HashMap::new();
        account_quotas.insert("backup@example.com".to_string(), 87.5);
        account_quotas.insert("mixcase@example.com".to_string(), 42.3);

        let state = TrayState {
            indicator: QuotaIndicator::Available(60.0),
            active_email: Some("active@example.com".into()),
            active_summary: "active@example.com: 60.0% remaining".into(),
            icon_name: "quota_60".into(),
            icon_data: Arc::new(Vec::new()),
            icon_width: 48,
            icon_height: 48,
            accounts: vec![
                AccountProfile {
                    email: "active@example.com".into(),
                    is_current_desktop_session: Some(true),
                    ..Default::default()
                },
                AccountProfile {
                    email: "backup@example.com".into(),
                    ..Default::default()
                },
                AccountProfile {
                    email: "MixCase@Example.COM".into(),
                    ..Default::default()
                },
                AccountProfile {
                    email: "unpolled@example.com".into(),
                    ..Default::default()
                },
            ],
            geo_summary: String::new(),
            is_restricted: false,
            is_shield_enabled: false,
            is_balancer_enabled: false,
            balancer_strategy: BalancerStrategy::ExpiryFirst,
            backup_candidate: Some(("backup@example.com".into(), 87.5)),
            account_quotas,
        };

        let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let tray = AntigravityTray {
            state: Arc::new(Mutex::new(state)),
            cmd_tx,
        };

        let menu = tray.menu();
        let switch_labels: Vec<String> = menu
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
            switch_labels,
            vec![
                "⚡ Switch to backup@example.com (87.5%)",
                "⚡ Switch to MixCase@Example.COM (42.3%)",
                "⚡ Switch to unpolled@example.com",
            ]
        );
    }
}
