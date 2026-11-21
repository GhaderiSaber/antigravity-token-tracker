use super::models::{BalancerConfig, BalancerState, BalancerStrategy};
use super::switcher::{SwitchSurface, has_account_credentials, switch_to_account};
use crate::quota::AccountQuotaResult;
use crate::storage::load_accounts;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub fn default_balancer_config_file() -> PathBuf {
    dirs::config_dir().map_or_else(
        || PathBuf::from(".config/antigravity-token-tracker/balancer.json"),
        |c| c.join("antigravity-token-tracker/balancer.json"),
    )
}

pub fn default_balancer_state_file() -> PathBuf {
    dirs::config_dir().map_or_else(
        || PathBuf::from(".config/antigravity-token-tracker/balancer_state.json"),
        |c| c.join("antigravity-token-tracker/balancer_state.json"),
    )
}

pub const SUPPORTED_STRATEGIES: &[&str] = &["watermark", "expiry_first", "round_robin", "reactive"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PoolAccount {
    pub email: String,
    pub quota_pct: f64,
    pub is_exhausted: bool,
    pub reset_seconds: i64,
    pub is_active: bool,
    pub is_eligible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BalancerDecision {
    pub triggered: bool,
    pub reason: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<BalancerStrategy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_quota_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_quota_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_remaining: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn load_balancer_config() -> BalancerConfig {
    let path = default_balancer_config_file();
    if path.is_file()
        && let Ok(content) = fs::read_to_string(&path)
        && let Ok(cfg) = serde_json::from_str::<BalancerConfig>(&content)
    {
        return cfg;
    }
    BalancerConfig::default()
}

pub fn save_balancer_config(cfg: &BalancerConfig) -> std::io::Result<()> {
    let path = default_balancer_config_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, content)
}

pub fn load_balancer_state() -> BalancerState {
    let path = default_balancer_state_file();
    if path.is_file()
        && let Ok(content) = fs::read_to_string(&path)
        && let Ok(state) = serde_json::from_str::<BalancerState>(&content)
    {
        return state;
    }
    BalancerState::default()
}

pub fn save_balancer_state(state: &BalancerState) -> std::io::Result<()> {
    let path = default_balancer_state_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, content)
}

pub fn get_account_reset_seconds(account_data: &AccountQuotaResult) -> i64 {
    for g in &account_data.groups {
        if let Some(w) = &g.weekly
            && let Some(secs) = w.seconds_remaining
        {
            return secs as i64;
        }
    }
    604_800
}

pub fn get_eligible_pool_accounts(
    analyzed_quotas: &HashMap<String, AccountQuotaResult>,
    min_quota_pct: f64,
) -> Vec<PoolAccount> {
    let registered = load_accounts().unwrap_or_default();
    let mut pool = Vec::new();

    for (email, q) in analyzed_quotas {
        let clean = email.trim().to_lowercase();
        if !has_account_credentials(&clean, Some(&registered)) {
            continue;
        }

        let mut rem_pct = 0.0;
        let mut is_exh = false;
        for g in &q.groups {
            if let Some(w) = &g.weekly {
                if g.display_name.to_lowercase().contains("gemini") {
                    rem_pct = w.remaining_percent.unwrap_or(0.0);
                    is_exh = w.is_exhausted.unwrap_or(false);
                    break;
                }
                if rem_pct == 0.0 {
                    rem_pct = w.remaining_percent.unwrap_or(0.0);
                    is_exh = w.is_exhausted.unwrap_or(false);
                }
            }
        }

        let reset_secs = get_account_reset_seconds(q);
        let is_active = q.is_current_desktop_session || q.is_current_ide_session;
        let is_eligible = rem_pct >= min_quota_pct && !is_exh;

        pool.push(PoolAccount {
            email: email.clone(),
            quota_pct: rem_pct,
            is_exhausted: is_exh,
            reset_seconds: reset_secs,
            is_active,
            is_eligible,
        });
    }

    pool
}

pub fn evaluate_watermark_strategy(
    active_acc: Option<&PoolAccount>,
    candidates: &[PoolAccount],
    spread_pct: f64,
) -> Option<(PoolAccount, String)> {
    if candidates.is_empty() {
        return None;
    }

    let best = candidates
        .iter()
        .max_by(|a, b| a.quota_pct.total_cmp(&b.quota_pct))?;
    let active_pct = active_acc.map_or(0.0, |a| a.quota_pct);
    let spread = best.quota_pct - active_pct;

    if spread >= spread_pct {
        let reason = format!(
            "Quota spread of {:.1}% exceeds watermark threshold ({:.1}%): candidate has {:.1}% vs active {:.1}%",
            spread, spread_pct, best.quota_pct, active_pct
        );
        return Some((best.clone(), reason));
    }

    None
}

pub fn evaluate_expiry_first_strategy(
    active_acc: Option<&PoolAccount>,
    candidates: &[PoolAccount],
    expiry_window_hours: u64,
    min_quota_pct: f64,
) -> Option<(PoolAccount, String)> {
    if candidates.is_empty() {
        return None;
    }

    let window_secs = (expiry_window_hours * 3600) as i64;
    let expiring_soon: Vec<&PoolAccount> = candidates
        .iter()
        .filter(|c| c.reset_seconds <= window_secs && c.quota_pct >= min_quota_pct)
        .collect();

    if !expiring_soon.is_empty() {
        let earliest = expiring_soon.into_iter().min_by_key(|c| c.reset_seconds)?;
        let hours_left = earliest.reset_seconds as f64 / 3600.0;
        let reason = format!(
            "Quota resets in {:.1}h with {:.1}% remaining (prioritizing before weekly reset)",
            hours_left, earliest.quota_pct
        );
        return Some((earliest.clone(), reason));
    }

    if let Some(active) = active_acc
        && active.quota_pct <= min_quota_pct
    {
        let healthiest = candidates
            .iter()
            .max_by(|a, b| a.quota_pct.total_cmp(&b.quota_pct))?;
        let reason = format!(
            "Active account is low ({:.1}%); rotating to healthiest account ({:.1}%)",
            active.quota_pct, healthiest.quota_pct
        );
        return Some((healthiest.clone(), reason));
    }

    None
}

pub fn evaluate_round_robin_strategy(
    active_acc: Option<&PoolAccount>,
    pool: &[PoolAccount],
    interval_minutes: u64,
    last_rotation_ts: f64,
    now: f64,
) -> Option<(PoolAccount, String)> {
    let eligible: Vec<&PoolAccount> = pool.iter().filter(|a| a.is_eligible).collect();
    if eligible.len() < 2 {
        return None;
    }

    let time_elapsed = now - last_rotation_ts;
    let interval_secs = (interval_minutes * 60) as f64;
    if time_elapsed < interval_secs {
        return None;
    }

    let current_email = active_acc
        .map(|a| a.email.to_lowercase())
        .unwrap_or_default();
    let current_idx = eligible
        .iter()
        .position(|a| a.email.to_lowercase() == current_email)
        .unwrap_or(0);

    let next_idx = (current_idx + 1) % eligible.len();
    let next_acc = eligible[next_idx];
    if next_acc.email.to_lowercase() == current_email {
        return None;
    }

    let elapsed_min = (time_elapsed / 60.0) as u64;
    let reason = format!(
        "Round-robin rotation window reached ({elapsed_min}m elapsed >= {interval_minutes}m target)"
    );
    Some((next_acc.clone(), reason))
}

pub async fn evaluate_and_execute_balancer(
    analyzed_quotas: &HashMap<String, AccountQuotaResult>,
    dry_run: bool,
    force: bool,
    strategy_override: Option<BalancerStrategy>,
) -> BalancerDecision {
    let cfg = load_balancer_config();
    let mut state = load_balancer_state();
    let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;

    if !cfg.enabled && !force {
        return BalancerDecision {
            triggered: false,
            reason: "BALANCER_DISABLED".into(),
            message: "Auto-balancer is disabled. Enable via 'agy-token balance on'.".into(),
            dry_run: None,
            strategy: None,
            from_account: None,
            from_quota_pct: None,
            to_account: None,
            to_quota_pct: None,
            cooldown_remaining: None,
            error: None,
        };
    }

    let strategy = strategy_override.unwrap_or(cfg.strategy);
    if strategy == BalancerStrategy::Reactive {
        return BalancerDecision {
            triggered: false,
            reason: "STRATEGY_REACTIVE".into(),
            message:
                "Balancer strategy is set to 'reactive' (emergency failover handles depletion)."
                    .into(),
            dry_run: None,
            strategy: Some(strategy),
            from_account: None,
            from_quota_pct: None,
            to_account: None,
            to_quota_pct: None,
            cooldown_remaining: None,
            error: None,
        };
    }

    // Cooldown check
    let cooldown = cfg.cooldown_seconds as f64;
    let last_ts = state.last_rotation_timestamp.unwrap_or(0.0);
    let time_since = now - last_ts;
    if time_since < cooldown && !force {
        let remaining_cd = (cooldown - time_since).round() as i64;
        return BalancerDecision {
            triggered: false,
            reason: "COOLDOWN".into(),
            message: format!(
                "Rotation cooldown active ({remaining_cd}s remaining). Skipping rotation."
            ),
            dry_run: None,
            strategy: Some(strategy),
            from_account: None,
            from_quota_pct: None,
            to_account: None,
            to_quota_pct: None,
            cooldown_remaining: Some(remaining_cd),
            error: None,
        };
    }

    let pool = get_eligible_pool_accounts(analyzed_quotas, cfg.min_quota_pct);
    if pool.len() < 2 {
        return BalancerDecision {
            triggered: false,
            reason: "INSUFFICIENT_POOL".into(),
            message: format!(
                "Auto-balancer requires at least 2 registered switchable accounts (found {}).",
                pool.len()
            ),
            dry_run: None,
            strategy: Some(strategy),
            from_account: None,
            from_quota_pct: None,
            to_account: None,
            to_quota_pct: None,
            cooldown_remaining: None,
            error: None,
        };
    }

    let active_acc = pool.iter().find(|a| a.is_active);
    let candidates: Vec<PoolAccount> = pool
        .iter()
        .filter(|a| a.is_eligible && !a.is_active)
        .cloned()
        .collect();

    let rotation_decision = match strategy {
        BalancerStrategy::Watermark => {
            evaluate_watermark_strategy(active_acc, &candidates, cfg.watermark_spread_pct)
        }
        BalancerStrategy::ExpiryFirst => evaluate_expiry_first_strategy(
            active_acc,
            &candidates,
            cfg.expiry_window_hours,
            cfg.min_quota_pct,
        ),
        BalancerStrategy::RoundRobin => evaluate_round_robin_strategy(
            active_acc,
            &pool,
            cfg.round_robin_interval_minutes,
            last_ts,
            now,
        ),
        BalancerStrategy::Reactive => None,
    };

    let Some((target_acc, reason_msg)) = rotation_decision else {
        return BalancerDecision {
            triggered: false,
            reason: "OPTIMAL".into(),
            message: format!(
                "Account pool is balanced under '{strategy}' strategy. No rotation needed."
            ),
            dry_run: None,
            strategy: Some(strategy),
            from_account: active_acc.map(|a| a.email.clone()),
            from_quota_pct: active_acc.map(|a| a.quota_pct),
            to_account: None,
            to_quota_pct: None,
            cooldown_remaining: None,
            error: None,
        };
    };

    let target_email = target_acc.email.clone();
    let target_pct = target_acc.quota_pct;
    let from_email = active_acc.map_or_else(|| "None".into(), |a| a.email.clone());
    let from_pct = active_acc.map_or(0.0, |a| a.quota_pct);

    if dry_run {
        return BalancerDecision {
            triggered: true,
            reason: reason_msg.clone(),
            message: format!(
                "[DRY RUN] Would rotate from {from_email} ({from_pct:.1}%) to {target_email} ({target_pct:.1}%): {reason_msg}"
            ),
            dry_run: Some(true),
            strategy: Some(strategy),
            from_account: Some(from_email),
            from_quota_pct: Some(from_pct),
            to_account: Some(target_email),
            to_quota_pct: Some(target_pct),
            cooldown_remaining: None,
            error: None,
        };
    }

    match switch_to_account(&target_email, cfg.auto_restart, SwitchSurface::Both).await {
        Ok(_switch_msg) => {
            state.last_rotation_timestamp = Some(now);
            state.last_from_email = Some(from_email.clone());
            state.last_to_email = Some(target_email.clone());
            state.rotation_count += 1;
            let _ = save_balancer_state(&state);

            BalancerDecision {
                triggered: true,
                reason: reason_msg.clone(),
                message: format!(
                    "Successfully rotated to {target_email} ({target_pct:.1}% quota): {reason_msg}"
                ),
                dry_run: Some(false),
                strategy: Some(strategy),
                from_account: Some(from_email),
                from_quota_pct: Some(from_pct),
                to_account: Some(target_email),
                to_quota_pct: Some(target_pct),
                cooldown_remaining: None,
                error: None,
            }
        }
        Err(err) => {
            let err_msg = err.to_string();
            BalancerDecision {
                triggered: false,
                reason: "SWITCH_FAILED".into(),
                message: format!("Rotation switch failed: {err_msg}"),
                dry_run: Some(false),
                strategy: Some(strategy),
                from_account: Some(from_email),
                from_quota_pct: Some(from_pct),
                to_account: Some(target_email),
                to_quota_pct: Some(target_pct),
                cooldown_remaining: None,
                error: Some(err_msg),
            }
        }
    }
}
