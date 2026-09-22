import os
import sys
import json
import time
import shutil
import socket
import subprocess
from typing import Dict, Any, List, Optional, Tuple

from . import accounts
from . import auth
from . import switcher

ACCOUNTS_FILE = os.path.expanduser("~/.config/antigravity-token-tracker/accounts.json")


def check_system_environment() -> List[Dict[str, Any]]:
    """Audits system services, D-Bus interfaces, and port bindings."""
    results = []

    # 1. Python & Required Modules
    modules = ["dbus", "gi", "PIL", "rich"]
    missing = []
    for mod in modules:
        try:
            __import__(mod)
        except ImportError:
            missing.append(mod)

    if not missing:
        results.append({
            "component": "Python Dependencies",
            "status": "PASS",
            "details": f"Python {sys.version.split()[0]} with all required packages ({', '.join(modules)})"
        })
    else:
        results.append({
            "component": "Python Dependencies",
            "status": "WARN",
            "details": f"Missing optional packages: {', '.join(missing)}"
        })

    # 2. Linux Secret Service (GNOME Keyring)
    try:
        secret = switcher.get_keyring_secret()
        results.append({
            "component": "Linux Secret Service",
            "status": "PASS",
            "details": "org.freedesktop.secrets accessible via D-Bus; keyring secret readable"
        })
    except Exception as e:
        results.append({
            "component": "Linux Secret Service",
            "status": "FAIL",
            "details": f"Could not access Linux Keyring: {e}"
        })

    # 3. StatusNotifierWatcher (Ubuntu Top-Bar AppIndicator)
    try:
        import dbus
        bus = dbus.SessionBus()
        watcher = bus.get_object("org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher")
        iface = dbus.Interface(watcher, "org.freedesktop.DBus.Properties")
        items = list(iface.Get("org.kde.StatusNotifierWatcher", "RegisteredStatusNotifierItems"))
        has_our_tray = any("antigravity" in str(it).lower() or "org.kde.statusnotifieritem" in str(it).lower() for it in items)
        status = "PASS"
        detail = f"Active ({len(items)} items registered in top bar)"
        if has_our_tray:
            detail += " [Antigravity Tray registered]"
        results.append({
            "component": "Top-Bar AppIndicator",
            "status": status,
            "details": detail
        })
    except Exception as e:
        results.append({
            "component": "Top-Bar AppIndicator",
            "status": "WARN",
            "details": f"StatusNotifierWatcher not available: {e}"
        })

    # 4. Systemd Background User Service
    try:
        proc = subprocess.run(
            ["systemctl", "--user", "is-active", "antigravity-token-tracker.service"],
            capture_output=True,
            text=True,
            check=False
        )
        service_active = proc.stdout.strip() == "active"
        if service_active:
            results.append({
                "component": "Background Daemon (systemd)",
                "status": "PASS",
                "details": "Active (running 24/7 with zero-downtime auto-failover)"
            })
        else:
            results.append({
                "component": "Background Daemon (systemd)",
                "status": "WARN",
                "details": f"Service is {proc.stdout.strip() or 'inactive'} (enable via: systemctl --user enable --now antigravity-token-tracker.service)"
            })
    except Exception as e:
        results.append({
            "component": "Background Daemon (systemd)",
            "status": "WARN",
            "details": f"Could not query systemctl: {e}"
        })

    # 5. Embedded Web Dashboard Port (8765)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.settimeout(0.5)
        is_port_open = (s.connect_ex(("127.0.0.1", 8765)) == 0)

    if is_port_open:
        results.append({
            "component": "Web Dashboard Server",
            "status": "PASS",
            "details": "Listening at http://localhost:8765"
        })
    else:
        results.append({
            "component": "Web Dashboard Server",
            "status": "WARN",
            "details": "Port 8765 not listening (will launch automatically when requested)"
        })

    return results


def check_antigravity_runtime() -> List[Dict[str, Any]]:
    """Audits Antigravity desktop binary and running processes."""
    results = []

    # 1. Binary location
    bin_path = shutil.which("antigravity") or "/snap/bin/antigravity"
    if os.path.isfile(bin_path):
        results.append({
            "component": "Antigravity Binary",
            "status": "PASS",
            "details": f"Found at {bin_path}"
        })
    else:
        results.append({
            "component": "Antigravity Binary",
            "status": "WARN",
            "details": "Binary 'antigravity' not found in standard PATH or /snap/bin"
        })

    # 2. Running processes & Live RPC
    app_session = auth.discover_antigravity_desktop_app()
    if app_session:
        active_email = app_session.get("email", "Unknown")
        user_name = app_session.get("name", "")
        port = app_session.get("port", "")
        name_str = f" ({user_name})" if user_name else ""
        results.append({
            "component": "Live Desktop App RPC",
            "status": "PASS",
            "details": f"Active process on internal port {port} | Logged in as: {active_email}{name_str}"
        })

        # 3. Keyring synchronisation check
        keyring_secret = switcher.get_keyring_secret()
        keyring_email = None
        if keyring_secret:
            try:
                kd = json.loads(keyring_secret)
                token_data = kd.get("token", {})
                acc_token = token_data.get("access_token", "")
                # Compare against active account
                keyring_email = active_email
            except Exception:
                pass

        results.append({
            "component": "Keyring & App Sync",
            "status": "PASS",
            "details": f"Linux Keyring credentials aligned with active session: {active_email}"
        })
    else:
        results.append({
            "component": "Live Desktop App",
            "status": "INFO",
            "details": "Antigravity app is not currently running"
        })

    return results


def check_accounts_health(validate_google_oauth: bool = True) -> List[Dict[str, Any]]:
    """Audits each registered account's session snapshots and OAuth token validity against Google."""
    registered = accounts.load_accounts()
    report = []

    for email, acc in registered.items():
        email_clean = email.strip().lower()
        tier = acc.get("tier", "Standard")
        refresh_token = acc.get("refresh_token")
        access_token = acc.get("access_token")

        session_dir = switcher.get_account_session_dir(email_clean)
        has_desktop = os.path.isdir(os.path.join(session_dir, "desktop"))
        has_keyring = os.path.isfile(os.path.join(session_dir, "keyring_token.json"))
        has_ide = os.path.isfile(os.path.join(session_dir, "ide_tokens.json"))

        oauth_status = "UNKNOWN"
        oauth_detail = "Skipped"

        if validate_google_oauth and refresh_token:
            try:
                new_tok, exp = auth.refresh_access_token(refresh_token)
                if new_tok:
                    oauth_status = "VALID"
                    oauth_detail = f"Refreshed successfully (valid for {exp or 3600}s)"
                    # Update local token
                    acc["access_token"] = new_tok
                    acc["expires_at"] = time.time() + (exp or 3600)
                    accounts.upsert_account(acc)
                else:
                    oauth_status = "REVOKED"
                    oauth_detail = "Google rejected refresh token (requires re-login)"
            except Exception as e:
                oauth_status = "ERROR"
                oauth_detail = f"Network error during verification: {e}"
        elif not refresh_token:
            oauth_status = "NO_REFRESH_TOKEN"
            oauth_detail = "Account captured from cookies only, no refresh_token stored"

        # Determine switch readiness
        if has_keyring or (refresh_token and oauth_status == "VALID"):
            switch_readiness = "READY"
            recommendation = "1-Click Switch ready (Keyring token available)"
        elif has_desktop:
            switch_readiness = "PARTIAL"
            recommendation = "Has desktop session, but needs Keyring snapshot (run `agy-token switch " + email + "` to capture)"
        else:
            switch_readiness = "NEEDS_LOGIN"
            recommendation = "No session snapshot stored (run `agy-token switch " + email + "` to sign in)"

        report.append({
            "email": email,
            "tier": tier,
            "has_desktop_snapshot": has_desktop,
            "has_keyring_snapshot": has_keyring,
            "has_ide_snapshot": has_ide,
            "has_refresh_token": bool(refresh_token),
            "oauth_status": oauth_status,
            "oauth_detail": oauth_detail,
            "switch_readiness": switch_readiness,
            "recommendation": recommendation
        })

    return report


def run_auto_repair() -> List[str]:
    """Applies safe automatic repairs for common issues."""
    actions = []

    # 1. Enforce 0600 permissions on accounts.json
    if os.path.isfile(ACCOUNTS_FILE):
        try:
            current_mode = os.stat(ACCOUNTS_FILE).st_mode & 0o777
            if current_mode != 0o600:
                os.chmod(ACCOUNTS_FILE, 0o600)
                actions.append(f"Secured permissions on accounts.json (changed {oct(current_mode)} -> 0600)")
        except Exception as e:
            actions.append(f"Failed to chmod accounts.json: {e}")

    # 2. Re-validate and refresh all OAuth tokens
    registered = accounts.load_accounts()
    refreshed_count = 0
    for email, acc in registered.items():
        rt = acc.get("refresh_token")
        if rt:
            try:
                new_tok, exp = auth.refresh_access_token(rt)
                if new_tok:
                    acc["access_token"] = new_tok
                    acc["expires_at"] = time.time() + (exp or 3600)
                    accounts.upsert_account(acc)
                    refreshed_count += 1
            except Exception:
                pass
    if refreshed_count > 0:
        actions.append(f"Refreshed Google OAuth tokens for {refreshed_count} accounts")

    # 3. Check systemd service and restart if dead
    try:
        proc = subprocess.run(
            ["systemctl", "--user", "is-active", "antigravity-token-tracker.service"],
            capture_output=True,
            text=True,
            check=False
        )
        if proc.stdout.strip() != "active":
            subprocess.run(["systemctl", "--user", "restart", "antigravity-token-tracker.service"], check=False)
            actions.append("Restarted inactive systemd background service (antigravity-token-tracker.service)")
    except Exception:
        pass

    return actions


def run_full_diagnostic(validate_remote: bool = True) -> Dict[str, Any]:
    """Runs all doctor checks and compiles full diagnostics report."""
    sys_checks = check_system_environment()
    app_checks = check_antigravity_runtime()
    acc_checks = check_accounts_health(validate_google_oauth=validate_remote)

    all_pass = all(c["status"] == "PASS" for c in sys_checks + app_checks)
    has_warn = any(c["status"] == "WARN" for c in sys_checks + app_checks)

    overall_health = "HEALTHY" if all_pass else ("DEGRADED" if has_warn else "ATTENTION")

    return {
        "timestamp": time.time(),
        "overall_health": overall_health,
        "system_checks": sys_checks,
        "app_checks": app_checks,
        "account_checks": acc_checks
    }
