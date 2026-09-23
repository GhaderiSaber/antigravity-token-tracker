import os
import sys
import json
import time
import subprocess
import shutil
import threading
import webbrowser
from typing import Dict, Any, Optional, List, Tuple, Callable

STATE_FILE = os.path.expanduser("~/.config/antigravity-token-tracker/notification_state.json")


def is_notify_available() -> bool:
    """Checks whether notify-send is available on the system."""
    return shutil.which("notify-send") is not None


class DBusNotificationClient:
    """Native Freedesktop DBus notification client supporting interactive action buttons."""

    def __init__(self):
        self._bus = None
        self._iface = None
        self._loop_thread = None
        self._active_actions: Dict[int, Dict[str, Callable[[], None]]] = {}
        self._lock = threading.Lock()
        self._initialized = False

    def initialize(self) -> bool:
        if self._initialized:
            return self._iface is not None

        try:
            import dbus
            import dbus.mainloop.glib

            # Configure dedicated DBus connection attached to GLib mainloop
            dbus_loop = dbus.mainloop.glib.DBusGMainLoop()
            self._bus = dbus.SessionBus(mainloop=dbus_loop, private=True)
            obj = self._bus.get_object("org.freedesktop.Notifications", "/org/freedesktop/Notifications")
            self._iface = dbus.Interface(obj, "org.freedesktop.Notifications")

            # Connect to signals
            self._bus.add_signal_receiver(
                self._on_action_invoked,
                signal_name="ActionInvoked",
                dbus_interface="org.freedesktop.Notifications"
            )
            self._bus.add_signal_receiver(
                self._on_notification_closed,
                signal_name="NotificationClosed",
                dbus_interface="org.freedesktop.Notifications"
            )

            # Ensure a GLib main loop is running in a background thread if none active
            self._ensure_glib_loop()
            self._initialized = True
            return True
        except Exception as e:
            self._initialized = True
            self._iface = None
            return False

    def _ensure_glib_loop(self):
        try:
            from gi.repository import GLib
            if self._loop_thread is None or not self._loop_thread.is_alive():
                def run_loop():
                    loop = GLib.MainLoop()
                    loop.run()

                self._loop_thread = threading.Thread(target=run_loop, daemon=True, name="dbus-notifier-glib-loop")
                self._loop_thread.start()
        except Exception:
            pass

    def _on_action_invoked(self, nid, action_key):
        nid = int(nid)
        action_key = str(action_key)
        callback = None
        with self._lock:
            actions_map = self._active_actions.pop(nid, None)
            if actions_map:
                callback = actions_map.get(action_key)

        if callback:
            def worker():
                try:
                    callback()
                except Exception as ex:
                    print(f"[Notifier] Error executing toast action '{action_key}': {ex}")

            threading.Thread(target=worker, daemon=True).start()

    def _on_notification_closed(self, nid, reason):
        nid = int(nid)
        with self._lock:
            self._active_actions.pop(nid, None)

    def notify(
        self,
        title: str,
        message: str,
        urgency: str = "normal",
        actions: Optional[List[Tuple[str, str, Callable[[], None]]]] = None,
        timeout_ms: Optional[int] = None,
        icon: Optional[str] = None
    ) -> Optional[int]:
        if not self.initialize() or self._iface is None:
            return None

        try:
            import dbus

            # Freedesktop urgency hints: 0=low, 1=normal, 2=critical
            urgency_val = 1
            if urgency == "low":
                urgency_val = 0
            elif urgency == "critical":
                urgency_val = 2

            hints = dbus.Dictionary({
                "urgency": dbus.Byte(urgency_val)
            }, signature="sv")

            actions_list = []
            callback_map = {}
            if actions:
                for key, label, cb in actions:
                    actions_list.append(str(key))
                    actions_list.append(str(label))
                    if cb:
                        callback_map[str(key)] = cb

            actions_array = dbus.Array(actions_list, signature="s")

            # Timeout: critical stays open until dismissed (0), normal defaults to 12s
            if timeout_ms is not None:
                expire_timeout = timeout_ms
            elif urgency == "critical":
                expire_timeout = 0
            else:
                expire_timeout = 12000

            nid = self._iface.Notify(
                "Antigravity Token Tracker",
                0,
                icon or "dialog-information",
                title,
                message,
                actions_array,
                hints,
                expire_timeout
            )
            nid = int(nid)
            if callback_map:
                with self._lock:
                    self._active_actions[nid] = callback_map
            return nid
        except Exception as e:
            print(f"[Notifier] DBus Notify error: {e}")
            return None


_client: Optional[DBusNotificationClient] = None


def get_dbus_client() -> DBusNotificationClient:
    global _client
    if _client is None:
        _client = DBusNotificationClient()
    return _client


def trigger_action_switch(target_email: str):
    """Callback for 1-click toast switch button."""
    try:
        from . import switcher
    except (ImportError, ValueError):
        import sys
        sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        from antigravity_tracker import switcher

    send_desktop_notification(
        "⚡ Switching Antigravity Account",
        f"Switching active session to {target_email}...\nRestarting Antigravity...",
        urgency="normal"
    )
    success, msg = switcher.switch_to_account(target_email, restart=True)
    if success:
        send_desktop_notification(
            "✓ Account Switched Successfully",
            f"Active Antigravity session is now {target_email}!",
            urgency="normal"
        )
    else:
        send_desktop_notification(
            "❌ Switch Failed",
            f"Could not switch to {target_email}: {msg}",
            urgency="critical"
        )


def trigger_action_dashboard():
    """Callback for 1-click open dashboard button."""
    try:
        webbrowser.open("http://localhost:8765")
    except Exception as e:
        print(f"[Notifier] Could not open browser: {e}")


def send_desktop_notification(
    title: str,
    message: str,
    urgency: str = "normal",
    actions: Optional[List[Tuple[str, str, Callable[[], None]]]] = None,
    timeout_ms: Optional[int] = None,
    icon: Optional[str] = None
) -> Optional[int]:
    """Sends a native desktop notification with optional 1-click action buttons.
    Uses native DBus with ActionInvoked signal listening, falling back to notify-send."""
    # 1. Try DBus client first (supports 1-click action buttons)
    try:
        client = get_dbus_client()
        nid = client.notify(
            title=title,
            message=message,
            urgency=urgency,
            actions=actions,
            timeout_ms=timeout_ms,
            icon=icon
        )
        if nid is not None:
            return nid
    except Exception as e:
        print(f"[Notifier] DBus notification failed, falling back to notify-send: {e}")

    # 2. Fallback to notify-send if DBus failed or is unavailable
    if not is_notify_available():
        return None

    try:
        cmd = ["notify-send", "-a", "Antigravity Token Tracker", "-u", urgency]
        if icon:
            cmd.extend(["-i", icon])
        if timeout_ms:
            cmd.extend(["-t", str(timeout_ms)])
        cmd.extend([title, message])
        subprocess.run(
            cmd,
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL
        )
    except Exception as e:
        print(f"[Notifier] Failed to send fallback desktop notification: {e}")
    return None


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


def record_account_switched(from_email: Optional[str], to_email: str):
    """Records an account switch in notification state to suppress stale depletion notifications."""
    state = load_notification_state()
    switched_from = state.get("switched_from", {})
    if from_email:
        switched_from[from_email.strip().lower()] = time.time()
    state["switched_from"] = switched_from
    state["last_switch_timestamp"] = time.time()
    state["current_active_email"] = to_email.strip().lower()
    save_notification_state(state)


def check_and_notify_lifecycle_events(analyzed_quotas: Dict[str, Dict[str, Any]]):
    """Detects quota exhaustion and weekly refresh transitions on ACTIVE accounts,
    and fires desktop notifications with 1-click action buttons."""
    state = load_notification_state()

    try:
        from . import failover
    except (ImportError, ValueError):
        import sys
        sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        from antigravity_tracker import failover

    active_sessions = failover.get_active_sessions(analyzed_quotas)
    active_emails = {pair[0].lower() for pair in active_sessions.values() if pair and pair[0]}

    switched_from = state.get("switched_from", {})
    now_ts = time.time()

    for email, q in analyzed_quotas.items():
        email_clean = email.strip().lower()
        is_desktop = bool(q.get("is_current_desktop_session"))
        is_ide = bool(q.get("is_current_ide_session"))
        is_active = is_desktop or is_ide or (email_clean in active_emails)

        # If this account was recently switched away from, suppress exhaustion/low alerts
        if email_clean in switched_from and (now_ts - switched_from[email_clean]) < 300:
            is_active = False

        surface_name = "Desktop App & IDE" if (is_desktop and is_ide) else ("Desktop App" if is_desktop else ("Antigravity IDE" if is_ide else "Active Session"))

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

            # Baseline initialization: do not fire depletion notifications on first observation
            if prev_status is None:
                pass

            # Event 1: Token exhaustion on an ACTIVE session
            elif is_active and curr_status == "EXHAUSTED" and prev_status != "EXHAUSTED":
                title = f"⚠️ Antigravity Quota Depleted ({surface_name}): {email}"
                msg = f"{g_name} weekly tokens are FINISHED ({curr_pct:.1f}%).\nFull refresh in {countdown}."

                candidate = failover.find_best_failover_candidate(
                    analyzed_quotas,
                    current_email=email,
                    exclude_emails=list(active_emails),
                    min_threshold_pct=5.0
                )
                actions = []
                if candidate:
                    best_email, best_pct = candidate
                    actions.append((
                        f"switch:{best_email}",
                        f"⚡ Switch to {best_email} ({best_pct:.1f}%)",
                        lambda e=best_email: trigger_action_switch(e)
                    ))
                actions.append((
                    "dashboard",
                    "🌐 Open Dashboard",
                    trigger_action_dashboard
                ))

                send_desktop_notification(title, msg, urgency="critical", actions=actions)

            # Event 2: Low quota warning on an ACTIVE session (dropped from OK to LOW <= 10%)
            elif is_active and curr_status == "LOW" and prev_status == "OK":
                title = f"⚠️ Low Quota Warning ({surface_name}): {email}"
                msg = f"{g_name} weekly tokens are down to {curr_pct:.1f}% remaining."

                candidate = failover.find_best_failover_candidate(
                    analyzed_quotas,
                    current_email=email,
                    exclude_emails=list(active_emails),
                    min_threshold_pct=5.0
                )
                actions = []
                if candidate:
                    best_email, best_pct = candidate
                    actions.append((
                        f"switch:{best_email}",
                        f"⚡ Switch to {best_email} ({best_pct:.1f}%)",
                        lambda e=best_email: trigger_action_switch(e)
                    ))
                actions.append((
                    "dashboard",
                    "🌐 Open Dashboard",
                    trigger_action_dashboard
                ))

                send_desktop_notification(title, msg, urgency="normal", actions=actions)

            # Event 3: Token refresh (can be on any account, including inactive backups)
            elif curr_pct >= 50.0 and prev_status in ["EXHAUSTED", "LOW"]:
                title = f"🎉 Antigravity Tokens Refreshed: {email}"
                msg = f"Your weekly limit for {g_name} has just refreshed ({curr_pct:.1f}% available)!"
                actions = [
                    (
                        "dashboard",
                        "🌐 Open Dashboard",
                        trigger_action_dashboard
                    )
                ]
                send_desktop_notification(title, msg, urgency="normal", actions=actions)

            # Update recorded state
            if email not in state:
                state[email] = {}
            state[email][bucket_key] = {
                "status": curr_status,
                "pct": curr_pct,
                "resetTime": w.get("resetTime")
            }

    save_notification_state(state)
