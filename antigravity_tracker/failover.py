import os
import json
import time
import webbrowser
from typing import Dict, Any, Optional, Tuple, List
from . import switcher
from . import notifier
from . import accounts

STATE_FILE = os.path.expanduser("~/.config/antigravity-token-tracker/failover_state.json")


def load_failover_state() -> Dict[str, Any]:
    """Loads failover state tracking cooldowns and past events."""
    if os.path.isfile(STATE_FILE):
        try:
            with open(STATE_FILE, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            pass
    return {
        "last_failover_timestamp": 0.0,
        "last_from_email": None,
        "last_to_email": None,
        "all_exhausted_alerted_at": 0.0,
        "history": []
    }


def save_failover_state(state: Dict[str, Any]):
    """Persists failover state safely."""
    os.makedirs(os.path.dirname(STATE_FILE), exist_ok=True)
    try:
        with open(STATE_FILE, "w", encoding="utf-8") as f:
            json.dump(state, f, indent=2)
    except Exception as e:
        print(f"[Failover] Failed to save state: {e}")


def get_account_primary_quota(account_data: Dict[str, Any]) -> Tuple[float, bool]:
    """Extracts primary quota remaining percent (Gemini weekly if available, or first weekly bucket).
    Returns (remaining_percent, is_exhausted)."""
    groups = account_data.get("groups", [])
    if not groups:
        return 0.0, True

    gemini_pct = None
    first_pct = None

    for g in groups:
        w = g.get("weekly")
        if not w:
            continue
        pct = float(w.get("remainingPercent", 0.0))
        if first_pct is None:
            first_pct = pct
        if "gemini" in g.get("displayName", "").lower():
            gemini_pct = pct
            break

    final_pct = gemini_pct if gemini_pct is not None else (first_pct if first_pct is not None else 0.0)
    is_exhausted = final_pct <= 1.0 or account_data.get("is_gemini_exhausted", False)
    return final_pct, is_exhausted


def get_active_account(analyzed_quotas: Dict[str, Dict[str, Any]]) -> Optional[Tuple[str, Dict[str, Any]]]:
    """Finds the account currently active in Antigravity Desktop App or IDE."""
    for email, q in analyzed_quotas.items():
        if q.get("is_current_desktop_session"):
            return email, q
    for email, q in analyzed_quotas.items():
        if q.get("is_current_ide_session"):
            return email, q
    return None


def find_best_failover_candidate(
    analyzed_quotas: Dict[str, Dict[str, Any]],
    current_email: Optional[str] = None,
    min_threshold_pct: float = 5.0
) -> Optional[Tuple[str, float]]:
    """Identifies the healthiest candidate account for failover.
    Excludes current_email and requires saved session data and available quota."""
    registered = accounts.load_accounts()
    current_clean = current_email.strip().lower() if current_email else ""

    candidates = []
    for email, q in analyzed_quotas.items():
        email_clean = email.strip().lower()
        if email_clean == current_clean:
            continue

        # Candidate must have a saved snapshot or refresh token to be switchable
        session_dir = switcher.get_account_session_dir(email_clean)
        has_snap = (
            os.path.isdir(os.path.join(session_dir, "desktop"))
            or os.path.isfile(os.path.join(session_dir, "keyring_token.json"))
            or os.path.isfile(os.path.join(session_dir, "ide_tokens.json"))
            or bool(registered.get(email_clean, {}).get("refresh_token"))
        )
        if not has_snap:
            continue

        rem_pct, is_exh = get_account_primary_quota(q)
        if rem_pct >= min_threshold_pct and not is_exh:
            candidates.append((email, rem_pct))

    if not candidates:
        return None

    # Sort by available quota descending
    candidates.sort(key=lambda x: x[1], reverse=True)
    return candidates[0]


def get_failover_status(analyzed_quotas: Dict[str, Dict[str, Any]]) -> Dict[str, Any]:
    """Provides a comprehensive status snapshot for CLI and Web dashboards."""
    active_pair = get_active_account(analyzed_quotas)
    active_email = active_pair[0] if active_pair else None
    active_pct = 0.0
    is_active_exhausted = False

    if active_pair:
        active_pct, is_active_exhausted = get_account_primary_quota(active_pair[1])

    best_candidate = find_best_failover_candidate(analyzed_quotas, current_email=active_email)
    state = load_failover_state()

    return {
        "active_email": active_email,
        "active_quota_pct": round(active_pct, 1),
        "is_active_exhausted": is_active_exhausted,
        "backup_candidate_email": best_candidate[0] if best_candidate else None,
        "backup_candidate_pct": round(best_candidate[1], 1) if best_candidate else 0.0,
        "last_failover_timestamp": state.get("last_failover_timestamp", 0.0),
        "last_from_email": state.get("last_from_email"),
        "last_to_email": state.get("last_to_email")
    }


def evaluate_and_execute_failover(
    analyzed_quotas: Dict[str, Dict[str, Any]],
    threshold: float = 1.0,
    restart: bool = True,
    cooldown_seconds: float = 120.0,
    dry_run: bool = False
) -> Dict[str, Any]:
    """Evaluates active account quota against threshold. If exhausted, automatically switches
    Antigravity to the healthiest backup account with desktop notification alerts."""
    state = load_failover_state()
    now = time.time()

    # 1. Check active account
    active_pair = get_active_account(analyzed_quotas)
    if not active_pair:
        return {
            "triggered": False,
            "reason": "NO_ACTIVE_ACCOUNT",
            "message": "No active Antigravity session detected."
        }

    active_email, active_data = active_pair
    active_pct, _ = get_account_primary_quota(active_data)

    # 2. Check if active account is below threshold
    if active_pct > threshold:
        return {
            "triggered": False,
            "reason": "HEALTHY",
            "active_email": active_email,
            "active_pct": active_pct,
            "message": f"Active account {active_email} has sufficient quota ({active_pct:.1f}% > {threshold}%)."
        }

    # 3. Check anti-flapping cooldown
    last_ts = state.get("last_failover_timestamp", 0.0)
    time_since = now - last_ts
    if time_since < cooldown_seconds:
        remaining_cd = int(cooldown_seconds - time_since)
        return {
            "triggered": False,
            "reason": "COOLDOWN",
            "active_email": active_email,
            "active_pct": active_pct,
            "cooldown_remaining": remaining_cd,
            "message": f"Cooldown active ({remaining_cd}s remaining). Skipping failover to prevent flapping."
        }

    # 4. Find healthiest candidate
    candidate = find_best_failover_candidate(analyzed_quotas, current_email=active_email, min_threshold_pct=5.0)
    if not candidate:
        # All accounts depleted
        last_alert = state.get("all_exhausted_alerted_at", 0.0)
        if now - last_alert > 1800:  # Alert at most once per 30 minutes
            actions = [
                ("dashboard", "🌐 Open Dashboard", lambda: webbrowser.open("http://localhost:8765"))
            ]
            notifier.send_desktop_notification(
                "⚠️ Antigravity Quotas Depleted",
                f"Active account {active_email} quota is finished ({active_pct:.1f}%), and no backup accounts have available quota.",
                urgency="critical",
                actions=actions
            )
            state["all_exhausted_alerted_at"] = now
            save_failover_state(state)

        return {
            "triggered": False,
            "reason": "ALL_ACCOUNTS_DEPLETED",
            "active_email": active_email,
            "active_pct": active_pct,
            "message": f"Active account {active_email} is exhausted, but no backup accounts have available quota."
        }

    target_email, target_pct = candidate

    if dry_run:
        return {
            "triggered": True,
            "dry_run": True,
            "reason": "THRESHOLD_EXCEEDED",
            "from_account": active_email,
            "to_account": target_email,
            "to_quota_pct": target_pct,
            "message": f"[DRY RUN] Would switch from {active_email} ({active_pct:.1f}%) to {target_email} ({target_pct:.1f}%)."
        }

    # 5. Execute failover with user desktop notifications
    notifier.send_desktop_notification(
        "⚡ Auto-Failover: Quota Exhausted",
        f"Quota depleted on {active_email} ({active_pct:.1f}%).\nSwitching session to {target_email} ({target_pct:.1f}% available)...",
        urgency="critical"
    )

    success, msg = switcher.switch_to_account(target_email, restart=restart)

    if success:
        # Update failover state
        state["last_failover_timestamp"] = now
        state["last_from_email"] = active_email
        state["last_to_email"] = target_email
        history = state.get("history", [])
        history.append({
            "timestamp": now,
            "from": active_email,
            "to": target_email,
            "from_pct": round(active_pct, 1),
            "to_pct": round(target_pct, 1),
            "reason": "AUTO_FAILOVER_QUOTA_EXHAUSTED"
        })
        state["history"] = history[-50:]  # keep last 50
        save_failover_state(state)

        actions = [
            ("dashboard", "🌐 Open Dashboard", lambda: webbrowser.open("http://localhost:8765"))
        ]
        notifier.send_desktop_notification(
            "✓ Auto-Failover Successful",
            f"Active Antigravity account is now {target_email} with {target_pct:.1f}% quota available!",
            urgency="normal",
            actions=actions
        )

        return {
            "triggered": True,
            "reason": "SWITCHED",
            "from_account": active_email,
            "to_account": target_email,
            "to_quota_pct": target_pct,
            "message": f"Successfully auto-failed over from {active_email} to {target_email} ({target_pct:.1f}% quota)."
        }
    else:
        actions = [
            ("retry", f"⚡ Retry {target_email}", lambda: notifier.trigger_action_switch(target_email)),
            ("dashboard", "🌐 Open Dashboard", lambda: webbrowser.open("http://localhost:8765"))
        ]
        notifier.send_desktop_notification(
            "❌ Auto-Failover Failed",
            f"Could not switch to {target_email}: {msg}",
            urgency="critical",
            actions=actions
        )
        return {
            "triggered": False,
            "reason": "SWITCH_FAILED",
            "error": msg,
            "message": f"Auto-failover switch failed: {msg}"
        }
