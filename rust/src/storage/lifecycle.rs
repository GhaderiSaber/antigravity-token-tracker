use crate::storage::models::{QuotaBucket, QuotaGroup, QuotaSnapshot};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusBadge {
    pub label: &'static str,
    pub style: &'static str,
}

pub fn get_status_badge(remaining_fraction: f64) -> StatusBadge {
    let pct = remaining_fraction * 100.0;
    if pct <= 1.0 {
        StatusBadge {
            label: "EXHAUSTED / FINISHED",
            style: "bold red",
        }
    } else if pct <= 10.0 {
        StatusBadge {
            label: "CRITICAL",
            style: "red",
        }
    } else if pct <= 25.0 {
        StatusBadge {
            label: "LOW",
            style: "yellow",
        }
    } else if pct <= 50.0 {
        StatusBadge {
            label: "MODERATE",
            style: "blue",
        }
    } else {
        StatusBadge {
            label: "HEALTHY",
            style: "bold green",
        }
    }
}

pub fn parse_iso_utc(ts_str: &str) -> Option<DateTime<Utc>> {
    let trimmed = ts_str.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Handle standard ISO timestamps like "2026-09-23T15:09:28Z"
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.with_timezone(&Utc));
    }
    // Also try without subseconds if needed
    DateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%SZ")
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

pub fn format_countdown(reset_time_str: &str, now: DateTime<Utc>) -> (String, f64) {
    let Some(dt) = parse_iso_utc(reset_time_str) else {
        return ("N/A".to_string(), 0.0);
    };

    let delta = dt.signed_duration_since(now);
    let total_secs = delta.num_seconds() as f64 + (delta.num_milliseconds() % 1000) as f64 / 1000.0;

    if total_secs <= 0.0 {
        return ("Refreshed / Ready".to_string(), 0.0);
    }

    let secs = total_secs as i64;
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 || days > 0 {
        parts.push(format!("{}h", hours));
    }
    parts.push(format!("{}m", minutes));

    (parts.join(" "), total_secs)
}

pub fn analyze_account_lifecycle(quota: &QuotaSnapshot, now: DateTime<Utc>) -> QuotaSnapshot {
    let mut analyzed_groups = Vec::new();
    let mut is_gemini_exhausted = false;
    let mut is_3p_exhausted = false;

    for group in &quota.groups {
        let group_name = &group.display_name;

        let analyzed_weekly = group.weekly.as_ref().map(|weekly| {
            let frac = weekly
                .remaining_fraction
                .unwrap_or_else(|| weekly.remaining_percent.map_or(0.0, |p| p / 100.0));
            let (cd_str, secs) = format_countdown(weekly.reset_time.as_deref().unwrap_or(""), now);
            let badge = get_status_badge(frac);
            let is_exhausted = frac <= 0.01;

            if group_name.to_lowercase().contains("gemini") {
                is_gemini_exhausted = is_exhausted;
            } else {
                is_3p_exhausted = is_exhausted;
            }

            QuotaBucket {
                bucket_id: weekly.bucket_id.clone(),
                window: weekly.window.clone(),
                display_name: weekly.display_name.clone(),
                remaining_fraction: Some(frac),
                remaining_percent: weekly.remaining_percent.or(Some(frac * 100.0)),
                reset_time: weekly.reset_time.clone(),
                description: weekly.description.clone(),
                countdown: Some(cd_str),
                seconds_remaining: Some(secs),
                status_badge: Some(badge.label.to_string()),
                status_style: Some(badge.style.to_string()),
                is_exhausted: Some(is_exhausted),
            }
        });

        let analyzed_5h = group.five_hour.as_ref().map(|five_hour| {
            let frac = five_hour
                .remaining_fraction
                .unwrap_or_else(|| five_hour.remaining_percent.map_or(0.0, |p| p / 100.0));
            let (cd_str, secs) =
                format_countdown(five_hour.reset_time.as_deref().unwrap_or(""), now);
            let badge = get_status_badge(frac);
            let is_exhausted = frac <= 0.01;

            QuotaBucket {
                bucket_id: five_hour.bucket_id.clone(),
                window: five_hour.window.clone(),
                display_name: five_hour.display_name.clone(),
                remaining_fraction: Some(frac),
                remaining_percent: five_hour.remaining_percent.or(Some(frac * 100.0)),
                reset_time: five_hour.reset_time.clone(),
                description: five_hour.description.clone(),
                countdown: Some(cd_str),
                seconds_remaining: Some(secs),
                status_badge: Some(badge.label.to_string()),
                status_style: Some(badge.style.to_string()),
                is_exhausted: Some(is_exhausted),
            }
        });

        analyzed_groups.push(QuotaGroup {
            display_name: group.display_name.clone(),
            description: group.description.clone(),
            weekly: analyzed_weekly,
            five_hour: analyzed_5h,
        });
    }

    QuotaSnapshot {
        checked_at: quota.checked_at,
        groups: analyzed_groups,
        models: quota.models.clone(),
        is_cached: quota.is_cached,
        is_gemini_exhausted: Some(is_gemini_exhausted),
        is_3p_exhausted: Some(is_3p_exhausted),
    }
}

pub fn compute_switching_recommendation(
    all_analyzed_quotas: &HashMap<String, QuotaSnapshot>,
    active_desktop_email: Option<&str>,
) -> Option<String> {
    if all_analyzed_quotas.len() < 2 {
        for (email, q) in all_analyzed_quotas {
            if q.is_gemini_exhausted.unwrap_or(false) {
                for g in &q.groups {
                    if g.display_name.to_lowercase().contains("gemini")
                        && let Some(w) = &g.weekly
                    {
                        let pct = w.remaining_percent.unwrap_or(0.0);
                        let cd = w.countdown.as_deref().unwrap_or("N/A");
                        return Some(format!(
                            "Weekly Gemini tokens on {} are EXHAUSTED ({:.1}%). Full refresh in {}.",
                            email, pct, cd
                        ));
                    }
                }
            }
        }
        return None;
    }

    let mut exhausted_accs = Vec::new();
    let mut healthy_accs: Vec<(&String, f64)> = Vec::new();

    for (email, q) in all_analyzed_quotas {
        if q.groups.is_empty() {
            continue;
        }

        if q.is_gemini_exhausted.unwrap_or(false) {
            exhausted_accs.push(email.as_str());
        } else {
            let mut rem_pct = 0.0;
            let mut has_gemini = false;
            for g in &q.groups {
                if g.display_name.to_lowercase().contains("gemini")
                    && let Some(w) = &g.weekly
                {
                    rem_pct = w.remaining_percent.unwrap_or(0.0);
                    has_gemini = true;
                }
            }
            if has_gemini {
                if rem_pct > 1.0 {
                    healthy_accs.push((email, rem_pct));
                } else {
                    exhausted_accs.push(email.as_str());
                }
            }
        }
    }

    healthy_accs.sort_by(|a, b| b.1.total_cmp(&a.1));

    // Case 1: Active desktop account is exhausted
    if let Some(active_email) = active_desktop_email
        && exhausted_accs.contains(&active_email)
    {
        if let Some((best_email, best_pct)) = healthy_accs.first() {
            return Some(format!(
                "⚠️ Active account ({}) Gemini quota is EXHAUSTED! Switch to {} which has {:.1}% quota available.",
                active_email, best_email, best_pct
            ));
        } else {
            return Some(format!(
                "⚠️ Active account ({}) Gemini quota is EXHAUSTED, and all backup accounts are also depleted!",
                active_email
            ));
        }
    }

    // Case 2: If all accounts are exhausted
    if healthy_accs.is_empty() && !exhausted_accs.is_empty() {
        return Some(
            "⚠️ All tracked accounts have exhausted their weekly Gemini quota.".to_string(),
        );
    }

    // If some are exhausted and active account is None
    if active_desktop_email.is_none() && !exhausted_accs.is_empty() && !healthy_accs.is_empty() {
        let (best_email, best_pct) = healthy_accs[0];
        return Some(format!(
            "💡 Switch Alert: {} Gemini tokens are finished. Recommended account: {} ({:.1}% quota available).",
            exhausted_accs.join(", "),
            best_email,
            best_pct
        ));
    }

    None
}

pub fn log_lifecycle_snapshot(
    history_file_path: &Path,
    analyzed_quotas: &HashMap<String, (Option<String>, QuotaSnapshot)>,
) -> std::io::Result<()> {
    if let Some(parent) = history_file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    #[derive(Serialize)]
    struct GroupSummary {
        weekly_pct: Option<f64>,
        weekly_reset: Option<String>,
        #[serde(rename = "5h_pct")]
        five_h_pct: Option<f64>,
    }

    #[derive(Serialize)]
    struct AccountSummary {
        tier: Option<String>,
        groups: HashMap<String, GroupSummary>,
    }

    #[derive(Serialize)]
    struct HistoryEntry {
        timestamp: f64,
        utc_time: String,
        accounts: HashMap<String, AccountSummary>,
    }

    let now_utc = Utc::now();
    let mut accounts_map = HashMap::new();

    for (email, (tier, q)) in analyzed_quotas {
        let mut groups_map = HashMap::new();
        for g in &q.groups {
            let weekly_pct = g.weekly.as_ref().and_then(|w| w.remaining_percent);
            let weekly_reset = g.weekly.as_ref().and_then(|w| w.reset_time.clone());
            let five_h_pct = g.five_hour.as_ref().and_then(|f| f.remaining_percent);

            groups_map.insert(
                g.display_name.clone(),
                GroupSummary {
                    weekly_pct,
                    weekly_reset,
                    five_h_pct,
                },
            );
        }

        accounts_map.insert(
            email.clone(),
            AccountSummary {
                tier: tier.clone(),
                groups: groups_map,
            },
        );
    }

    let entry = HistoryEntry {
        timestamp: now_utc.timestamp_millis() as f64 / 1000.0,
        utc_time: now_utc.to_rfc3339(),
        accounts: accounts_map,
    };

    let line = serde_json::to_string(&entry)? + "\n";
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_file_path)?;
    file.write_all(line.as_bytes())?;
    file.flush()?;

    Ok(())
}
