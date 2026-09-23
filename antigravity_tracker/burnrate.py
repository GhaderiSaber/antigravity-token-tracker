import os
import json
import time
from datetime import datetime, timezone, timedelta
from typing import Dict, Any, Optional, Tuple, List

try:
    from . import failover
except (ImportError, ValueError):
    import sys
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from antigravity_tracker import failover

HISTORY_FILE = os.path.expanduser("~/.gemini/antigravity/token_history.jsonl")


def load_snapshots(
    history_file: Optional[str] = None,
    max_age_seconds: float = 86400.0,
    limit: int = 1000
) -> List[Dict[str, Any]]:
    """Loads and filters recent timestamped snapshots from token_history.jsonl.
    Returns list of snapshot dicts sorted by timestamp ascending."""
    file_path = history_file or HISTORY_FILE
    if not os.path.isfile(file_path):
        return []

    now = time.time()
    min_ts = now - max_age_seconds
    snapshots = []

    try:
        with open(file_path, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    entry = json.loads(line)
                    ts = float(entry.get("timestamp", 0.0))
                    if ts >= min_ts:
                        snapshots.append(entry)
                except Exception:
                    continue
    except Exception as e:
        print(f"[BurnRate] Error reading history: {e}")
        return []

    # Sort ascending
    snapshots.sort(key=lambda x: float(x.get("timestamp", 0.0)))
    if len(snapshots) > limit:
        snapshots = snapshots[-limit:]

    return snapshots


def extract_account_quota_pct(snapshot_entry: Dict[str, Any], email: str, group_name: str = "Gemini Models") -> Optional[float]:
    """Extracts weekly quota percentage for a specific account from a snapshot."""
    accs = snapshot_entry.get("accounts", {})
    # Match email case-insensitively
    acc_data = None
    email_clean = email.strip().lower()
    for e, data in accs.items():
        if e.strip().lower() == email_clean:
            acc_data = data
            break

    if not acc_data:
        return None

    groups = acc_data.get("groups", {})
    # Try exact match or substring match (e.g. "gemini")
    for g_name, g_info in groups.items():
        if g_name.lower() == group_name.lower():
            return g_info.get("weekly_pct")

    for g_name, g_info in groups.items():
        if "gemini" in g_name.lower() and "weekly_pct" in g_info:
            return g_info.get("weekly_pct")

    # Fallback to first available weekly_pct
    for g_name, g_info in groups.items():
        if "weekly_pct" in g_info and g_info.get("weekly_pct") is not None:
            return g_info.get("weekly_pct")

    return None


def format_eta_countdown(seconds: float) -> str:
    """Formats ETA seconds into a concise human string like '~34m', '~1h 15m', or '~5h'."""
    if seconds <= 0:
        return "Now"
    mins = int(round(seconds / 60.0))
    if mins < 60:
        return f"~{mins}m"
    hours = mins // 60
    rem_mins = mins % 60
    if hours >= 24:
        days = hours // 24
        return f"~{days}d {hours % 24}h"
    if rem_mins == 0:
        return f"~{hours}h"
    return f"~{hours}h {rem_mins}m"


def format_depletion_clock(target_timestamp: float) -> str:
    """Formats clock string for when depletion is estimated to occur."""
    target_dt = datetime.fromtimestamp(target_timestamp)
    now_dt = datetime.now()

    if target_dt.date() == now_dt.date():
        return target_dt.strftime("%I:%M %p")
    elif target_dt.date() == (now_dt + timedelta(days=1)).date():
        return f"Tomorrow at {target_dt.strftime('%I:%M %p')}"
    else:
        return target_dt.strftime("%b %d, %I:%M %p")


def calculate_account_velocity(
    snapshots: List[Dict[str, Any]],
    email: str,
    group_name: str = "Gemini Models",
    window_seconds: float = 3600.0,
    current_quota_pct: Optional[float] = None,
    now: Optional[float] = None
) -> Dict[str, Any]:
    """Calculates consumption velocity (% / hour) and Runout Clock ETA for an account over a time window."""
    current_time = now or time.time()
    min_time = current_time - window_seconds

    # Extract time series of (timestamp, quota_pct)
    series: List[Tuple[float, float]] = []
    for s in snapshots:
        ts = float(s.get("timestamp", 0.0))
        if ts >= min_time:
            pct = extract_account_quota_pct(s, email, group_name)
            if pct is not None:
                series.append((ts, pct))

    # Append current live datapoint if provided
    if current_quota_pct is not None:
        series.append((current_time, current_quota_pct))

    curr_pct = current_quota_pct if current_quota_pct is not None else (series[-1][1] if series else 0.0)

    # If insufficient datapoints or too short time span
    if len(series) < 2:
        return {
            "email": email,
            "current_quota_pct": round(curr_pct, 2),
            "velocity_pct_per_hour": 0.0,
            "quota_consumed_in_window": 0.0,
            "window_seconds": window_seconds,
            "datapoints": len(series),
            "pace_status": "IDLE",
            "pace_icon": "💤",
            "pace_style": "dim",
            "eta_seconds": None,
            "eta_human": "Stable / Idle",
            "depletion_timestamp": None,
            "depletion_time_str": "N/A",
            "is_depleting": False
        }

    # Calculate net consumption, respecting quota refreshes/resets
    total_consumed = 0.0
    for i in range(len(series) - 1):
        prev_pct = series[i][1]
        next_pct = series[i + 1][1]
        if next_pct < prev_pct:
            total_consumed += (prev_pct - next_pct)
        elif next_pct > prev_pct + 5.0:
            # Quota refresh/reset detected; reset boundary, do not subtract
            pass

    time_span = series[-1][0] - series[0][0]
    if time_span < 60.0:  # less than 1 minute of data
        velocity = 0.0
    else:
        velocity = (total_consumed / (time_span / 3600.0))

    # Clean small floating noise
    velocity = max(0.0, round(velocity, 2))
    is_depleting = velocity >= 0.5 and curr_pct > 0.0

    # Determine Pace Category
    if velocity >= 25.0:
        pace_status = "HIGH BURN"
        pace_icon = "🔥"
        pace_style = "bold red"
    elif velocity >= 10.0:
        pace_status = "ACTIVE"
        pace_icon = "⚡"
        pace_style = "bold yellow"
    elif velocity >= 2.0:
        pace_status = "MODERATE"
        pace_icon = "🌱"
        pace_style = "cyan"
    else:
        pace_status = "IDLE"
        pace_icon = "💤"
        pace_style = "dim"

    # Estimated Time to Depletion (Runout Clock)
    eta_seconds = None
    eta_human = "Stable / Idle"
    depletion_ts = None
    depletion_str = "N/A"

    if is_depleting:
        eta_seconds = (curr_pct / velocity) * 3600.0
        eta_human = format_eta_countdown(eta_seconds)
        depletion_ts = current_time + eta_seconds
        depletion_str = format_depletion_clock(depletion_ts)
    elif curr_pct <= 1.0:
        eta_seconds = 0.0
        eta_human = "Exhausted"
        depletion_str = "Now"

    return {
        "email": email,
        "current_quota_pct": round(curr_pct, 2),
        "velocity_pct_per_hour": velocity,
        "quota_consumed_in_window": round(total_consumed, 2),
        "window_seconds": window_seconds,
        "datapoints": len(series),
        "pace_status": pace_status,
        "pace_icon": pace_icon,
        "pace_style": pace_style,
        "eta_seconds": round(eta_seconds, 1) if eta_seconds is not None else None,
        "eta_human": eta_human,
        "depletion_timestamp": round(depletion_ts, 1) if depletion_ts is not None else None,
        "depletion_time_str": depletion_str,
        "is_depleting": is_depleting
    }


def calculate_multiwindow_velocity(
    snapshots: List[Dict[str, Any]],
    email: str,
    group_name: str = "Gemini Models",
    current_quota_pct: Optional[float] = None
) -> Dict[str, Any]:
    """Calculates velocity and Runout Clock across 15m (burst), 1h (baseline), and 6h (sustained) windows."""
    now = time.time()
    v_15m = calculate_account_velocity(snapshots, email, group_name, window_seconds=900.0, current_quota_pct=current_quota_pct, now=now)
    v_1h = calculate_account_velocity(snapshots, email, group_name, window_seconds=3600.0, current_quota_pct=current_quota_pct, now=now)
    v_6h = calculate_account_velocity(snapshots, email, group_name, window_seconds=21600.0, current_quota_pct=current_quota_pct, now=now)

    # Primary metric defaults to 1h, but uses 15m if active burst is detected or 6h if 1h has few points
    primary = v_1h
    if v_1h["datapoints"] < 3 and v_6h["datapoints"] >= 3:
        primary = v_6h

    return {
        "email": email,
        "primary": primary,
        "windows": {
            "15m": v_15m,
            "1h": v_1h,
            "6h": v_6h
        }
    }


def calculate_pool_burnrates(
    analyzed_quotas: Dict[str, Dict[str, Any]],
    history_file: Optional[str] = None
) -> Dict[str, Any]:
    """Computes comprehensive burn rates and Runout Clock projections for all registered accounts."""
    snapshots = load_snapshots(history_file=history_file, max_age_seconds=86400.0)

    # Active account
    active_pair = failover.get_active_account(analyzed_quotas)
    active_email = active_pair[0] if active_pair else None

    accounts_data = {}
    active_metrics = None

    for email, q in analyzed_quotas.items():
        curr_pct, _ = failover.get_account_primary_quota(q)
        multi = calculate_multiwindow_velocity(snapshots, email, current_quota_pct=curr_pct)
        accounts_data[email] = multi
        if active_email and email.lower() == active_email.lower():
            active_metrics = multi

    return {
        "active_email": active_email,
        "active_metrics": active_metrics,
        "accounts": accounts_data,
        "snapshot_count": len(snapshots)
    }


def format_burnrate_badge(metrics: Optional[Dict[str, Any]]) -> str:
    """Formats a concise terminal/badge string: e.g. '[🔥 14.2%/hr • Empty in ~34m]'."""
    if not metrics:
        return "[dim]Pace: N/A[/dim]"

    pri = metrics.get("primary", metrics)
    vel = pri.get("velocity_pct_per_hour", 0.0)
    icon = pri.get("pace_icon", "💤")
    style = pri.get("pace_style", "dim")
    eta = pri.get("eta_human", "Stable")

    if pri.get("is_depleting"):
        return f"[{style}]{icon} {vel:.1f}%/hr • Empty in {eta}[/{style}]"
    elif pri.get("current_quota_pct", 0.0) <= 1.0:
        return "[bold red]✖ Depleted / Exhausted[/bold red]"
    else:
        return f"[dim]{icon} Stable (<1%/hr)[/dim]"


def format_burnrate_tray_summary(metrics: Optional[Dict[str, Any]]) -> str:
    """Formats a concise tray tooltip string: e.g. '(-14.2%/h, ~34m left)'."""
    if not metrics:
        return ""
    pri = metrics.get("primary", metrics)
    if pri.get("is_depleting"):
        return f" (-{pri.get('velocity_pct_per_hour', 0.0):.1f}%/h, {pri.get('eta_human', '')} left)"
    return ""
