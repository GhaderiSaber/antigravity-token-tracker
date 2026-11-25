use super::models::{DiagnosticItem, DiagnosticStatus, DoctorReport};
use crate::auth::{discover_antigravity_desktop_app, discover_antigravity_ide};
use crate::balancer::has_account_credentials;
use crate::shield::{EgressGeo, ShieldConfig};
use crate::storage::{default_config_dir, load_accounts};
use std::time::Duration;

/// Evaluates Geo-Shield policy for the provided configuration and egress geolocation.
#[must_use]
pub fn build_geo_shield_diagnostic(cfg: &ShieldConfig, geo: &EgressGeo) -> DiagnosticItem {
    let (is_allowed, reason) = crate::shield::is_ip_allowed(geo, cfg);

    DiagnosticItem {
        component: "Geo-Shield Protection".into(),
        status: if is_allowed {
            DiagnosticStatus::Pass
        } else {
            DiagnosticStatus::Fail
        },
        details: format!(
            "Shield: {}, Egress IP: {} ({}), Policy: {}",
            if cfg.enabled { "ENABLED" } else { "DISABLED" },
            geo.ip,
            geo.country_code,
            reason
        ),
    }
}

/// Aggregates individual [`DiagnosticItem`] statuses into an overall [`DiagnosticStatus`].
#[must_use]
pub fn compute_overall_status(items: &[DiagnosticItem]) -> DiagnosticStatus {
    if items.iter().any(|item| item.status.is_fail()) {
        DiagnosticStatus::Fail
    } else if items.iter().any(|item| item.status.is_warn()) {
        DiagnosticStatus::Warn
    } else {
        DiagnosticStatus::Pass
    }
}

/// Runs the full diagnostic check suite across keyring, API connectivity, IDE/desktop discovery, accounts, and Geo-Shield.
pub async fn run_doctor_suite() -> DoctorReport {
    let mut items = Vec::new();
    let mut has_fail = false;
    let mut has_warn = false;

    // 1. Linux Secret Service (Keyring)
    let keyring_diag = match crate::auth::probe_keyring_session().await {
        Ok(()) => DiagnosticItem {
            component: "Linux Secret Service".into(),
            status: DiagnosticStatus::Pass,
            details: "org.freedesktop.secrets OpenSession/SearchItems verified; keyring ready"
                .into(),
        },
        Err(err) => {
            has_warn = true;
            DiagnosticItem {
                component: "Linux Secret Service".into(),
                status: DiagnosticStatus::Warn,
                details: format!(
                    "Secret Service session check failed ({err}); falling back to SQLite tokens"
                ),
            }
        }
    };
    items.push(keyring_diag);

    // 2. Google CloudCode API connectivity
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build();

    let google_ok = if let Ok(client) = &client {
        client
            .head("https://daily-cloudcode-pa.googleapis.com")
            .send()
            .await
            .is_ok()
    } else {
        false
    };

    if google_ok {
        items.push(DiagnosticItem {
            component: "Google CloudCode API".into(),
            status: DiagnosticStatus::Pass,
            details: "Network connection to daily-cloudcode-pa.googleapis.com verified".into(),
        });
    } else {
        has_warn = true;
        items.push(DiagnosticItem {
            component: "Google CloudCode API".into(),
            status: DiagnosticStatus::Warn,
            details: "Failed to connect to Google CloudCode API; offline or blocked".into(),
        });
    }

    // 3. Geolocation API connectivity
    let geo_ok = if let Ok(client) = &client {
        client.head("https://ipwho.is").send().await.is_ok()
    } else {
        false
    };

    if geo_ok {
        items.push(DiagnosticItem {
            component: "IP Geolocation API".into(),
            status: DiagnosticStatus::Pass,
            details: "https://ipwho.is reachable for Geo-Shield egress verification".into(),
        });
    } else {
        has_warn = true;
        items.push(DiagnosticItem {
            component: "IP Geolocation API".into(),
            status: DiagnosticStatus::Warn,
            details: "Geolocation API unreachable; Geo-Shield cannot verify egress IP".into(),
        });
    }

    // 4. Antigravity Desktop & IDE Processes
    let desktop = discover_antigravity_desktop_app().await;
    let ide = discover_antigravity_ide().await;

    if let Some(d) = desktop {
        items.push(DiagnosticItem {
            component: "Antigravity Desktop App".into(),
            status: DiagnosticStatus::Pass,
            details: format!(
                "Running (email: {}, port: {})",
                d.email,
                d.port.unwrap_or(0)
            ),
        });
    } else {
        items.push(DiagnosticItem {
            component: "Antigravity Desktop App".into(),
            status: DiagnosticStatus::Info,
            details: "Not currently running".into(),
        });
    }

    if let Some(i) = ide {
        items.push(DiagnosticItem {
            component: "Antigravity IDE".into(),
            status: DiagnosticStatus::Pass,
            details: format!(
                "Running (email: {}, port: {})",
                i.email,
                i.port.unwrap_or(0)
            ),
        });
    } else {
        items.push(DiagnosticItem {
            component: "Antigravity IDE".into(),
            status: DiagnosticStatus::Info,
            details: "Not currently running".into(),
        });
    }

    // 5. Registered Accounts Audit
    match load_accounts() {
        Ok(reg) => {
            let total = reg.accounts.len();
            let mut ready_count = 0;
            for em in reg.accounts.keys() {
                if has_account_credentials(em, Some(&reg)) {
                    ready_count += 1;
                }
            }

            if total == 0 {
                has_warn = true;
                items.push(DiagnosticItem {
                    component: "Registered Accounts".into(),
                    status: DiagnosticStatus::Warn,
                    details: "No registered accounts found. Run `agy-token sync` to import.".into(),
                });
            } else {
                items.push(DiagnosticItem {
                    component: "Registered Accounts".into(),
                    status: DiagnosticStatus::Pass,
                    details: format!(
                        "{total} registered accounts ({ready_count} switchable with credentials)"
                    ),
                });
            }
        }
        Err(e) => {
            has_fail = true;
            items.push(DiagnosticItem {
                component: "Accounts Storage".into(),
                status: DiagnosticStatus::Fail,
                details: format!("Failed reading accounts.json: {e}"),
            });
        }
    }

    // 6. Geo-Shield Audit
    let config_dir = default_config_dir();
    let cfg = crate::shield::load_shield_config(&config_dir);
    let geo = crate::shield::get_ip_geo(false).await;
    let geo_item = build_geo_shield_diagnostic(&cfg, &geo);
    if geo_item.status.is_fail() {
        has_fail = true;
    }
    items.push(geo_item);

    let overall = if has_fail {
        DiagnosticStatus::Fail
    } else if has_warn {
        DiagnosticStatus::Warn
    } else {
        DiagnosticStatus::Pass
    };

    DoctorReport {
        items,
        overall_status: overall,
        timestamp: chrono::Utc::now().timestamp_millis() as f64 / 1000.0,
    }
}
