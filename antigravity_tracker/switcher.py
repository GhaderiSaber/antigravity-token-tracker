import os
import shutil
import json
import sqlite3
import time
import subprocess
import signal
from typing import Optional, Tuple, Dict, Any

from . import auth
from . import accounts
from . import lifecycle
from . import quota

SESSIONS_DIR = os.path.expanduser("~/.config/antigravity-token-tracker/sessions")
DESKTOP_CONFIG_DIR = os.path.expanduser("~/.config/Antigravity")
IDE_STATE_DB = os.path.expanduser("~/.config/Antigravity IDE/User/globalStorage/state.vscdb")

DESKTOP_FILES_TO_SAVE = [
    "app_storage.json",
    "Cookies",
    "Network Persistent State",
    "SharedStorage",
    "Preferences",
    "Local State"
]

DESKTOP_DIRS_TO_SAVE = [
    "Local Storage",
    "Session Storage"
]


def ensure_sessions_dir():
    os.makedirs(SESSIONS_DIR, mode=0o700, exist_ok=True)


def get_account_session_dir(email: str, create: bool = False) -> str:
    clean_email = email.strip().lower()
    path = os.path.join(SESSIONS_DIR, clean_email)
    if create:
        ensure_sessions_dir()
        os.makedirs(path, mode=0o700, exist_ok=True)
    return path


def has_account_credentials(email: str, registered: Optional[Dict[str, Any]] = None) -> bool:
    """Checks whether an account has saved session snapshots or a valid refresh token."""
    clean_email = email.strip().lower()
    s_dir = get_account_session_dir(clean_email, create=False)
    if os.path.isdir(s_dir):
        has_desktop = os.path.isdir(os.path.join(s_dir, "desktop"))
        has_keyring = os.path.isfile(os.path.join(s_dir, "keyring_token.json"))
        has_ide = os.path.isfile(os.path.join(s_dir, "ide_tokens.json"))
        if has_desktop or has_keyring or has_ide:
            return True
    if registered is None:
        registered = accounts.load_accounts()
    return bool(registered.get(clean_email, {}).get("refresh_token"))


def get_keyring_secret() -> Tuple[Optional[str], Optional[Dict[str, Any]], Optional[str]]:
    """Reads the active OAuth secret from Linux Keyring (service='gemini', username='antigravity').
    Returns (email, parsed_dict, raw_json_str)."""
    return auth.get_desktop_keyring_token()


def set_keyring_secret(secret_json_str: str) -> bool:
    """Sets the active OAuth secret in Linux Keyring (service='gemini', username='antigravity')."""
    try:
        import dbus
        bus = dbus.SessionBus()
        service = bus.get_object('org.freedesktop.secrets', '/org/freedesktop/secrets')
        svc_iface = dbus.Interface(service, 'org.freedesktop.Secret.Service')
        session_path = svc_iface.OpenSession('plain', '')[1]

        secret_struct = (
            session_path,
            dbus.Array([], signature='y'),
            dbus.ByteArray(secret_json_str.encode('utf-8')),
            'text/plain; charset=utf-8'
        )

        unlocked, _ = svc_iface.SearchItems({'service': 'gemini', 'username': 'antigravity'})
        if unlocked:
            item = bus.get_object('org.freedesktop.secrets', unlocked[0])
            item.SetSecret(secret_struct, dbus_interface='org.freedesktop.Secret.Item')
            return True
        else:
            default_col = svc_iface.ReadAlias('default')
            col = bus.get_object('org.freedesktop.secrets', default_col)
            col_iface = dbus.Interface(col, 'org.freedesktop.Secret.Collection')
            properties = {
                'org.freedesktop.Secret.Item.Label': dbus.String("Password for 'antigravity' on 'gemini'"),
                'org.freedesktop.Secret.Item.Attributes': dbus.Dictionary({
                    'service': 'gemini',
                    'username': 'antigravity'
                }, signature='ss')
            }
            col_iface.CreateItem(properties, secret_struct, True)
            return True
    except Exception as e:
        print(f"[Switcher] Error setting keyring secret: {e}")
        return False


def clear_keyring_secret() -> bool:
    """Clears/removes the antigravity secret from Linux Keyring so the user can log into a new account."""
    try:
        import dbus
        bus = dbus.SessionBus()
        service = bus.get_object('org.freedesktop.secrets', '/org/freedesktop/secrets')
        svc_iface = dbus.Interface(service, 'org.freedesktop.Secret.Service')
        unlocked, _ = svc_iface.SearchItems({'service': 'gemini', 'username': 'antigravity'})
        if unlocked:
            item = bus.get_object('org.freedesktop.secrets', unlocked[0])
            item.Delete(dbus_interface='org.freedesktop.Secret.Item')
            return True
        return False
    except Exception as e:
        print(f"[Switcher] Error clearing keyring secret: {e}")
        return False


def snapshot_desktop_session(email: str) -> bool:
    """Snapshots the active Antigravity Desktop App session files and keyring OAuth token for the specified account."""
    if not os.path.isdir(DESKTOP_CONFIG_DIR):
        return False

    clean_email = email.strip().lower()
    target_dir = os.path.join(get_account_session_dir(clean_email, create=True), "desktop")
    os.makedirs(target_dir, mode=0o700, exist_ok=True)

    copied_any = False
    for f in DESKTOP_FILES_TO_SAVE:
        src = os.path.join(DESKTOP_CONFIG_DIR, f)
        if os.path.isfile(src):
            try:
                shutil.copy2(src, os.path.join(target_dir, f))
                copied_any = True
            except Exception as e:
                print(f"[Switcher] Warning: could not copy {f}: {e}")

    for d in DESKTOP_DIRS_TO_SAVE:
        src = os.path.join(DESKTOP_CONFIG_DIR, d)
        if os.path.isdir(src):
            try:
                dest = os.path.join(target_dir, d)
                if os.path.exists(dest):
                    shutil.rmtree(dest)
                shutil.copytree(src, dest)
                copied_any = True
            except Exception as e:
                print(f"[Switcher] Warning: could not copy directory {d}: {e}")

    # Snapshot Linux Keyring OAuth token (the true desktop app credentials)
    k_email, k_data, k_raw = get_keyring_secret()
    if k_raw:
        # Save if keyring email matches or is unlabelled
        if not k_email or k_email == clean_email:
            keyring_file = os.path.join(get_account_session_dir(clean_email), "keyring_token.json")
            try:
                with open(keyring_file, "w", encoding="utf-8") as fp:
                    fp.write(k_raw)
                copied_any = True

                # Persist access & refresh tokens to accounts.json
                if k_data and "token" in k_data:
                    tok = k_data["token"]
                    registered = accounts.load_accounts()
                    if clean_email in registered:
                        if tok.get("access_token"):
                            registered[clean_email]["access_token"] = tok["access_token"]
                        if tok.get("refresh_token"):
                            registered[clean_email]["refresh_token"] = tok["refresh_token"]
                        accounts.save_accounts(registered)
            except Exception as e:
                print(f"[Switcher] Warning: could not save keyring token: {e}")

    # Record snapshot metadata
    meta_file = os.path.join(get_account_session_dir(clean_email), "snapshot_meta.json")
    meta = {}
    if os.path.isfile(meta_file):
        try:
            with open(meta_file, "r", encoding="utf-8") as fp:
                meta = json.load(fp)
        except Exception:
            meta = {}
    meta.update({
        "email": clean_email,
        "snapshotted_at": time.time(),
        "has_desktop_session": copied_any or meta.get("has_desktop_session", False),
        "has_keyring_token": os.path.isfile(os.path.join(get_account_session_dir(clean_email), "keyring_token.json")),
        "has_ide_session": meta.get("has_ide_session", False) or os.path.isfile(os.path.join(get_account_session_dir(clean_email), "ide_tokens.json"))
    })
    with open(meta_file, "w", encoding="utf-8") as fp:
        json.dump(meta, fp, indent=2)

    return copied_any


def snapshot_ide_session(email: str) -> bool:
    """Snapshots Antigravity IDE state.vscdb auth keys for the specified account."""
    if not os.path.isfile(IDE_STATE_DB):
        return False

    clean_email = email.strip().lower()

    # Safety check: verify that state.vscdb tokens actually match this account before snapshotting,
    # preventing cross-account token pollution.
    tokens = auth.extract_tokens_from_state_db(IDE_STATE_DB)
    if tokens:
        reg = accounts.load_accounts()
        token_owner = None
        for em, acc in reg.items():
            if acc.get("refresh_token") and acc.get("refresh_token") == tokens.get("refresh_token"):
                token_owner = em.lower()
                break
        if token_owner and token_owner != clean_email:
            return False

        # Additional verification against userStatus in state.vscdb
        extract_fn = getattr(auth, "extract_user_email_from_state_db", None)
        if extract_fn:
            db_email = extract_fn(IDE_STATE_DB)
            if db_email and db_email != clean_email:
                return False

    try:
        uri = f"file:{IDE_STATE_DB}?mode=ro"
        conn = sqlite3.connect(uri, uri=True, timeout=5)
        c = conn.cursor()
        c.execute("SELECT key, value FROM ItemTable WHERE key IN (?, ?, ?)",
                  ("antigravityUnifiedStateSync.oauthToken", "antigravityUnifiedStateSync.userStatus", "antigravity.profileUrl"))
        rows = c.fetchall()
        conn.close()

        if not rows:
            return False

        saved = {}
        for k, v in rows:
            saved[k] = v

        ide_file = os.path.join(get_account_session_dir(clean_email, create=True), "ide_tokens.json")
        with open(ide_file, "w", encoding="utf-8") as fp:
            json.dump(saved, fp, indent=2)

        # 1. Persist access & refresh tokens to accounts.json
        if tokens:
            reg = accounts.load_accounts()
            if clean_email in reg:
                updated = False
                if tokens.get("access_token") and not reg[clean_email].get("access_token"):
                    reg[clean_email]["access_token"] = tokens["access_token"]
                    updated = True
                if tokens.get("refresh_token") and reg[clean_email].get("refresh_token") != tokens.get("refresh_token"):
                    reg[clean_email]["refresh_token"] = tokens["refresh_token"]
                    updated = True
                if updated:
                    accounts.save_accounts(reg)

        # 2. Synthesize keyring token if missing so desktop app can also use it
        keyring_file = os.path.join(get_account_session_dir(clean_email), "keyring_token.json")
        if not os.path.isfile(keyring_file) and tokens and tokens.get("refresh_token"):
            token_obj = {
                "token": {
                    "access_token": tokens.get("access_token", ""),
                    "token_type": "Bearer",
                    "refresh_token": tokens.get("refresh_token", ""),
                    "expiry": ""
                },
                "auth_method": "OAUTH",
                "id_token": ""
            }
            try:
                with open(keyring_file, "w", encoding="utf-8") as fp:
                    json.dump(token_obj, fp, indent=2)
            except Exception:
                pass

        # 3. Update snapshot_meta.json
        meta_file = os.path.join(get_account_session_dir(clean_email), "snapshot_meta.json")
        meta = {}
        if os.path.isfile(meta_file):
            try:
                with open(meta_file, "r", encoding="utf-8") as fp:
                    meta = json.load(fp)
            except Exception:
                meta = {}
        meta["email"] = clean_email
        meta["snapshotted_at"] = time.time()
        meta["has_ide_session"] = True
        meta["has_desktop_session"] = meta.get("has_desktop_session", False) or os.path.isdir(os.path.join(get_account_session_dir(clean_email), "desktop"))
        meta["has_keyring_token"] = os.path.isfile(keyring_file)
        with open(meta_file, "w", encoding="utf-8") as fp:
            json.dump(meta, fp, indent=2)

        return True
    except Exception as e:
        print(f"[Switcher] Error snapshotting IDE state: {e}")
        return False


def snapshot_account(email: str) -> Dict[str, bool]:
    """Snapshots all available session surfaces for an account."""
    clean_email = email.strip().lower()
    return {
        "desktop": snapshot_desktop_session(clean_email),
        "ide": snapshot_ide_session(clean_email)
    }


def restore_desktop_session(email: str) -> bool:
    """Restores a snapshotted Antigravity Desktop App session and Keyring secret."""
    clean_email = email.strip().lower()
    source_dir = os.path.join(get_account_session_dir(clean_email), "desktop")
    keyring_file = os.path.join(get_account_session_dir(clean_email), "keyring_token.json")

    has_files = os.path.isdir(source_dir)
    has_keyring = os.path.isfile(keyring_file)

    reg = accounts.load_accounts()
    has_refresh_token = bool(reg.get(clean_email, {}).get("refresh_token"))

    if not has_files and not has_keyring and not has_refresh_token:
        return False

    if has_files:
        os.makedirs(DESKTOP_CONFIG_DIR, mode=0o700, exist_ok=True)
        for f in DESKTOP_FILES_TO_SAVE:
            src = os.path.join(source_dir, f)
            if os.path.isfile(src):
                try:
                    shutil.copy2(src, os.path.join(DESKTOP_CONFIG_DIR, f))
                except Exception as e:
                    print(f"[Switcher] Warning: could not restore file {f}: {e}")

        for d in DESKTOP_DIRS_TO_SAVE:
            src = os.path.join(source_dir, d)
            if os.path.isdir(src):
                try:
                    dest = os.path.join(DESKTOP_CONFIG_DIR, d)
                    if os.path.exists(dest):
                        shutil.rmtree(dest)
                    shutil.copytree(src, dest)
                except Exception as e:
                    print(f"[Switcher] Warning: could not restore directory {d}: {e}")

        # Ensure app_storage.json has correct username
        storage_file = os.path.join(DESKTOP_CONFIG_DIR, "app_storage.json")
        if os.path.isfile(storage_file):
            try:
                with open(storage_file, "r", encoding="utf-8") as fp:
                    data = json.load(fp)
                data["jetski.onboarding.lastLoginUsername"] = clean_email
                with open(storage_file, "w", encoding="utf-8") as fp:
                    json.dump(data, fp, indent=2)
            except Exception:
                pass

    # Restore Linux Keyring secret
    if has_keyring:
        try:
            with open(keyring_file, "r", encoding="utf-8") as fp:
                k_raw = fp.read()
            set_keyring_secret(k_raw)
        except Exception as e:
            print(f"[Switcher] Warning: could not restore keyring secret: {e}")
    else:
        # Check if account has refresh_token in accounts.json (e.g. imported from IDE)
        reg = accounts.load_accounts()
        acc = reg.get(clean_email, {})
        if acc.get("refresh_token"):
            token_obj = {
                "token": {
                    "access_token": acc.get("access_token", ""),
                    "token_type": "Bearer",
                    "refresh_token": acc.get("refresh_token", ""),
                    "expiry": ""
                },
                "auth_method": "OAUTH",
                "id_token": ""
            }
            k_raw = json.dumps(token_obj)
            set_keyring_secret(k_raw)
            try:
                with open(keyring_file, "w", encoding="utf-8") as fp:
                    fp.write(k_raw)
            except Exception:
                pass
        else:
            # Clear old account's keyring token so app doesn't load previous user
            clear_keyring_secret()

    return True


def restore_ide_session(email: str) -> bool:
    """Restores saved IDE auth keys into ~/.config/Antigravity IDE/User/globalStorage/state.vscdb."""
    clean_email = email.strip().lower()
    ide_file = os.path.join(get_account_session_dir(clean_email), "ide_tokens.json")
    if not os.path.isfile(ide_file):
        return False

    os.makedirs(os.path.dirname(IDE_STATE_DB), mode=0o700, exist_ok=True)

    try:
        with open(ide_file, "r", encoding="utf-8") as fp:
            saved = json.load(fp)

        conn = sqlite3.connect(IDE_STATE_DB, timeout=5)
        c = conn.cursor()
        c.execute("CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY, value TEXT)")
        for k, v in saved.items():
            c.execute("INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)", (k, v))
        conn.commit()
        conn.close()
        return True
    except Exception as e:
        print(f"[Switcher] Error restoring IDE state: {e}")
        return False


def get_antigravity_pids() -> list[int]:
    """Finds PIDs of running Antigravity processes (including language server, excluding IDE)."""
    pids = []
    try:
        out = subprocess.check_output(["pgrep", "-f", "/snap/antigravity/"], stderr=subprocess.DEVNULL)
        for line in out.splitlines():
            pid = int(line.strip())
            if pid != os.getpid():
                try:
                    with open(f"/proc/{pid}/cmdline", "rb") as f:
                        cmd = f.read().decode("utf-8", errors="ignore")
                    if "antigravity-ide" in cmd:
                        continue
                except Exception:
                    pass
                pids.append(pid)
    except Exception:
        pass
    return pids


def is_antigravity_running() -> bool:
    return len(get_antigravity_pids()) > 0


def stop_antigravity() -> bool:
    """Terminates running Antigravity Desktop processes and language server cleanly."""
    pids = get_antigravity_pids()
    if pids:
        # Graceful SIGTERM
        for p in pids:
            try:
                os.kill(p, signal.SIGTERM)
            except OSError:
                pass

        # Wait up to 4 seconds for exit
        for _ in range(40):
            time.sleep(0.1)
            remaining = [p for p in pids if os.path.exists(f"/proc/{p}")]
            if not remaining:
                break
        else:
            # Force SIGKILL on stubborn processes
            for p in pids:
                try:
                    os.kill(p, signal.SIGKILL)
                except OSError:
                    pass
            time.sleep(0.5)

    # Clean up single-instance locks so fresh instance starts immediately without crash dialogs
    for lock_name in ["SingletonLock", "SingletonCookie", "SingletonSocket"]:
        lock_path = os.path.join(DESKTOP_CONFIG_DIR, lock_name)
        try:
            if os.path.islink(lock_path) or os.path.isfile(lock_path):
                os.unlink(lock_path)
        except Exception:
            pass

    return True


def launch_antigravity() -> bool:
    """Relaunches Antigravity Desktop App with proper desktop environment."""
    antigravity_bin = shutil.which("antigravity") or "/snap/bin/antigravity"
    if not os.path.isfile(antigravity_bin):
        return False

    env = os.environ.copy()
    uid = os.getuid()
    runtime_dir = env.get("XDG_RUNTIME_DIR", f"/run/user/{uid}")
    if "WAYLAND_DISPLAY" not in env and os.path.exists(os.path.join(runtime_dir, "wayland-0")):
        env["WAYLAND_DISPLAY"] = "wayland-0"
    if "XDG_CURRENT_DESKTOP" not in env:
        env["XDG_CURRENT_DESKTOP"] = "ubuntu:GNOME"
    if "XAUTHORITY" not in env:
        import glob
        auth_candidates = glob.glob(f"{runtime_dir}/.mutter-Xwaylandauth*") + glob.glob(f"{runtime_dir}/gdm/Xauthority")
        if auth_candidates:
            env["XAUTHORITY"] = auth_candidates[0]

    subprocess.Popen(
        [antigravity_bin],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True
    )
    return True


def restart_antigravity() -> bool:
    """Terminates running Antigravity instances and relaunches cleanly."""
    stop_antigravity()
    return launch_antigravity()


def _record_switch_state(from_email: Optional[str], target_email: str, surface: str):
    """Updates failover, balancer, and notifier state on successful manual switch."""
    now_ts = time.time()
    try:
        from . import failover
        fstate = failover.load_failover_state()
        fstate["last_failover_timestamp"] = now_ts
        if from_email:
            fstate["last_from_email"] = from_email
        fstate["last_to_email"] = target_email
        failover.save_failover_state(fstate)
    except Exception:
        pass

    try:
        from . import balancer
        bstate = balancer.load_balancer_state()
        bstate["last_rotation_timestamp"] = now_ts
        if from_email:
            bstate["last_from_email"] = from_email
        bstate["last_to_email"] = target_email
        balancer.save_balancer_state(bstate)
    except Exception:
        pass

    try:
        from . import notifier
        notifier.record_account_switched(from_email, target_email)
    except Exception:
        pass


def switch_to_account(target_email: str, restart: bool = True, surface: str = "both") -> Tuple[bool, str]:
    """Switches the active session to target_email.
    surface: 'both' (default), 'desktop' (or 'app'), or 'ide'.
    """
    target_clean = target_email.strip().lower()
    registered = accounts.load_accounts()

    if target_clean not in registered:
        return False, f"Account '{target_clean}' is not in the registered accounts list."

    surface_clean = (surface or "both").strip().lower()
    do_desktop = surface_clean in ("both", "desktop", "app", "all")
    do_ide = surface_clean in ("both", "ide", "all")

    # 1. Identify currently active sessions
    current_app = auth.discover_antigravity_desktop_app()
    current_desktop_email = current_app.get("email").strip().lower() if current_app and current_app.get("email") else None

    current_ide = auth.discover_antigravity_ide()
    current_ide_email = current_ide.get("email").strip().lower() if current_ide and current_ide.get("email") else None

    if not current_desktop_email:
        for em, acc in registered.items():
            if acc.get("is_current_desktop_session"):
                current_desktop_email = em
                break

    if not current_ide_email:
        extract_fn = getattr(auth, "extract_user_email_from_state_db", None)
        if extract_fn:
            current_ide_email = extract_fn(IDE_STATE_DB)
        if not current_ide_email:
            for em, acc in registered.items():
                if acc.get("is_current_ide_session"):
                    current_ide_email = em
                    break

    primary_from_email = current_desktop_email if do_desktop else current_ide_email

    # Check if target surface(s) are already aligned
    is_desktop_aligned = (current_desktop_email == target_clean)
    is_ide_aligned = (current_ide_email == target_clean)

    if do_desktop and not do_ide and is_desktop_aligned:
        return True, f"Account '{target_clean}' is already the currently active account in Antigravity Desktop App."
    if do_ide and not do_desktop and is_ide_aligned:
        return True, f"Account '{target_clean}' is already the currently active account in Antigravity IDE."
    if do_desktop and do_ide and is_desktop_aligned and is_ide_aligned:
        return True, f"Account '{target_clean}' is already the currently active account across App and IDE."

    if do_desktop and current_desktop_email:
        snapshot_desktop_session(current_desktop_email)
    if do_ide and current_ide_email:
        snapshot_ide_session(current_ide_email)

    # 2. Check available session snapshots for the target account
    target_session_dir = get_account_session_dir(target_clean)
    has_desktop_snap = os.path.isdir(os.path.join(target_session_dir, "desktop"))
    has_keyring_snap = os.path.isfile(os.path.join(target_session_dir, "keyring_token.json"))
    has_ide_snap = os.path.isfile(os.path.join(target_session_dir, "ide_tokens.json"))
    has_refresh_token = bool(registered.get(target_clean, {}).get("refresh_token"))

    was_running = is_antigravity_running()

    # Terminate running desktop app BEFORE restoring files so Chromium does not lock or overwrite them on exit
    if do_desktop and restart and was_running and not is_desktop_aligned:
        stop_antigravity()

    restored_desktop = False
    if do_desktop and not is_desktop_aligned and (has_desktop_snap or has_keyring_snap or has_refresh_token):
        restored_desktop = restore_desktop_session(target_clean)

    restored_ide = False
    if do_ide and not is_ide_aligned and has_ide_snap:
        restored_ide = restore_ide_session(target_clean)

    # 3. Handle relaunch if requested and previously running
    if do_desktop and restart and was_running and not is_desktop_aligned:
        launch_antigravity()
        # Poll up to 8s for the new language server to become active with target account
        for _ in range(16):
            time.sleep(0.5)
            cur = auth.discover_antigravity_desktop_app()
            if cur and cur.get("email") == target_clean:
                break

    # 4. Refresh registry status
    accounts.sync_from_antigravity()

    if not do_desktop and do_ide:
        if restored_ide:
            _record_switch_state(primary_from_email, target_clean, surface)
            return True, f"✓ Switched Antigravity IDE session to {target_clean}! (Desktop App remains untouched)"
        else:
            return False, (
                f"No saved IDE session snapshot found for {target_clean}.\n"
                f"Please sign in once in Antigravity IDE so the tracker can capture the session."
            )
    elif do_desktop and not do_ide:
        if has_keyring_snap or has_refresh_token or restored_desktop:
            _record_switch_state(primary_from_email, target_clean, surface)
            return True, f"✓ Switched Antigravity Desktop App session to {target_clean}! (IDE remains untouched)"
        else:
            return False, (
                f"No saved Desktop session snapshot found for {target_clean}.\n"
                f"Please sign in once in Antigravity Desktop App so the tracker can capture the session."
            )
    else:
        if is_desktop_aligned and restored_ide:
            _record_switch_state(primary_from_email, target_clean, surface)
            return True, f"✓ Synchronized IDE session to match desktop account {target_clean}!"
        elif has_keyring_snap or has_refresh_token or restored_ide:
            _record_switch_state(primary_from_email, target_clean, surface)
            return True, f"✓ Switched active session to {target_clean}!"
        elif restored_desktop:
            _record_switch_state(primary_from_email, target_clean, surface)
            return True, (
                f"✓ Prepared Antigravity session for {target_clean}.\n"
                f"Please click 'Sign In' in Antigravity to authenticate. Once logged in, its token will be automatically captured for future 1-click switching!"
            )
        else:
            return False, (
                f"No saved session snapshot found for {target_clean}.\n"
                f"Please sign in once in Antigravity or Antigravity IDE so the tracker can capture the session."
            )
