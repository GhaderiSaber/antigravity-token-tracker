import os
import sys
import time
import threading
import webbrowser
from typing import Dict, Any, Optional, Tuple, List

import dbus
import dbus.service
import dbus.mainloop.glib
from gi.repository import GLib
from PIL import Image, ImageDraw

try:
    from . import accounts
    from . import quota
    from . import lifecycle
    from . import switcher
    from . import failover
    from . import notifier
    from . import geo
    from . import shield
    from . import balancer
    from . import burnrate
except (ImportError, ValueError):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from antigravity_tracker import accounts, quota, lifecycle, switcher, failover, notifier, geo, shield, balancer, burnrate

ICON_CACHE_DIR = os.path.expanduser("~/.config/antigravity-token-tracker/icons")
TRAY_LOCK_FILE = os.path.expanduser("~/.config/antigravity-token-tracker/tray.lock")


def acquire_tray_lock(lock_path: Optional[str] = None) -> Tuple[Optional[int], Optional[int]]:
    """Attempts to acquire an exclusive, non-blocking lock on the tray lockfile.

    Returns:
        (fd, None) if lock acquired successfully.
        (None, existing_pid) if another process already holds the lock.
    """
    path = lock_path or TRAY_LOCK_FILE
    lock_dir = os.path.dirname(path)
    os.makedirs(lock_dir, mode=0o700, exist_ok=True)

    try:
        fd = os.open(path, os.O_CREAT | os.O_RDWR, 0o600)
    except OSError:
        return None, None

    try:
        import fcntl
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        # We got the lock! Truncate and write our PID
        os.ftruncate(fd, 0)
        os.lseek(fd, 0, os.SEEK_SET)
        os.write(fd, f"{os.getpid()}\n".encode("utf-8"))
        return fd, None
    except (BlockingIOError, OSError):
        # Locked by another process. Read its PID if available
        existing_pid = None
        try:
            with open(path, "r") as f:
                content = f.read().strip()
                if content.isdigit():
                    existing_pid = int(content)
        except Exception:
            pass
        os.close(fd)
        return None, existing_pid


def release_tray_lock(fd: Optional[int], lock_path: Optional[str] = None):
    """Releases and closes the lock file descriptor."""
    if fd is not None:
        try:
            import fcntl
            fcntl.flock(fd, fcntl.LOCK_UN)
            os.close(fd)
        except Exception:
            pass


def generate_tray_icon(pct: float, is_exhausted: bool = False) -> str:
    """Renders a dynamic 48x48 PNG icon with a quota progress ring and lightning bolt.
    Returns the icon base name (without extension)."""
    os.makedirs(ICON_CACHE_DIR, exist_ok=True)
    icon_name = f"quota_{int(round(pct))}" if not is_exhausted else "quota_exhausted"
    icon_path = os.path.join(ICON_CACHE_DIR, f"{icon_name}.png")

    # Pick color based on remaining quota
    if is_exhausted or pct <= 1.0:
        color = (234, 67, 53, 255)       # Red
    elif pct <= 10.0:
        color = (234, 67, 53, 255)       # Red
    elif pct <= 25.0:
        color = (251, 188, 4, 255)       # Yellow
    elif pct <= 50.0:
        color = (66, 133, 244, 255)      # Blue
    else:
        color = (52, 168, 83, 255)       # Green

    img = Image.new("RGBA", (48, 48), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # 1. Background subtle track ring
    draw.arc([4, 4, 44, 44], start=0, end=360, fill=(60, 64, 72, 160), width=4)

    # 2. Foreground quota progress arc
    fraction = max(0.0, min(1.0, pct / 100.0))
    if fraction > 0.01:
        arc_end = -90 + int(360 * fraction)
        draw.arc([4, 4, 44, 44], start=-90, end=arc_end, fill=color, width=4)

    # 3. Center lightning bolt
    points = [
        (26, 12),
        (18, 25),
        (24, 25),
        (22, 36),
        (30, 23),
        (24, 23)
    ]
    draw.polygon(points, fill=(255, 255, 255, 240))

    img.save(icon_path)
    return icon_name


class StatusNotifierItem(dbus.service.Object):
    def __init__(self, bus, tray_manager):
        self.tray = tray_manager
        super().__init__(bus, "/StatusNotifierItem")

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="ss", out_signature="v")
    def Get(self, interface_name, property_name):
        return self.GetAll(interface_name).get(property_name, "")

    @dbus.service.method("org.freedesktop.DBus.Properties", in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface_name):
        if interface_name == "org.kde.StatusNotifierItem":
            active_info = self.tray.get_active_summary()
            return {
                "Category": dbus.String("ApplicationStatus"),
                "Id": dbus.String("antigravity-token-tracker"),
                "Title": dbus.String("Antigravity Token Tracker"),
                "Status": dbus.String("Active"),
                "IconName": dbus.String(self.tray.current_icon_name),
                "IconThemePath": dbus.String(ICON_CACHE_DIR),
                "ItemIsMenu": dbus.Boolean(True),
                "Menu": dbus.ObjectPath("/MenuBar"),
                "ToolTip": dbus.Struct(
                    (
                        dbus.String(self.tray.current_icon_name),
                        dbus.Array([], signature="(iiay)"),
                        dbus.String("Antigravity Quota"),
                        dbus.String(active_info)
                    ),
                    signature="(sa(iiay)ss)"
                )
            }
        return {}

    @dbus.service.method("org.kde.StatusNotifierItem", in_signature="", out_signature="")
    def Activate(self, x=0, y=0):
        # On click, trigger web dashboard or cycle status
        webbrowser.open("http://localhost:8765")

    @dbus.service.method("org.kde.StatusNotifierItem", in_signature="", out_signature="")
    def ContextMenu(self, x=0, y=0):
        pass

    @dbus.service.signal("org.kde.StatusNotifierItem", signature="")
    def NewIcon(self):
        pass

    @dbus.service.signal("org.kde.StatusNotifierItem", signature="")
    def NewToolTip(self):
        pass


class DBusMenu(dbus.service.Object):
    def __init__(self, bus, tray_manager):
        self.tray = tray_manager
        self.revision = 1
        super().__init__(bus, "/MenuBar")

    @dbus.service.method("com.canonical.dbusmenu", in_signature="iias", out_signature="u(ia{sv}av)")
    def GetLayout(self, parentId, recursionDepth, propertyNames):
        layout = self.tray.build_menu_layout()
        return dbus.UInt32(self.revision), layout

    @dbus.service.method("com.canonical.dbusmenu", in_signature="isvu", out_signature="")
    def Event(self, id, eventId, data, timestamp):
        if eventId == "clicked":
            self.tray.handle_menu_click(int(id))

    @dbus.service.method("com.canonical.dbusmenu", in_signature="i", out_signature="b")
    def AboutToShow(self, id):
        self.tray.refresh_data(force=False)
        self.revision += 1
        return True

    @dbus.service.method("com.canonical.dbusmenu", in_signature="aias", out_signature="a(ia{sv})")
    def GetGroupProperties(self, ids, propertyNames):
        return dbus.Array([], signature="(ia{sv})")

    @dbus.service.signal("com.canonical.dbusmenu", signature="ui")
    def LayoutUpdated(self, revision, parent):
        pass


class TrayApplet:
    def __init__(self, update_interval: int = 60, lock_path: Optional[str] = None):
        self.update_interval = update_interval
        self.lock_path = lock_path
        self.lock_fd: Optional[int] = None
        self.loop = None
        self.session_bus = None
        self.sni_object = None
        self.menu_object = None
        self.current_icon_name = "quota_100"
        self.analyzed_quotas: Dict[str, Dict[str, Any]] = {}
        self.burnrate_data: Dict[str, Any] = {}
        self.geo_info: Dict[str, Any] = {}
        self.last_is_restricted = False
        self.menu_action_map: Dict[int, Any] = {}

    def acquire_lock(self) -> bool:
        """Attempts to acquire the singleton process lock.

        Returns True if successfully acquired, False otherwise.
        """
        if self.lock_fd is not None:
            return True
        fd, existing_pid = acquire_tray_lock(self.lock_path)
        if fd is None:
            pid_str = f" (PID {existing_pid})" if existing_pid else ""
            print(f"[Tray] Another tray indicator instance is already active{pid_str}. Exiting duplicate.")
            return False
        self.lock_fd = fd
        return True

    def release_lock(self):
        """Releases the singleton process lock if currently held."""
        if self.lock_fd is not None:
            release_tray_lock(self.lock_fd, self.lock_path)
            self.lock_fd = None

    def get_active_summary(self) -> str:
        """Returns brief summary for tooltip."""
        geo_str = geo.format_ip_summary(self.geo_info) if self.geo_info else ""
        geo_part = f" | {geo_str}" if geo_str else ""
        if self.geo_info.get("is_restricted"):
            geo_part = f" | ⚠️ RESTRICTED REGION ({self.geo_info.get('country_code', '')})"

        active_pair = failover.get_active_account(self.analyzed_quotas)
        if not active_pair:
            return f"No active Antigravity session{geo_part}"

        email, q = active_pair
        pct, is_exh = failover.get_account_primary_quota(q)
        status_str = "EXHAUSTED" if is_exh else f"{pct:.1f}% remaining"

        burn_part = ""
        active_metrics = self.burnrate_data.get("active_metrics")
        if active_metrics:
            burn_part = burnrate.format_burnrate_tray_summary(active_metrics)

        return f"{email}: {status_str}{burn_part}{geo_part}"

    def refresh_data(self, force: bool = False):
        """Fetches latest quotas and refreshes the tray icon and IP info."""
        try:
            accounts.sync_from_antigravity()
            all_q = quota.fetch_all_accounts_quota(force_refresh=force)
            self.analyzed_quotas = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_q.items()}
            try:
                self.burnrate_data = burnrate.calculate_pool_burnrates(self.analyzed_quotas)
            except Exception as be:
                print(f"[Tray] Error computing burn rates: {be}")
                self.burnrate_data = {}

            self.geo_info = geo.get_ip_geo(force_refresh=force)

            # Enforce IP Killswitch Shield
            shield.evaluate_and_enforce_shield(self.geo_info)

            active_pair = failover.get_active_account(self.analyzed_quotas)
            if active_pair:
                _, q = active_pair
                pct, is_exh = failover.get_account_primary_quota(q)
                self.current_icon_name = generate_tray_icon(pct, is_exhausted=is_exh)
            else:
                self.current_icon_name = generate_tray_icon(0.0, is_exhausted=True)

            if self.sni_object:
                self.sni_object.NewIcon()
                self.sni_object.NewToolTip()
        except Exception as e:
            print(f"[Tray] Error refreshing data: {e}")

    def build_menu_layout(self) -> dbus.Struct:
        """Constructs the native DBusMenu hierarchy."""
        self.menu_action_map.clear()
        children = []
        item_id = 1

        active_pair = failover.get_active_account(self.analyzed_quotas)
        active_email = active_pair[0] if active_pair else None

        # 1. Active Account Header
        if active_pair:
            email, q = active_pair
            pct, is_exh = failover.get_account_primary_quota(q)
            exh_str = " [EXHAUSTED]" if is_exh else ""
            header_text = f"● Active: {email} ({pct:.1f}%){exh_str}"

            children.append(dbus.Struct((
                dbus.Int32(item_id),
                dbus.Dictionary({"label": dbus.String(header_text), "enabled": dbus.Boolean(False)}, signature="sv"),
                dbus.Array([], signature="v")
            ), signature="(ia{sv}av)"))
            item_id += 1

            # Quota breakdown
            for g in q.get("groups", []):
                w = g.get("weekly")
                if w:
                    g_text = f"  {g['displayName']}: {w['remainingPercent']:.1f}% (Resets in {w['countdown']})"
                    children.append(dbus.Struct((
                        dbus.Int32(item_id),
                        dbus.Dictionary({"label": dbus.String(g_text), "enabled": dbus.Boolean(False)}, signature="sv"),
                        dbus.Array([], signature="v")
                    ), signature="(ia{sv}av)"))
                    item_id += 1

            # Runout Clock / Burn Rate
            active_metrics = self.burnrate_data.get("active_metrics")
            if active_metrics:
                pri = active_metrics.get("primary", {})
                if pri.get("is_depleting"):
                    clock_text = f"  ⏱️ Runout: Empty in {pri.get('eta_human', '')} ({pri.get('depletion_time_str', '')}) [-{pri.get('velocity_pct_per_hour', 0.0):.1f}%/h]"
                    children.append(dbus.Struct((
                        dbus.Int32(item_id),
                        dbus.Dictionary({"label": dbus.String(clock_text), "enabled": dbus.Boolean(False)}, signature="sv"),
                        dbus.Array([], signature="v")
                    ), signature="(ia{sv}av)"))
                    item_id += 1
                elif pri.get("current_quota_pct", 0.0) <= 1.0:
                    clock_text = "  ⏱️ Runout: Quota Exhausted"
                    children.append(dbus.Struct((
                        dbus.Int32(item_id),
                        dbus.Dictionary({"label": dbus.String(clock_text), "enabled": dbus.Boolean(False)}, signature="sv"),
                        dbus.Array([], signature="v")
                    ), signature="(ia{sv}av)"))
                    item_id += 1
                else:
                    clock_text = "  ⏱️ Runout: Stable / Idle (<1%/h)"
                    children.append(dbus.Struct((
                        dbus.Int32(item_id),
                        dbus.Dictionary({"label": dbus.String(clock_text), "enabled": dbus.Boolean(False)}, signature="sv"),
                        dbus.Array([], signature="v")
                    ), signature="(ia{sv}av)"))
                    item_id += 1
        else:
            children.append(dbus.Struct((
                dbus.Int32(item_id),
                dbus.Dictionary({"label": dbus.String("● No active account detected"), "enabled": dbus.Boolean(False)}, signature="sv"),
                dbus.Array([], signature="v")
            ), signature="(ia{sv}av)"))
            item_id += 1

        # Separator
        children.append(dbus.Struct((
            dbus.Int32(item_id),
            dbus.Dictionary({"type": dbus.String("separator")}, signature="sv"),
            dbus.Array([], signature="v")
        ), signature="(ia{sv}av)"))
        item_id += 1

        # IP & Geolocation Entry
        if self.geo_info:
            if self.geo_info.get("is_restricted"):
                warn_label = f"⚠️ RESTRICTED: {self.geo_info.get('flag', '')} {self.geo_info.get('country_name', 'Unknown')} (VPN Required!)"
                children.append(dbus.Struct((
                    dbus.Int32(item_id),
                    dbus.Dictionary({"label": dbus.String(warn_label), "enabled": dbus.Boolean(True)}, signature="sv"),
                    dbus.Array([], signature="v")
                ), signature="(ia{sv}av)"))
                self.menu_action_map[item_id] = ("ip_info", None)
                item_id += 1

            ip_val = self.geo_info.get("ip", "Unknown")
            flag = self.geo_info.get("flag", "🌐")
            country_name = self.geo_info.get("country_name", "Unknown")
            city = self.geo_info.get("city", "")
            loc = f"{city}, {country_name}" if city else country_name
            ip_label = f"🌐 IP: {ip_val} ({flag} {loc})"

            children.append(dbus.Struct((
                dbus.Int32(item_id),
                dbus.Dictionary({"label": dbus.String(ip_label), "enabled": dbus.Boolean(True)}, signature="sv"),
                dbus.Array([], signature="v")
            ), signature="(ia{sv}av)"))
            self.menu_action_map[item_id] = ("ip_info", None)
            item_id += 1

            # IP Shield / Killswitch toggle
            shield_cfg = shield.load_shield_config()
            is_shield_on = shield_cfg.get("enabled", True)
            shield_icon = "🛡️" if is_shield_on else "⚪"
            shield_state = "ACTIVE" if is_shield_on else "OFF"
            shield_label = f"{shield_icon} IP Killswitch Shield: [{shield_state}] (Click to toggle)"
            children.append(dbus.Struct((
                dbus.Int32(item_id),
                dbus.Dictionary({"label": dbus.String(shield_label), "enabled": dbus.Boolean(True)}, signature="sv"),
                dbus.Array([], signature="v")
            ), signature="(ia{sv}av)"))
            self.menu_action_map[item_id] = ("toggle_shield", None)
            item_id += 1

            # Separator after IP & Shield
            children.append(dbus.Struct((
                dbus.Int32(item_id),
                dbus.Dictionary({"type": dbus.String("separator")}, signature="sv"),
                dbus.Array([], signature="v")
            ), signature="(ia{sv}av)"))
            item_id += 1

        # 2. Switch Account Options
        registered = accounts.list_accounts()
        for acc in registered:
            email = acc.get("email", "")
            if email.lower() == (active_email or "").lower():
                continue

            q_data = self.analyzed_quotas.get(email, {})
            rem_pct, _ = failover.get_account_primary_quota(q_data)

            switch_label = f"🔄 Switch to {email} ({rem_pct:.1f}% available)"
            children.append(dbus.Struct((
                dbus.Int32(item_id),
                dbus.Dictionary({"label": dbus.String(switch_label), "enabled": dbus.Boolean(True)}, signature="sv"),
                dbus.Array([], signature="v")
            ), signature="(ia{sv}av)"))
            self.menu_action_map[item_id] = ("switch", email)
            item_id += 1

        # Separator
        children.append(dbus.Struct((
            dbus.Int32(item_id),
            dbus.Dictionary({"type": dbus.String("separator")}, signature="sv"),
            dbus.Array([], signature="v")
        ), signature="(ia{sv}av)"))
        item_id += 1

        # 3. Auto-Failover Section
        failover_info = failover.get_failover_status(self.analyzed_quotas)
        backup_candidate = failover_info.get("backup_candidate_email")
        backup_pct = failover_info.get("backup_candidate_pct", 0.0)

        standby_label = f"🛡️ Failover Standby: {backup_candidate} ({backup_pct:.1f}%)" if backup_candidate else "🛡️ Failover Standby: None available"
        children.append(dbus.Struct((
            dbus.Int32(item_id),
            dbus.Dictionary({"label": dbus.String(standby_label), "enabled": dbus.Boolean(False)}, signature="sv"),
            dbus.Array([], signature="v")
        ), signature="(ia{sv}av)"))
        item_id += 1

        if backup_candidate:
            children.append(dbus.Struct((
                dbus.Int32(item_id),
                dbus.Dictionary({"label": dbus.String(f"⚡ Auto-Switch to Best ({backup_candidate})"), "enabled": dbus.Boolean(True)}, signature="sv"),
                dbus.Array([], signature="v")
            ), signature="(ia{sv}av)"))
            self.menu_action_map[item_id] = ("switch", backup_candidate)
            item_id += 1

        # Separator
        children.append(dbus.Struct((
            dbus.Int32(item_id),
            dbus.Dictionary({"type": dbus.String("separator")}, signature="sv"),
            dbus.Array([], signature="v")
        ), signature="(ia{sv}av)"))
        item_id += 1

        # 4. Auto-Balancer Section
        bal_cfg = balancer.load_balancer_config()
        is_bal_on = bal_cfg.get("enabled", False)
        strat_key = bal_cfg.get("strategy", "watermark")
        strat_name = strat_key.replace("_", " ").title()
        bal_icon = "🔄" if is_bal_on else "⚪"
        bal_state = "ACTIVE" if is_bal_on else "OFF"
        bal_label = f"{bal_icon} Auto-Balancer: [{bal_state} - {strat_name}] (Click to toggle)"
        children.append(dbus.Struct((
            dbus.Int32(item_id),
            dbus.Dictionary({"label": dbus.String(bal_label), "enabled": dbus.Boolean(True)}, signature="sv"),
            dbus.Array([], signature="v")
        ), signature="(ia{sv}av)"))
        self.menu_action_map[item_id] = ("toggle_balancer", None)
        item_id += 1

        children.append(dbus.Struct((
            dbus.Int32(item_id),
            dbus.Dictionary({"label": dbus.String("🔀 Rotate Account Now"), "enabled": dbus.Boolean(True)}, signature="sv"),
            dbus.Array([], signature="v")
        ), signature="(ia{sv}av)"))
        self.menu_action_map[item_id] = ("rotate_balancer", None)
        item_id += 1

        # Separator
        children.append(dbus.Struct((
            dbus.Int32(item_id),
            dbus.Dictionary({"type": dbus.String("separator")}, signature="sv"),
            dbus.Array([], signature="v")
        ), signature="(ia{sv}av)"))
        item_id += 1

        # 4. Quick Actions
        children.append(dbus.Struct((
            dbus.Int32(item_id),
            dbus.Dictionary({"label": dbus.String("🌐 Open Web Dashboard"), "enabled": dbus.Boolean(True)}, signature="sv"),
            dbus.Array([], signature="v")
        ), signature="(ia{sv}av)"))
        self.menu_action_map[item_id] = ("web", None)
        item_id += 1

        children.append(dbus.Struct((
            dbus.Int32(item_id),
            dbus.Dictionary({"label": dbus.String("↻ Refresh Quotas"), "enabled": dbus.Boolean(True)}, signature="sv"),
            dbus.Array([], signature="v")
        ), signature="(ia{sv}av)"))
        self.menu_action_map[item_id] = ("refresh", None)
        item_id += 1

        children.append(dbus.Struct((
            dbus.Int32(item_id),
            dbus.Dictionary({"label": dbus.String("❌ Quit Applet"), "enabled": dbus.Boolean(True)}, signature="sv"),
            dbus.Array([], signature="v")
        ), signature="(ia{sv}av)"))
        self.menu_action_map[item_id] = ("quit", None)

        root = dbus.Struct((
            dbus.Int32(0),
            dbus.Dictionary({"children-display": dbus.String("submenu")}, signature="sv"),
            dbus.Array(children, signature="v")
        ), signature="(ia{sv}av)")

        return root

    def handle_menu_click(self, item_id: int):
        action_data = self.menu_action_map.get(item_id)
        if not action_data:
            return

        action, arg = action_data

        if action == "switch":
            target_email = arg
            notifier.send_desktop_notification(
                "⚡ Switching Antigravity Account",
                f"Switching active session to {target_email}...\nRestarting Antigravity...",
                urgency="normal"
            )
            # Run in thread so D-Bus loop isn't blocked
            def do_switch():
                success, msg = switcher.switch_to_account(target_email, restart=True)
                if success:
                    notifier.send_desktop_notification(
                        "✓ Account Switched",
                        f"Active account is now {target_email}!",
                        urgency="normal"
                    )
                else:
                    notifier.send_desktop_notification(
                        "❌ Switch Failed",
                        msg,
                        urgency="critical"
                    )
                GLib.idle_add(lambda: self.refresh_data(force=True))

            threading.Thread(target=do_switch, daemon=True).start()

        elif action == "web":
            try:
                import web_server
                web_server.start_background_server(port=8765)
            except Exception:
                pass
            webbrowser.open("http://localhost:8765")

        elif action == "ip_info":
            webbrowser.open("https://ipwho.is")

        elif action == "toggle_shield":
            cfg = shield.load_shield_config()
            new_state = not cfg.get("enabled", True)
            shield.set_shield_enabled(new_state)
            status_desc = "ACTIVATED (Auto-terminates Antigravity if VPN drops)" if new_state else "DISABLED"
            notifier.send_desktop_notification(
                "🛡️ IP Shield Protection",
                f"IP Killswitch Shield is now {status_desc}.",
                urgency="normal"
            )
            GLib.idle_add(lambda: self.refresh_data(force=False))

        elif action == "toggle_balancer":
            cfg = balancer.load_balancer_config()
            new_state = not cfg.get("enabled", False)
            cfg["enabled"] = new_state
            balancer.save_balancer_config(cfg)
            status_desc = f"ENABLED ({cfg.get('strategy', 'watermark').title()})" if new_state else "DISABLED"
            notifier.send_desktop_notification(
                "🔄 Auto-Balancer",
                f"Multi-account pool rotation is now {status_desc}.",
                urgency="normal"
            )
            GLib.idle_add(lambda: self.refresh_data(force=False))

        elif action == "rotate_balancer":
            notifier.send_desktop_notification(
                "🔀 Evaluating Pool Rotation",
                "Checking account pool and rotating session...",
                urgency="normal"
            )

            def do_rotate():
                res = balancer.evaluate_and_execute_balancer(self.analyzed_quotas, force=True)
                if not res.get("triggered"):
                    notifier.send_desktop_notification(
                        "🔄 Pool Already Optimal",
                        res.get("message", "No rotation needed."),
                        urgency="normal"
                    )
                GLib.idle_add(lambda: self.refresh_data(force=True))

            threading.Thread(target=do_rotate, daemon=True).start()

        elif action == "refresh":
            threading.Thread(target=lambda: self.refresh_data(force=True), daemon=True).start()

        elif action == "quit":
            print("[Tray] Quitting applet...")
            self.release_lock()
            if self.loop:
                self.loop.quit()

    def start(self) -> bool:
        """Initializes and runs the D-Bus StatusNotifierItem service loop.

        Returns True if the loop ran to completion, False if the lock was not acquired.
        """
        if self.lock_fd is None:
            if not self.acquire_lock():
                return False

        try:
            import web_server
            web_server.start_background_server(port=8765)
        except Exception:
            pass

        dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
        self.session_bus = dbus.SessionBus()

        self.refresh_data(force=False)

        bus_service_name = f"org.kde.StatusNotifierItem-{os.getpid()}-1"
        _ = dbus.service.BusName(bus_service_name, self.session_bus)

        self.sni_object = StatusNotifierItem(self.session_bus, self)
        self.menu_object = DBusMenu(self.session_bus, self)

        # Register with StatusNotifierWatcher
        try:
            watcher = self.session_bus.get_object("org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher")
            watcher.RegisterStatusNotifierItem(bus_service_name, dbus_interface="org.kde.StatusNotifierWatcher")
            print(f"[Tray] Registered '{bus_service_name}' with Ubuntu AppIndicator top bar.")
        except Exception as e:
            print(f"[Tray] Warning: Could not register with StatusNotifierWatcher: {e}")

        # Periodic background refresh
        def periodic_refresh():
            self.refresh_data(force=False)
            return True

        GLib.timeout_add_seconds(self.update_interval, periodic_refresh)

        self.loop = GLib.MainLoop()
        try:
            self.loop.run()
        except KeyboardInterrupt:
            print("\n[Tray] Stopped.")
        finally:
            self.release_lock()
        return True


def run_tray(interval: int = 60, lock_path: Optional[str] = None) -> bool:
    applet = TrayApplet(update_interval=interval, lock_path=lock_path)
    return applet.start()


if __name__ == "__main__":
    run_tray()
