use crate::balancer::models::BalancerConfig;
use crate::quota::models::AccountQuotaResult;
use crate::shield::models::{EgressGeo, ShieldConfig};
use std::collections::HashMap;
use std::fmt::Write;

#[must_use]
pub fn format_progress_bar(pct: f64) -> String {
    let filled = ((pct / 100.0) * 10.0).round() as usize;
    let filled = filled.min(10);
    let empty = 10 - filled;
    format!("{:.1}% [{}{}]", pct, "█".repeat(filled), "░".repeat(empty))
}

#[must_use]
pub fn render_account_table(email: &str, account: &AccountQuotaResult, is_active: bool) -> String {
    let mut out = String::new();
    let tier = if account.tier.is_empty() {
        "Standard"
    } else {
        &account.tier
    };
    let active_str = if is_active { " (● ACTIVE)" } else { "" };
    let _ = writeln!(
        out,
        "╭─ Account: {email} [{tier}{active_str}] ──────────────────────────────╮"
    );
    let _ = writeln!(
        out,
        "│ {:<28} │ {:<18} │ {:<15} │ {:<12} │",
        "Group Name", "Weekly Quota", "Reset In", "Status"
    );
    out.push_str(
        "├──────────────────────────────┼────────────────────┼─────────────────┼──────────────┤\n",
    );

    if account.groups.is_empty() {
        let _ = writeln!(
            out,
            "│ {:<28} │ {:<18} │ {:<15} │ {:<12} │",
            "No quota recorded", "-", "-", "NO DATA"
        );
    } else {
        for g in &account.groups {
            let g_name = if g.display_name.len() > 28 {
                format!("{}...", &g.display_name[..25])
            } else {
                g.display_name.clone()
            };
            let w_pct = g
                .weekly
                .as_ref()
                .and_then(|w| w.remaining_percent)
                .unwrap_or(0.0);
            let w_bar = format_progress_bar(w_pct);
            let w_reset = g
                .weekly
                .as_ref()
                .and_then(|w| w.countdown.as_deref())
                .unwrap_or("-");
            let badge = g
                .weekly
                .as_ref()
                .and_then(|w| w.status_badge.as_deref())
                .unwrap_or("HEALTHY");

            let _ = writeln!(
                out,
                "│ {g_name:<28} │ {w_bar:<18} │ {w_reset:<15} │ {badge:<12} │"
            );
        }
    }

    out.push_str(
        "╰──────────────────────────────┴────────────────────┴─────────────────┴──────────────╯\n",
    );
    out
}

#[must_use]
pub fn render_status_dashboard(
    all_quotas: &HashMap<String, AccountQuotaResult>,
    geo_info: &EgressGeo,
    shield_cfg: &ShieldConfig,
    bal_cfg: &BalancerConfig,
) -> String {
    let mut out = String::new();

    // 1. IP Header
    out.push_str(
        "\n╭─ 🌐 Egress IP & Network Status ──────────────────────────────────────────────╮\n",
    );
    let safe_badge = if geo_info.is_restricted {
        "⚠️ RESTRICTED REGION - VPN REQUIRED"
    } else {
        "✓ Safe Egress"
    };
    let _ = writeln!(
        out,
        "│ IP: {} {} ({}, {}) | Status: {:<20} │",
        geo_info.flag, geo_info.ip, geo_info.city, geo_info.country_code, safe_badge
    );
    out.push_str(
        "╰──────────────────────────────────────────────────────────────────────────────╯\n\n",
    );

    // 2. Account Quotas
    for (email, q) in all_quotas {
        let is_active = q.is_current_desktop_session || q.is_current_ide_session;
        out.push_str(&render_account_table(email, q, is_active));
        out.push('\n');
    }

    // 3. Shield & Balancer Summary
    out.push_str(
        "╭─ 🛡️ System Protection & Automation ──────────────────────────────────────────╮\n",
    );
    let _ = writeln!(
        out,
        "│ Geo-Shield: {:<10} | Mode: {:<12} | Action: {:<20} │",
        if shield_cfg.enabled {
            "ENABLED"
        } else {
            "DISABLED"
        },
        shield_cfg.mode,
        shield_cfg.action
    );
    let _ = writeln!(
        out,
        "│ Auto-Balancer: {:<7} | Strategy: {:<8} | Watermark Spread: {:<13} │",
        if bal_cfg.enabled {
            "ENABLED"
        } else {
            "DISABLED"
        },
        bal_cfg.strategy,
        format!("{:.1}%", bal_cfg.watermark_spread_pct)
    );
    out.push_str(
        "╰──────────────────────────────────────────────────────────────────────────────╯\n",
    );

    out
}
