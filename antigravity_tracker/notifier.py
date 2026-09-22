import os
import json
import subprocess
import shutil
from typing import Dict, Any, Optional

STATE_FILE = os.path.expanduser("~/.config/antigravity-token-tracker/notification_state.json")


def is_notify_available() -> bool:
    """Checks whether notify-send is available on the system."""
    return shutil.which("notify-send") is not None


def send_desktop_notification(title: str, message: str, urgency: str = "normal"):
    """Sends a native desktop notification via notify-send."""
    if not is_notify_available():
        return

    try:
        subprocess.run(
            ["notify-send", "-a", "Antigravity Token Tracker", "-u", urgency, title, message],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL
        )
    except Exception as e:
        print(f"[Notifier] Failed to send desktop notification: {e}")


def load_notification_state() -> Dict[str, Any]:
    if os.path.isfile(STATE_FILE):
        try:
            with open(STATE_FILE, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            pass
    return {}


def save_notification_state(state: Dict[str, Any]):
    os.makedirs(os.path.dirname(STATE_FILE), exist_ok=True)
    try:
        with open(STATE_FILE, "w", encoding="utf-8") as f:
            json.dump(state, f, indent=2)
    except Exception:
        pass


def check_and_notify_lifecycle_events(analyzed_quotas: Dict[str, Dict[str, Any]]):
    """Detects quota exhaustion and weekly refresh transitions, and fires desktop notifications."""
    state = load_notification_state()

    for email, q in analyzed_quotas.items():
        acc_state = state.get(email, {})
        for g in q.get("groups", []):
            g_name = g.get("displayName", "")
            w = g.get("weekly")
            if not w:
                continue

            bucket_key = f"{g_name}-weekly"
            prev_status = acc_state.get(bucket_key, {}).get("status")
            curr_pct = w.get("remainingPercent", 0.0)
            curr_status = "EXHAUSTED" if curr_pct <= 1.0 else ("LOW" if curr_pct <= 10.0 else "OK")

            countdown = w.get("countdown", "")

            # Event 1: Token exhaustion
            if curr_status == "EXHAUSTED" and prev_status != "EXHAUSTED":
                title = f"⚠️ Antigravity Quota Depleted: {email}"
                msg = f"{g_name} weekly tokens are FINISHED ({curr_pct}%).\nFull refresh in {countdown}."
                send_desktop_notification(title, msg, urgency="critical")

            # Event 2: Token refresh (was previously exhausted or low, now >= 50%)
            elif curr_pct >= 50.0 and prev_status in ["EXHAUSTED", "LOW"]:
                title = f"🎉 Antigravity Tokens Refreshed: {email}"
                msg = f"Your weekly limit for {g_name} has just refreshed ({curr_pct}% available)!"
                send_desktop_notification(title, msg, urgency="normal")

            # Update recorded state
            if email not in state:
                state[email] = {}
            state[email][bucket_key] = {
                "status": curr_status,
                "pct": curr_pct,
                "resetTime": w.get("resetTime")
            }

    save_notification_state(state)
