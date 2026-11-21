use crate::balancer::models::BalancerConfig;
use crate::quota::models::AccountQuotaResult;
use crate::shield::models::{EgressGeo, ShieldConfig};
use std::collections::HashMap;

pub fn format_progress_bar(pct: f64) -> String {
    let filled = ((pct / 100.0) * 10.0).round() as usize;
    let filled = filled.min(10);
    let empty = 10 - filled;
    format!("{:.1}% [{}{}]", pct, "█".repeat(filled), "░".repeat(empty))
}

pub fn render_account_table(email: &str, account: &AccountQuotaResult, is_active: bool) -> String {
    let mut out = String::new();
    let tier = if account.tier.is_empty() {
        "Standard"
    } else {
        &account.tier
    };
    let active_str = if is_active { " (● ACTIVE)" } else { "" };
    out.push_str(&format!(
        "╭─ Account: {} [{}{}] ──────────────────────────────╮\n",
        email, tier, active_str
    ));
    out.push_str(&format!(
        "│ {:<28} │ {:<18} │ {:<15} │ {:<12} │\n",
        "Group Name", "Weekly Quota", "Reset In", "Status"
    ));
    out.push_str(
        "├──────────────────────────────┼────────────────────┼─────────────────┼──────────────┤\n",
    );

    if account.groups.is_empty() {
        out.push_str(&format!(
            "│ {:<28} │ {:<18} │ {:<15} │ {:<12} │\n",
            "No quota recorded", "-", "-", "NO DATA"
        ));
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

            out.push_str(&format!(
                "│ {:<28} │ {:<18} │ {:<15} │ {:<12} │\n",
                g_name, w_bar, w_reset, badge
            ));
        }
    }

    out.push_str(
        "╰──────────────────────────────┴────────────────────┴─────────────────┴──────────────╯\n",
    );
    out
}

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
    out.push_str(&format!(
        "│ IP: {} {} ({}, {}) | Status: {:<20} │\n",
        geo_info.flag, geo_info.ip, geo_info.city, geo_info.country_code, safe_badge
    ));
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
    out.push_str(&format!(
        "│ Geo-Shield: {:<10} | Mode: {:<12} | Action: {:<20} │\n",
        if shield_cfg.enabled {
            "ENABLED"
        } else {
            "DISABLED"
        },
        shield_cfg.mode,
        shield_cfg.action
    ));
    out.push_str(&format!(
        "│ Auto-Balancer: {:<7} | Strategy: {:<8} | Watermark Spread: {:<13} │\n",
        if bal_cfg.enabled {
            "ENABLED"
        } else {
            "DISABLED"
        },
        bal_cfg.strategy,
        format!("{:.1}%", bal_cfg.watermark_spread_pct)
    ));
    out.push_str(
        "╰──────────────────────────────────────────────────────────────────────────────╯\n",
    );

    out
}
