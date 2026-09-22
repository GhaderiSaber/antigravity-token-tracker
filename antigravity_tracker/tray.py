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
except (ImportError, ValueError):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from antigravity_tracker import accounts, quota, lifecycle, switcher, failover, notifier

ICON_CACHE_DIR = os.path.expanduser("~/.config/antigravity-token-tracker/icons")


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
    def __init__(self, update_interval: int = 60):
        self.update_interval = update_interval
        self.loop = None
        self.session_bus = None
        self.sni_object = None
        self.menu_object = None
        self.current_icon_name = "quota_100"
        self.analyzed_quotas: Dict[str, Dict[str, Any]] = {}
        self.menu_action_map: Dict[int, Any] = {}

    def get_active_summary(self) -> str:
        """Returns brief summary for tooltip."""
        active_pair = failover.get_active_account(self.analyzed_quotas)
        if not active_pair:
            return "No active Antigravity session detected"

        email, q = active_pair
        pct, is_exh = failover.get_account_primary_quota(q)
        status_str = "EXHAUSTED" if is_exh else f"{pct:.1f}% remaining"
        return f"{email}: {status_str}"

    def refresh_data(self, force: bool = False):
        """Fetches latest quotas and refreshes the tray icon."""
        try:
            accounts.sync_from_antigravity()
            all_q = quota.fetch_all_accounts_quota(force_refresh=force)
            self.analyzed_quotas = {e: lifecycle.analyze_account_lifecycle(q) for e, q in all_q.items()}

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

        elif action == "refresh":
            threading.Thread(target=lambda: self.refresh_data(force=True), daemon=True).start()

        elif action == "quit":
            print("[Tray] Quitting applet...")
            if self.loop:
                self.loop.quit()

    def start(self):
        """Initializes and runs the D-Bus StatusNotifierItem service loop."""
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


def run_tray(interval: int = 60):
    applet = TrayApplet(update_interval=interval)
    applet.start()


if __name__ == "__main__":
    run_tray()
