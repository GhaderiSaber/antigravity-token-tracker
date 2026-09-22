import os
import json
import time
from datetime import datetime, timezone
from typing import Dict, Any, Optional, Tuple, List

HISTORY_FILE = os.path.expanduser("~/.gemini/antigravity/token_history.jsonl")


def parse_iso_utc(ts_str: str) -> Optional[datetime]:
    """Parses an ISO 8601 UTC timestamp string like '2026-09-23T15:09:28Z'."""
    if not ts_str:
        return None
    try:
        # Normalize trailing Z
        clean = ts_str.replace("Z", "+00:00")
        return datetime.fromisoformat(clean)
    except Exception:
        return None


def format_countdown(reset_time_str: str) -> Tuple[str, float]:
    """Computes the humanized countdown and seconds remaining until reset_time.
    Returns (human_str, seconds_remaining)."""
    dt = parse_iso_utc(reset_time_str)
    if not dt:
        return "N/A", 0.0

    now_utc = datetime.now(timezone.utc)
    delta = dt - now_utc
    secs = delta.total_seconds()

    if secs <= 0:
        return "Refreshed / Ready", 0.0

    days = int(secs // 86400)
    hours = int((secs % 86400) // 3600)
    minutes = int((secs % 3600) // 60)

    parts = []
    if days > 0:
        parts.append(f"{days}d")
    if hours > 0 or days > 0:
        parts.append(f"{hours}h")
    parts.append(f"{minutes}m")

    return " ".join(parts), secs


def get_status_badge(remaining_fraction: float) -> Tuple[str, str]:
    """Returns (status_label, color_style) based on remaining quota fraction."""
    pct = remaining_fraction * 100.0
    if pct <= 1.0:
        return "EXHAUSTED / FINISHED", "bold red"
    elif pct <= 10.0:
        return "CRITICAL", "red"
    elif pct <= 25.0:
        return "LOW", "yellow"
    elif pct <= 50.0:
        return "MODERATE", "blue"
    else:
        return "HEALTHY", "bold green"


def analyze_account_lifecycle(account_quota: Dict[str, Any]) -> Dict[str, Any]:
    """Enhances raw account quota data with countdowns, status badges, and flags."""
    analyzed_groups = []
    is_gemini_exhausted = False
    is_3p_exhausted = False

    for group in account_quota.get("groups", []):
        group_name = group.get("displayName", "")
        weekly = group.get("weekly")
        five_hour = group.get("fiveHour")

        analyzed_weekly = None
        if weekly:
            cd_str, secs = format_countdown(weekly.get("resetTime", ""))
            badge, style = get_status_badge(weekly.get("remainingFraction", 0.0))
            is_exhausted = weekly.get("remainingFraction", 0.0) <= 0.01
            if "gemini" in group_name.lower():
                is_gemini_exhausted = is_exhausted
            else:
                is_3p_exhausted = is_exhausted

            analyzed_weekly = {
                **weekly,
                "countdown": cd_str,
                "secondsRemaining": secs,
                "statusBadge": badge,
                "statusStyle": style,
                "isExhausted": is_exhausted
            }

        analyzed_5h = None
        if five_hour:
            cd_str, secs = format_countdown(five_hour.get("resetTime", ""))
            badge, style = get_status_badge(five_hour.get("remainingFraction", 0.0))
            analyzed_5h = {
                **five_hour,
                "countdown": cd_str,
                "secondsRemaining": secs,
                "statusBadge": badge,
                "statusStyle": style,
                "isExhausted": five_hour.get("remainingFraction", 0.0) <= 0.01
            }

        analyzed_groups.append({
            "displayName": group_name,
            "description": group.get("description", ""),
            "weekly": analyzed_weekly,
            "fiveHour": analyzed_5h
        })

    return {
        **account_quota,
        "groups": analyzed_groups,
        "is_gemini_exhausted": is_gemini_exhausted,
        "is_3p_exhausted": is_3p_exhausted
    }


def compute_switching_recommendation(all_analyzed_quotas: Dict[str, Dict[str, Any]]) -> Optional[str]:
    """If active account has exhausted Gemini tokens, checks if another account has available quota."""
    if len(all_analyzed_quotas) < 2:
        for email, q in all_analyzed_quotas.items():
            if q.get("is_gemini_exhausted"):
                for g in q.get("groups", []):
                    if "gemini" in g.get("displayName", "").lower() and g.get("weekly"):
                        return f"Weekly Gemini tokens on {email} are EXHAUSTED ({g['weekly']['remainingPercent']}%). Full refresh in {g['weekly']['countdown']}."
        return None

    exhausted_accs = []
    healthy_accs = []
    active_desktop_email = None

    for email, q in all_analyzed_quotas.items():
        if q.get("is_current_desktop_session"):
            active_desktop_email = email

        if not q.get("groups"):
            continue

        if q.get("is_gemini_exhausted"):
            exhausted_accs.append(email)
        else:
            rem_pct = 0.0
            has_gemini = False
            for g in q.get("groups", []):
                if "gemini" in g.get("displayName", "").lower() and g.get("weekly"):
                    rem_pct = g["weekly"].get("remainingPercent", 0.0)
                    has_gemini = True
            if has_gemini:
                if rem_pct > 1.0:
                    healthy_accs.append((email, rem_pct))
                else:
                    exhausted_accs.append(email)

    healthy_accs.sort(key=lambda x: x[1], reverse=True)

    # Case 1: Active desktop account is exhausted
    if active_desktop_email and active_desktop_email in exhausted_accs:
        if healthy_accs:
            best_email, best_pct = healthy_accs[0]
            return f"⚠️ Active account ({active_desktop_email}) Gemini quota is EXHAUSTED! Switch to {best_email} which has {best_pct:.1f}% quota available."
        else:
            return f"⚠️ Active account ({active_desktop_email}) Gemini quota is EXHAUSTED, and all backup accounts are also depleted!"

    # Case 2: No active desktop account detected or active account not exhausted
    # If all accounts are exhausted
    if not healthy_accs and exhausted_accs:
        return "⚠️ All tracked accounts have exhausted their weekly Gemini quota."

    # If some are exhausted and active account is None
    if not active_desktop_email and exhausted_accs and healthy_accs:
        best_email, best_pct = healthy_accs[0]
        return f"💡 Switch Alert: {', '.join(exhausted_accs)} Gemini tokens are finished. Recommended account: {best_email} ({best_pct:.1f}% quota available)."

    return None


def log_lifecycle_snapshot(analyzed_quotas: Dict[str, Dict[str, Any]]):
    """Logs snapshot entry to ~/.gemini/antigravity/token_history.jsonl for burn-rate and refresh tracking."""
    os.makedirs(os.path.dirname(HISTORY_FILE), exist_ok=True)
    entry = {
        "timestamp": time.time(),
        "utc_time": datetime.now(timezone.utc).isoformat(),
        "accounts": {}
    }

    for email, q in analyzed_quotas.items():
        acc_summary = {
            "tier": q.get("tier"),
            "groups": {}
        }
        for g in q.get("groups", []):
            g_name = g.get("displayName")
            w = g.get("weekly")
            f = g.get("fiveHour")
            acc_summary["groups"][g_name] = {
                "weekly_pct": w.get("remainingPercent") if w else None,
                "weekly_reset": w.get("resetTime") if w else None,
                "5h_pct": f.get("remainingPercent") if f else None
            }
        entry["accounts"][email] = acc_summary

    try:
        with open(HISTORY_FILE, "a", encoding="utf-8") as f:
            f.write(json.dumps(entry) + "\n")
    except Exception as e:
        print(f"[Lifecycle] Failed to write history: {e}")
