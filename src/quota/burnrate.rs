use chrono::{DateTime, Duration as ChronoDuration, Local, TimeZone};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub use super::models::{AccountQuotaResult, AccountVelocity, MultiwindowVelocity, PoolBurnrates};

pub fn default_history_file() -> PathBuf {
    dirs::home_dir().map_or_else(
        || PathBuf::from(".gemini/antigravity/token_history.jsonl"),
        |h| h.join(".gemini/antigravity/token_history.jsonl"),
    )
}

pub fn load_snapshots(
    history_file: &Path,
    max_age_seconds: f64,
    limit: usize,
    now_epoch: f64,
) -> Vec<Value> {
    if !history_file.exists() {
        return Vec::new();
    }

    let min_ts = now_epoch - max_age_seconds;
    let mut snapshots = Vec::new();

    if let Ok(file) = File::open(history_file) {
        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                let ts = val.get("timestamp").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if ts >= min_ts {
                    snapshots.push(val);
                }
            }
        }
    }

    snapshots.sort_by(|a, b| {
        let ts_a = a.get("timestamp").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let ts_b = b.get("timestamp").and_then(|v| v.as_f64()).unwrap_or(0.0);
        ts_a.total_cmp(&ts_b)
    });

    if snapshots.len() > limit {
        let start = snapshots.len() - limit;
        snapshots.drain(0..start);
    }

    snapshots
}

pub fn extract_account_quota_pct(snapshot: &Value, email: &str, group_name: &str) -> Option<f64> {
    let accs = snapshot.get("accounts")?.as_object()?;
    let email_clean = email.trim().to_lowercase();

    let mut target_data = None;
    for (k, v) in accs {
        if k.trim().to_lowercase() == email_clean {
            target_data = Some(v);
            break;
        }
    }

    let acc_val = target_data?;
    let groups = acc_val.get("groups")?.as_object()?;

    // Exact match
    for (g_name, g_info) in groups {
        if g_name.eq_ignore_ascii_case(group_name)
            && let Some(pct) = g_info.get("weekly_pct").and_then(|v| v.as_f64())
        {
            return Some(pct);
        }
    }

    // Substring match
    for (g_name, g_info) in groups {
        if g_name.to_lowercase().contains("gemini")
            && let Some(pct) = g_info.get("weekly_pct").and_then(|v| v.as_f64())
        {
            return Some(pct);
        }
    }

    // Fallback to first
    for g_info in groups.values() {
        if let Some(pct) = g_info.get("weekly_pct").and_then(|v| v.as_f64()) {
            return Some(pct);
        }
    }

    None
}

pub fn format_eta_countdown(seconds: f64) -> String {
    if seconds <= 0.0 {
        return "Now".to_string();
    }
    let mins = (seconds / 60.0).round() as i64;
    if mins < 60 {
        return format!("~{}m", mins);
    }
    let hours = mins / 60;
    let rem_mins = mins % 60;
    if hours >= 24 {
        let days = hours / 24;
        return format!("~{}d {}h", days, hours % 24);
    }
    if rem_mins == 0 {
        return format!("~{}h", hours);
    }
    format!("~{}h {}m", hours, rem_mins)
}

pub fn format_depletion_clock(target_timestamp: f64) -> String {
    let secs = target_timestamp as i64;
    let nsecs = ((target_timestamp - secs as f64) * 1_000_000_000.0) as u32;

    let target_dt: DateTime<Local> = match Local.timestamp_opt(secs, nsecs) {
        chrono::LocalResult::Single(dt) => dt,
        _ => return "N/A".to_string(),
    };
    let now_dt: DateTime<Local> = Local::now();

    if target_dt.date_naive() == now_dt.date_naive() {
        target_dt.format("%I:%M %p").to_string()
    } else if target_dt.date_naive() == (now_dt + ChronoDuration::days(1)).date_naive() {
        format!("Tomorrow at {}", target_dt.format("%I:%M %p"))
    } else {
        target_dt.format("%b %d, %I:%M %p").to_string()
    }
}

pub fn calculate_account_velocity(
    snapshots: &[Value],
    email: &str,
    group_name: &str,
    window_seconds: f64,
    current_quota_pct: Option<f64>,
    now_epoch: f64,
) -> AccountVelocity {
    let min_time = now_epoch - window_seconds;

    let mut series: Vec<(f64, f64)> = Vec::new();
    for s in snapshots {
        let ts = s.get("timestamp").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if ts >= min_time
            && let Some(pct) = extract_account_quota_pct(s, email, group_name)
        {
            series.push((ts, pct));
        }
    }

    if let Some(cur) = current_quota_pct {
        series.push((now_epoch, cur));
    }

    let curr_pct = current_quota_pct
        .or_else(|| series.last().map(|p| p.1))
        .unwrap_or(0.0);

    if series.len() < 2 {
        return AccountVelocity {
            email: email.to_string(),
            current_quota_pct: (curr_pct * 100.0).round() / 100.0,
            velocity_pct_per_hour: 0.0,
            quota_consumed_in_window: 0.0,
            window_seconds,
            datapoints: series.len(),
            pace_status: "IDLE".into(),
            pace_icon: "💤".into(),
            pace_style: "dim".into(),
            eta_seconds: None,
            eta_human: "Stable / Idle".into(),
            depletion_timestamp: None,
            depletion_time_str: "N/A".into(),
            is_depleting: false,
        };
    }

    let mut total_consumed = 0.0;
    for i in 0..(series.len() - 1) {
        let prev_pct = series[i].1;
        let next_pct = series[i + 1].1;
        if next_pct < prev_pct {
            total_consumed += prev_pct - next_pct;
        }
    }

    let time_span = match (series.first(), series.last()) {
        (Some(first), Some(last)) => last.0 - first.0,
        _ => 0.0,
    };
    let velocity = if time_span < 60.0 {
        0.0
    } else {
        (total_consumed / (time_span / 3600.0)).max(0.0)
    };
    let velocity = (velocity * 100.0).round() / 100.0;
    let is_depleting = velocity >= 0.5 && curr_pct > 0.0;

    let (pace_status, pace_icon, pace_style) = if velocity >= 25.0 {
        ("HIGH BURN", "🔥", "bold red")
    } else if velocity >= 10.0 {
        ("ACTIVE", "⚡", "bold yellow")
    } else if velocity >= 2.0 {
        ("MODERATE", "🌱", "cyan")
    } else {
        ("IDLE", "💤", "dim")
    };

    let mut eta_seconds = None;
    let mut eta_human = "Stable / Idle".to_string();
    let mut depletion_ts = None;
    let mut depletion_str = "N/A".to_string();

    if is_depleting {
        let secs = (curr_pct / velocity) * 3600.0;
        eta_seconds = Some((secs * 10.0).round() / 10.0);
        eta_human = format_eta_countdown(secs);
        let dep_t = now_epoch + secs;
        depletion_ts = Some((dep_t * 10.0).round() / 10.0);
        depletion_str = format_depletion_clock(dep_t);
    } else if curr_pct <= 1.0 {
        eta_seconds = Some(0.0);
        eta_human = "Exhausted".to_string();
        depletion_str = "Now".to_string();
    }

    AccountVelocity {
        email: email.to_string(),
        current_quota_pct: (curr_pct * 100.0).round() / 100.0,
        velocity_pct_per_hour: velocity,
        quota_consumed_in_window: (total_consumed * 100.0).round() / 100.0,
        window_seconds,
        datapoints: series.len(),
        pace_status: pace_status.into(),
        pace_icon: pace_icon.into(),
        pace_style: pace_style.into(),
        eta_seconds,
        eta_human,
        depletion_timestamp: depletion_ts,
        depletion_time_str: depletion_str,
        is_depleting,
    }
}

pub fn calculate_multiwindow_velocity(
    snapshots: &[Value],
    email: &str,
    group_name: &str,
    current_quota_pct: Option<f64>,
    now_epoch: f64,
) -> MultiwindowVelocity {
    let v_15m = calculate_account_velocity(
        snapshots,
        email,
        group_name,
        900.0,
        current_quota_pct,
        now_epoch,
    );
    let v_1h = calculate_account_velocity(
        snapshots,
        email,
        group_name,
        3600.0,
        current_quota_pct,
        now_epoch,
    );
    let v_6h = calculate_account_velocity(
        snapshots,
        email,
        group_name,
        21600.0,
        current_quota_pct,
        now_epoch,
    );

    let primary = if v_1h.datapoints < 3 && v_6h.datapoints >= 3 {
        v_6h.clone()
    } else {
        v_1h.clone()
    };

    let mut windows = HashMap::new();
    windows.insert("15m".into(), v_15m);
    windows.insert("1h".into(), v_1h);
    windows.insert("6h".into(), v_6h);

    MultiwindowVelocity {
        email: email.to_string(),
        primary,
        windows,
    }
}

pub fn calculate_pool_burnrates(
    analyzed_quotas: &HashMap<String, AccountQuotaResult>,
    active_email: Option<String>,
    history_file: Option<&Path>,
    now_epoch: f64,
) -> PoolBurnrates {
    let def_hist = default_history_file();
    let hist_path = history_file.unwrap_or(&def_hist);
    let snapshots = load_snapshots(hist_path, 86400.0, 500, now_epoch);

    let mut accounts_data = HashMap::new();
    let mut active_metrics = None;

    for (email, q) in analyzed_quotas {
        let mut curr_pct = None;
        for g in &q.groups {
            if let Some(w) = &g.weekly {
                if g.display_name.to_lowercase().contains("gemini") {
                    curr_pct = w.remaining_percent;
                    break;
                }
                if curr_pct.is_none() {
                    curr_pct = w.remaining_percent;
                }
            }
        }

        let multi =
            calculate_multiwindow_velocity(&snapshots, email, "Gemini", curr_pct, now_epoch);
        if let Some(ref act) = active_email
            && email.eq_ignore_ascii_case(act)
        {
            active_metrics = Some(multi.clone());
        }
        accounts_data.insert(email.to_string(), multi);
    }

    PoolBurnrates {
        active_email,
        active_metrics,
        accounts: accounts_data,
        snapshot_count: snapshots.len(),
    }
}

pub fn format_burnrate_badge(metrics: Option<&MultiwindowVelocity>) -> String {
    let Some(metrics) = metrics else {
        return "[dim]Pace: N/A[/dim]".to_string();
    };
    let pri = &metrics.primary;
    if pri.is_depleting {
        format!(
            "[{}]{} {:.1}%/hr • Empty in {}[/]",
            pri.pace_style, pri.pace_icon, pri.velocity_pct_per_hour, pri.eta_human
        )
    } else if pri.current_quota_pct <= 1.0 {
        "[bold red]✖ Depleted / Exhausted[/bold red]".to_string()
    } else {
        format!("[dim]{} Stable (<1%/hr)[/dim]", pri.pace_icon)
    }
}

pub fn format_burnrate_tray_summary(metrics: Option<&MultiwindowVelocity>) -> String {
    let Some(metrics) = metrics else {
        return "".to_string();
    };
    let pri = &metrics.primary;
    if pri.is_depleting {
        format!(
            " (-{:.1}%/h, {} left)",
            pri.velocity_pct_per_hour, pri.eta_human
        )
    } else {
        "".to_string()
    }
}
