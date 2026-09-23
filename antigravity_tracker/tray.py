import os
import sys
import time
import threading
import subprocess
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


def open_browser(url: str = "http://localhost:8765") -> bool:
    """Robustly opens the browser in desktop environment using xdg-open/gio with fallback."""
    for cmd in [["xdg-open", url], ["gio", "open", url]]:
        try:
            subprocess.Popen(
                cmd,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True
            )
            return True
        except Exception:
            pass
    try:
        return webbrowser.open(url)
    except Exception:
        return False


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
        try:
            import web_server
            web_server.start_background_server(port=8765)
        except Exception:
            pass
        open_browser("http://localhost:8765")

    @dbus.service.method("org.kde.StatusNotifierItem", in_signature="ii", out_signature="")
    def SecondaryActivate(self, x=0, y=0):
        try:
            import web_server
            web_server.start_background_server(port=8765)
        except Exception:
            pass
        open_browser("http://localhost:8765")

    @dbus.service.method("org.kde.StatusNotifierItem", in_signature="u", out_signature="")
    def XAyatanaSecondaryActivate(self, timestamp=0):
        try:
            import web_server
            web_server.start_background_server(port=8765)
        except Exception:
            pass
        open_browser("http://localhost:8765")

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
        if str(eventId) == "clicked":
            self.tray.handle_menu_click(int(id))

    @dbus.service.method("com.canonical.dbusmenu", in_signature="a(isvu)", out_signature="ai")
    def EventGroup(self, events):
        for event in events:
            item_id, event_id, data, timestamp = event
            if str(event_id) == "clicked":
                self.tray.handle_menu_click(int(item_id))
        return dbus.Array([], signature="i")

    @dbus.service.method("com.canonical.dbusmenu", in_signature="i", out_signature="b")
    def AboutToShow(self, id):
        return False

    @dbus.service.method("com.canonical.dbusmenu", in_signature="ai", out_signature="aiai")
    def AboutToShowGroup(self, ids):
        return dbus.Array([], signature="i"), dbus.Array([], signature="i")

    @dbus.service.method("com.canonical.dbusmenu", in_signature="aias", out_signature="a(ia{sv})")
    def GetGroupProperties(self, ids, propertyNames):
        results = []
        prop_filter = set(propertyNames) if propertyNames else None
        for item_id in ids:
            int_id = int(item_id)
            if int_id in self.tray.menu_items_props:
                item_props = self.tray.menu_items_props[int_id]
                if prop_filter:
                    filtered = {k: v for k, v in item_props.items() if k in prop_filter}
                else:
                    filtered = item_props
                results.append(dbus.Struct((dbus.Int32(int_id), dbus.Dictionary(filtered, signature="sv")), signature="(ia{sv})"))
        return dbus.Array(results, signature="(ia{sv})")

    @dbus.service.method("com.canonical.dbusmenu", in_signature="is", out_signature="v")
    def GetProperty(self, id, name):
        int_id = int(id)
        if int_id in self.tray.menu_items_props and name in self.tray.menu_items_props[int_id]:
            return self.tray.menu_items_props[int_id][name]
        return dbus.String("")

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
        self.menu_items_props: Dict[int, Dict[str, Any]] = {}

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

        active_sessions = failover.get_active_sessions(self.analyzed_quotas)
        if not active_sessions:
            return f"No active Antigravity session{geo_part}"

        d_pair = active_sessions.get("desktop")
        i_pair = active_sessions.get("ide")

        # Split sessions case: separate accounts on Desktop App and IDE
        if d_pair and i_pair and d_pair[0].lower() != i_pair[0].lower():
            d_email, d_q = d_pair
            d_pct, d_exh = failover.get_account_primary_quota(d_q)
            d_status = "EXH" if d_exh else f"{d_pct:.1f}%"
            d_metrics = self.burnrate_data.get("accounts", {}).get(d_email)
            d_burn = burnrate.format_burnrate_tray_summary(d_metrics) if d_metrics else ""

            i_email, i_q = i_pair
            i_pct, i_exh = failover.get_account_primary_quota(i_q)
            i_status = "EXH" if i_exh else f"{i_pct:.1f}%"
            i_metrics = self.burnrate_data.get("accounts", {}).get(i_email)
            i_burn = burnrate.format_burnrate_tray_summary(i_metrics) if i_metrics else ""

            return f"📱 App: {d_email} ({d_status}{d_burn}) | 💻 IDE: {i_email} ({i_status}{i_burn}){geo_part}"

        # Unified or single active session
        if d_pair and i_pair:
            email, q = d_pair
            surface_tag = " (App + IDE)"
        elif d_pair:
            email, q = d_pair
            surface_tag = " (App)"
        else:
            email, q = i_pair
            surface_tag = " (IDE)"

        pct, is_exh = failover.get_account_primary_quota(q)
        status_str = "EXHAUSTED" if is_exh else f"{pct:.1f}% remaining"

        burn_part = ""
        active_metrics = self.burnrate_data.get("accounts", {}).get(email) or self.burnrate_data.get("active_metrics")
        if active_metrics:
            burn_part = burnrate.format_burnrate_tray_summary(active_metrics)

        return f"{email}{surface_tag}: {status_str}{burn_part}{geo_part}"

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

            active_sessions = failover.get_active_sessions(self.analyzed_quotas)
            if active_sessions:
                pcts = []
                is_any_exh = False
                for _, (_, q) in active_sessions.items():
                    pct, is_exh = failover.get_account_primary_quota(q)
                    pcts.append(pct)
                    if is_exh:
                        is_any_exh = True
                min_pct = min(pcts) if pcts else 0.0
                self.current_icon_name = generate_tray_icon(min_pct, is_exhausted=is_any_exh)
            else:
                self.current_icon_name = generate_tray_icon(0.0, is_exhausted=True)

            if self.sni_object:
                self.sni_object.NewIcon()
                self.sni_object.NewToolTip()
        except Exception as e:
            print(f"[Tray] Error refreshing data: {e}")

    def build_menu_layout(self) -> dbus.Struct:
        """Constructs a compact, high-efficiency DBusMenu hierarchy."""
        self.menu_action_map.clear()
        self.menu_items_props.clear()
        children = []
        item_id = 1

        def make_entry(props_dict: Dict[str, Any], sub_items: Optional[List] = None) -> Tuple[dbus.Struct, int]:
            nonlocal item_id
            current_id = item_id
            item_id += 1
            self.menu_items_props[current_id] = props_dict
            subs = sub_items or []
            return dbus.Struct((
                dbus.Int32(current_id),
                dbus.Dictionary(props_dict, signature="sv"),
                dbus.Array(subs, signature="v")
            ), signature="(ia{sv}av)"), current_id

        # 1. Primary Action: Web Dashboard (Always at top, always reachable!)
        dash_entry, d_id = make_entry({
            "label": dbus.String("🌐 Open Web Dashboard"),
            "enabled": dbus.Boolean(True)
        })
        children.append(dash_entry)
        self.menu_action_map[d_id] = ("web", None)

        # Separator
        sep1, _ = make_entry({"type": dbus.String("separator")})
        children.append(sep1)

        # 2. Active Session(s) Summary
        active_sessions = failover.get_active_sessions(self.analyzed_quotas)
        d_pair = active_sessions.get("desktop")
        i_pair = active_sessions.get("ide")
        active_emails = {pair[0].lower() for pair in active_sessions.values() if pair}

        is_split = bool(d_pair and i_pair and d_pair[0].lower() != i_pair[0].lower())

        if is_split:
            d_email, d_q = d_pair
            i_email, i_q = i_pair

            d_pct, d_exh = failover.get_account_primary_quota(d_q)
            d_status = "EXH" if d_exh else f"{d_pct:.1f}%"
            d_m = self.burnrate_data.get("accounts", {}).get(d_email)
            d_burn = burnrate.format_burnrate_tray_summary(d_m) if d_m else ""

            i_pct, i_exh = failover.get_account_primary_quota(i_q)
            i_status = "EXH" if i_exh else f"{i_pct:.1f}%"
            i_m = self.burnrate_data.get("accounts", {}).get(i_email)
            i_burn = burnrate.format_burnrate_tray_summary(i_m) if i_m else ""

            d_entry, d_id = make_entry({
                "label": dbus.String(f"📱 Desktop App: {d_email} ({d_status}{d_burn})"),
                "enabled": dbus.Boolean(True)
            })
            children.append(d_entry)
            self.menu_action_map[d_id] = ("web", None)

            i_entry, i_id = make_entry({
                "label": dbus.String(f"💻 Antigravity IDE: {i_email} ({i_status}{i_burn})"),
                "enabled": dbus.Boolean(True)
            })
            children.append(i_entry)
            self.menu_action_map[i_id] = ("web", None)

            # Quick Surface Alignment Actions
            s1_entry, s1_id = make_entry({
                "label": dbus.String(f"🔗 Sync Surfaces: Align IDE to App ({d_email})"),
                "enabled": dbus.Boolean(True)
            })
            children.append(s1_entry)
            self.menu_action_map[s1_id] = ("switch", (d_email, "ide"))

            s2_entry, s2_id = make_entry({
                "label": dbus.String(f"🔗 Sync Surfaces: Align App to IDE ({i_email})"),
                "enabled": dbus.Boolean(True)
            })
            children.append(s2_entry)
            self.menu_action_map[s2_id] = ("switch", (i_email, "desktop"))

        elif d_pair or i_pair:
            pair = d_pair or i_pair
            email, q = pair
            surf = "App + IDE" if (d_pair and i_pair) else ("Desktop App" if d_pair else "Antigravity IDE")
            pct, is_exh = failover.get_account_primary_quota(q)
            st = "EXH" if is_exh else f"{pct:.1f}%"
            m = self.burnrate_data.get("accounts", {}).get(email)
            burn = burnrate.format_burnrate_tray_summary(m) if m else ""

            a_entry, a_id = make_entry({
                "label": dbus.String(f"● Active ({surf}): {email} ({st}{burn})"),
                "enabled": dbus.Boolean(True)
            })
            children.append(a_entry)
            self.menu_action_map[a_id] = ("web", None)
        else:
            none_entry, n_id = make_entry({
                "label": dbus.String("● No active account detected"),
                "enabled": dbus.Boolean(True)
            })
            children.append(none_entry)
            self.menu_action_map[n_id] = ("web", None)

        # Separator
        sep2, _ = make_entry({"type": dbus.String("separator")})
        children.append(sep2)

        # 3. Switch Account Submenu
        registered = accounts.list_accounts()
        inactive_accounts = [acc for acc in registered if acc.get("email", "").lower() not in active_emails]
        switch_sub_items = []

        for acc in inactive_accounts:
            email = acc.get("email", "")
            q_data = self.analyzed_quotas.get(email, {})
            rem_pct, _ = failover.get_account_primary_quota(q_data)

            acc_entry, acc_id = make_entry({
                "label": dbus.String(f"⚡ Switch to {email} ({rem_pct:.1f}%)"),
                "enabled": dbus.Boolean(True)
            })
            switch_sub_items.append(acc_entry)
            self.menu_action_map[acc_id] = ("switch", (email, "both"))

        if not switch_sub_items:
            empty_entry, _ = make_entry({
                "label": dbus.String("No other accounts registered"),
                "enabled": dbus.Boolean(False)
            })
            switch_sub_items.append(empty_entry)

        switch_parent, sp_id = make_entry({
            "label": dbus.String("🔄 Switch Account"),
            "children-display": dbus.String("submenu"),
            "enabled": dbus.Boolean(True)
        }, sub_items=switch_sub_items)
        children.append(switch_parent)
        self.menu_action_map[sp_id] = ("web", None)

        # Separator
        sep3, _ = make_entry({"type": dbus.String("separator")})
        children.append(sep3)

        # 4. IP Geolocation & Killswitch Shield
        shield_cfg = shield.load_shield_config()
        is_shield_on = shield_cfg.get("enabled", True)
        shield_icon = "🛡️" if is_shield_on else "⚪"
        shield_state = "ON" if is_shield_on else "OFF"

        if self.geo_info:
            flag = self.geo_info.get("flag", "🌐")
            cc = self.geo_info.get("country_code", "US")
            ip_val = self.geo_info.get("ip", "")
            if self.geo_info.get("is_restricted"):
                ip_label = f"⚠️ RESTRICTED: {flag} {cc} (VPN Required!)"
            else:
                ip_label = f"{shield_icon} IP: {ip_val} ({flag} {cc}) [Shield: {shield_state}]"
        else:
            ip_label = f"{shield_icon} IP Killswitch Shield: [{shield_state}]"

        ip_entry, ip_id = make_entry({
            "label": dbus.String(ip_label),
            "enabled": dbus.Boolean(True)
        })
        children.append(ip_entry)
        self.menu_action_map[ip_id] = ("toggle_shield", None)

        # 5. Failover & Auto-Balancer
        failover_info = failover.get_failover_status(self.analyzed_quotas)
        backup_candidate = failover_info.get("backup_candidate_email")
        backup_pct = failover_info.get("backup_candidate_pct", 0.0)

        if backup_candidate:
            fo_label = f"⚡ Failover: Auto-Switch ({backup_candidate} {backup_pct:.1f}%)"
            fo_entry, fo_id = make_entry({
                "label": dbus.String(fo_label),
                "enabled": dbus.Boolean(True)
            })
            children.append(fo_entry)
            self.menu_action_map[fo_id] = ("switch", (backup_candidate, "both"))
        else:
            fo_entry, _ = make_entry({
                "label": dbus.String("🛡️ Failover: Standby None available"),
                "enabled": dbus.Boolean(False)
            })
            children.append(fo_entry)

        bal_cfg = balancer.load_balancer_config()
        is_bal_on = bal_cfg.get("enabled", False)
        strat_name = bal_cfg.get("strategy", "watermark").replace("_", " ").title()
        bal_icon = "🔄" if is_bal_on else "⚪"
        bal_state = "ON" if is_bal_on else "OFF"
        bal_label = f"{bal_icon} Auto-Balancer: [{bal_state} - {strat_name}]"

        bal_entry, bal_id = make_entry({
            "label": dbus.String(bal_label),
            "enabled": dbus.Boolean(True)
        })
        children.append(bal_entry)
        self.menu_action_map[bal_id] = ("toggle_balancer", None)

        # Separator
        sep4, _ = make_entry({"type": dbus.String("separator")})
        children.append(sep4)

        # 6. Controls
        ref_entry, ref_id = make_entry({
            "label": dbus.String("↻ Refresh Quotas"),
            "enabled": dbus.Boolean(True)
        })
        children.append(ref_entry)
        self.menu_action_map[ref_id] = ("refresh", None)

        quit_entry, q_id = make_entry({
            "label": dbus.String("❌ Quit Applet"),
            "enabled": dbus.Boolean(True)
        })
        children.append(quit_entry)
        self.menu_action_map[q_id] = ("quit", None)

        self.menu_items_props[0] = {"children-display": dbus.String("submenu")}
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
            if isinstance(arg, tuple):
                target_email, surface = arg
            else:
                target_email, surface = arg, "both"

            surface_name = "Both Surfaces" if surface == "both" else ("Desktop App" if surface in ("desktop", "app") else "Antigravity IDE")
            notifier.send_desktop_notification(
                f"⚡ Switching {surface_name}",
                f"Switching {surface_name} to {target_email}...\nRestarting as needed...",
                urgency="normal"
            )
            # Run in thread so D-Bus loop isn't blocked
            def do_switch():
                success, msg = switcher.switch_to_account(target_email, restart=True, surface=surface)
                if success:
                    notifier.send_desktop_notification(
                        "✓ Account Switched",
                        f"Active {surface_name} is now {target_email}!",
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
            print("[Tray] Opening Web Dashboard: http://localhost:8765")
            try:
                import web_server
                web_server.start_background_server(port=8765)
            except Exception:
                pass
            open_browser("http://localhost:8765")

        elif action == "ip_info":
            open_browser("https://ipwho.is")

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
