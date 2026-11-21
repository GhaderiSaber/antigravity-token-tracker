use super::models::{FailoverDecision, FailoverEvent, FailoverState};
use super::notifier::send_desktop_notification;
use crate::balancer::{SwitchSurface, has_account_credentials, switch_to_account};
use crate::quota::AccountQuotaResult;
use crate::storage::load_accounts;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub fn default_failover_state_file() -> PathBuf {
    dirs::config_dir().map_or_else(
        || PathBuf::from(".config/antigravity-token-tracker/failover_state.json"),
        |c| c.join("antigravity-token-tracker/failover_state.json"),
    )
}

pub fn load_failover_state() -> FailoverState {
    let path = default_failover_state_file();
    if path.is_file()
        && let Ok(content) = fs::read_to_string(&path)
        && let Ok(state) = serde_json::from_str::<FailoverState>(&content)
    {
        return state;
    }
    FailoverState::default()
}

pub fn save_failover_state(state: &FailoverState) -> std::io::Result<()> {
    let path = default_failover_state_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, content)
}

pub fn get_account_primary_quota(q: &AccountQuotaResult) -> (f64, bool) {
    let mut gemini_pct = None;
    let mut first_pct = None;
    let mut is_gemini_exh = false;

    for g in &q.groups {
        if let Some(w) = &g.weekly {
            let pct = w.remaining_percent.unwrap_or(0.0);
            let exh = w.is_exhausted.unwrap_or(false);
            if first_pct.is_none() {
                first_pct = Some(pct);
            }
            if g.display_name.to_lowercase().contains("gemini") {
                gemini_pct = Some(pct);
                is_gemini_exh = exh;
                break;
            }
        }
    }

    let final_pct = gemini_pct.or(first_pct).unwrap_or(0.0);
    let is_exhausted = final_pct <= 1.0 || is_gemini_exh;
    (final_pct, is_exhausted)
}

pub fn get_active_sessions(
    analyzed_quotas: &HashMap<String, AccountQuotaResult>,
) -> HashMap<String, (String, AccountQuotaResult)> {
    let mut sessions = HashMap::new();
    for (email, q) in analyzed_quotas {
        if q.is_current_desktop_session {
            sessions.insert("desktop".into(), (email.clone(), q.clone()));
        }
        if q.is_current_ide_session {
            sessions.insert("ide".into(), (email.clone(), q.clone()));
        }
    }
    sessions
}

pub fn get_active_account(
    analyzed_quotas: &HashMap<String, AccountQuotaResult>,
) -> Option<(String, AccountQuotaResult)> {
    for (email, q) in analyzed_quotas {
        if q.is_current_desktop_session {
            return Some((email.clone(), q.clone()));
        }
    }
    for (email, q) in analyzed_quotas {
        if q.is_current_ide_session {
            return Some((email.clone(), q.clone()));
        }
    }
    None
}

pub fn find_best_failover_candidate(
    analyzed_quotas: &HashMap<String, AccountQuotaResult>,
    current_email: Option<&str>,
    exclude_emails: &[String],
    min_threshold_pct: f64,
) -> Option<(String, f64)> {
    let registered = load_accounts().unwrap_or_default();
    let mut excludes: HashSet<String> = HashSet::new();
    if let Some(curr) = current_email {
        excludes.insert(curr.trim().to_lowercase());
    }
    for e in exclude_emails {
        excludes.insert(e.trim().to_lowercase());
    }

    let mut candidates = Vec::new();
    for (email, q) in analyzed_quotas {
        let clean = email.trim().to_lowercase();
        if excludes.contains(&clean) {
            continue;
        }

        if !has_account_credentials(&clean, Some(&registered)) {
            continue;
        }

        let (rem_pct, is_exh) = get_account_primary_quota(q);
        if rem_pct >= min_threshold_pct && !is_exh {
            candidates.push((email.clone(), rem_pct));
        }
    }

    candidates.into_iter().max_by(|a, b| a.1.total_cmp(&b.1))
}

pub fn get_failover_status(
    analyzed_quotas: &HashMap<String, AccountQuotaResult>,
) -> serde_json::Value {
    let active_pair = get_active_account(analyzed_quotas);
    let active_email = active_pair.as_ref().map(|(e, _)| e.clone());
    let mut active_pct = 0.0;
    let mut is_active_exhausted = false;

    if let Some((_, ref q)) = active_pair {
        let (pct, exh) = get_account_primary_quota(q);
        active_pct = pct;
        is_active_exhausted = exh;
    }

    let active_sessions = get_active_sessions(analyzed_quotas);
    let active_emails: Vec<String> = active_sessions.values().map(|(e, _)| e.clone()).collect();
    let best_candidate = find_best_failover_candidate(
        analyzed_quotas,
        active_email.as_deref(),
        &active_emails,
        5.0,
    );
    let state = load_failover_state();

    serde_json::json!({
        "active_email": active_email,
        "active_quota_pct": (active_pct * 10.0).round() / 10.0,
        "is_active_exhausted": is_active_exhausted,
        "backup_candidate_email": best_candidate.as_ref().map(|(e, _)| e.clone()),
        "backup_candidate_pct": best_candidate.as_ref().map_or(0.0, |(_, p)| (p * 10.0).round() / 10.0),
        "last_failover_timestamp": state.last_failover_timestamp.unwrap_or(0.0),
        "last_from_email": state.last_from_email,
        "last_to_email": state.last_to_email
    })
}

pub async fn evaluate_and_execute_failover(
    analyzed_quotas: &HashMap<String, AccountQuotaResult>,
    threshold: f64,
    restart: bool,
    cooldown_seconds: f64,
    dry_run: bool,
) -> FailoverDecision {
    let mut state = load_failover_state();
    let now = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;

    let active_pair = get_active_account(analyzed_quotas);
    let Some((active_email, active_q)) = active_pair else {
        return FailoverDecision {
            triggered: false,
            reason: "NO_ACTIVE_SESSION".into(),
            message: "No active Antigravity session detected to monitor for failover.".into(),
            dry_run: None,
            from_account: None,
            to_account: None,
            to_quota_pct: None,
            error: None,
        };
    };

    let (active_pct, is_exhausted) = get_account_primary_quota(&active_q);
    if active_pct > threshold && !is_exhausted {
        return FailoverDecision {
            triggered: false,
            reason: "HEALTHY".into(),
            message: format!(
                "Active account '{active_email}' quota is healthy ({active_pct:.1}%)."
            ),
            dry_run: None,
            from_account: Some(active_email),
            to_account: None,
            to_quota_pct: None,
            error: None,
        };
    }

    // Cooldown check
    let last_ts = state.last_failover_timestamp.unwrap_or(0.0);
    let time_since = now - last_ts;
    if time_since < cooldown_seconds {
        let rem = (cooldown_seconds - time_since).round() as i64;
        return FailoverDecision {
            triggered: false,
            reason: "COOLDOWN".into(),
            message: format!("Failover cooldown active ({rem}s remaining). Skipping failover."),
            dry_run: None,
            from_account: Some(active_email),
            to_account: None,
            to_quota_pct: None,
            error: None,
        };
    }

    let active_sessions = get_active_sessions(analyzed_quotas);
    let active_emails: Vec<String> = active_sessions.values().map(|(e, _)| e.clone()).collect();
    let candidate = find_best_failover_candidate(
        analyzed_quotas,
        Some(&active_email),
        &active_emails,
        threshold.max(5.0),
    );

    let Some((target_email, target_pct)) = candidate else {
        let _ = send_desktop_notification(
            "⚠️ All Antigravity Accounts Depleted",
            &format!("Active account '{active_email}' has reached {active_pct:.1}% quota and no backup accounts have quota remaining."),
            "critical",
        ).await;

        return FailoverDecision {
            triggered: false,
            reason: "ALL_EXHAUSTED".into(),
            message: format!(
                "All accounts exhausted. No candidate found to replace '{active_email}' ({active_pct:.1}%)."
            ),
            dry_run: None,
            from_account: Some(active_email),
            to_account: None,
            to_quota_pct: None,
            error: None,
        };
    };

    if dry_run {
        return FailoverDecision {
            triggered: true,
            reason: "EXHAUSTED".into(),
            message: format!(
                "[DRY RUN] Would failover from '{active_email}' ({active_pct:.1}%) to '{target_email}' ({target_pct:.1}%)."
            ),
            dry_run: Some(true),
            from_account: Some(active_email),
            to_account: Some(target_email),
            to_quota_pct: Some(target_pct),
            error: None,
        };
    }

    let _ = send_desktop_notification(
        "⚡ Emergency Quota Failover",
        &format!("Active account '{active_email}' exhausted ({active_pct:.1}%). Switching to '{target_email}' ({target_pct:.1}% available)..."),
        "critical",
    ).await;

    match switch_to_account(&target_email, restart, SwitchSurface::Both).await {
        Ok(_switch_msg) => {
            state.last_failover_timestamp = Some(now);
            state.last_from_email = Some(active_email.clone());
            state.last_to_email = Some(target_email.clone());
            state.history.push(FailoverEvent {
                timestamp: now,
                from: active_email.clone(),
                to: target_email.clone(),
                from_pct: active_pct,
                to_pct: target_pct,
                reason: format!("Primary quota reached {active_pct:.1}%"),
            });
            if state.history.len() > 50 {
                let start = state.history.len() - 50;
                state.history.drain(0..start);
            }
            let _ = save_failover_state(&state);

            let _ = send_desktop_notification(
                "✓ Antigravity Failover Complete",
                &format!(
                    "Switched to backup account '{target_email}' ({target_pct:.1}% quota available)."
                ),
                "normal",
            )
            .await;

            FailoverDecision {
                triggered: true,
                reason: "SUCCESS".into(),
                message: format!(
                    "Successfully failed over to '{target_email}' ({target_pct:.1}%)."
                ),
                dry_run: Some(false),
                from_account: Some(active_email),
                to_account: Some(target_email),
                to_quota_pct: Some(target_pct),
                error: None,
            }
        }
        Err(err) => {
            let switch_msg = err.to_string();
            let _ = send_desktop_notification(
                "❌ Failover Switch Failed",
                &format!("Failed switching to '{target_email}': {switch_msg}"),
                "critical",
            )
            .await;

            FailoverDecision {
                triggered: false,
                reason: "SWITCH_FAILED".into(),
                message: format!("Failover switch to '{target_email}' failed: {switch_msg}"),
                dry_run: Some(false),
                from_account: Some(active_email),
                to_account: Some(target_email),
                to_quota_pct: Some(target_pct),
                error: Some(switch_msg),
            }
        }
    }
}
