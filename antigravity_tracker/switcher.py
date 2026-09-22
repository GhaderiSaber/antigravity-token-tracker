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


def get_account_session_dir(email: str) -> str:
    ensure_sessions_dir()
    clean_email = email.strip().lower()
    path = os.path.join(SESSIONS_DIR, clean_email)
    os.makedirs(path, mode=0o700, exist_ok=True)
    return path


def snapshot_desktop_session(email: str) -> bool:
    """Snapshots the active Antigravity Desktop App session files for the specified account."""
    if not os.path.isdir(DESKTOP_CONFIG_DIR):
        return False

    target_dir = os.path.join(get_account_session_dir(email), "desktop")
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

    # Record snapshot metadata
    meta_file = os.path.join(get_account_session_dir(email), "snapshot_meta.json")
    with open(meta_file, "w", encoding="utf-8") as fp:
        json.dump({
            "email": email,
            "snapshotted_at": time.time(),
            "has_desktop_session": copied_any
        }, fp, indent=2)

    return copied_any


def snapshot_ide_session(email: str) -> bool:
    """Snapshots Antigravity IDE state.vscdb auth keys for the specified account."""
    if not os.path.isfile(IDE_STATE_DB):
        return False

    try:
        uri = f"file:{IDE_STATE_DB}?mode=ro"
        conn = sqlite3.connect(uri, uri=True, timeout=5)
        c = conn.cursor()
        c.execute("SELECT key, value FROM ItemTable WHERE key IN (?, ?)",
                  ("antigravityUnifiedStateSync.oauthToken", "antigravityUnifiedStateSync.userStatus"))
        rows = c.fetchall()
        conn.close()

        if not rows:
            return False

        saved = {}
        for k, v in rows:
            saved[k] = v

        ide_file = os.path.join(get_account_session_dir(email), "ide_tokens.json")
        with open(ide_file, "w", encoding="utf-8") as fp:
            json.dump(saved, fp, indent=2)
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
    """Restores a snapshotted Antigravity Desktop App session into ~/.config/Antigravity."""
    source_dir = os.path.join(get_account_session_dir(email), "desktop")
    if not os.path.isdir(source_dir):
        return False

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
            data["jetski.onboarding.lastLoginUsername"] = email
            with open(storage_file, "w", encoding="utf-8") as fp:
                json.dump(data, fp, indent=2)
        except Exception:
            pass

    return True


def restore_ide_session(email: str) -> bool:
    """Restores saved IDE auth keys into ~/.config/Antigravity IDE/User/globalStorage/state.vscdb."""
    ide_file = os.path.join(get_account_session_dir(email), "ide_tokens.json")
    if not os.path.isfile(ide_file):
        return False

    if not os.path.isfile(IDE_STATE_DB):
        return False

    try:
        with open(ide_file, "r", encoding="utf-8") as fp:
            saved = json.load(fp)

        conn = sqlite3.connect(IDE_STATE_DB, timeout=5)
        c = conn.cursor()
        for k, v in saved.items():
            c.execute("INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?, ?)", (k, v))
        conn.commit()
        conn.close()
        return True
    except Exception as e:
        print(f"[Switcher] Error restoring IDE state: {e}")
        return False


def get_antigravity_pids() -> list[int]:
    """Finds PIDs of running Antigravity processes."""
    pids = []
    try:
        out = subprocess.check_output(["pgrep", "-f", "/snap/antigravity/.*/antigravity"], stderr=subprocess.DEVNULL)
        for line in out.splitlines():
            pid = int(line.strip())
            if pid != os.getpid():
                pids.append(pid)
    except Exception:
        pass
    return pids


def is_antigravity_running() -> bool:
    return len(get_antigravity_pids()) > 0


def restart_antigravity() -> bool:
    """Terminates running Antigravity instances and relaunches cleanly."""
    pids = get_antigravity_pids()
    if pids:
        # Graceful SIGTERM
        for p in pids:
            try:
                os.kill(p, signal.SIGTERM)
            except OSError:
                pass

        # Wait up to 3 seconds for exit
        for _ in range(30):
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

    # Relaunch in background
    antigravity_bin = shutil.which("antigravity") or "/snap/bin/antigravity"
    if os.path.isfile(antigravity_bin):
        env = os.environ.copy()
        subprocess.Popen(
            [antigravity_bin],
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True
        )
        return True
    return False


def switch_to_account(target_email: str, restart: bool = True) -> Tuple[bool, str]:
    """Switches the active session to target_email across Desktop App and IDE."""
    target_clean = target_email.strip().lower()
    registered = accounts.load_accounts()

    if target_clean not in registered:
        return False, f"Account '{target_clean}' is not in the registered accounts list."

    # 1. Identify currently active account to snapshot it before swapping
    current_app = auth.discover_antigravity_desktop_app()
    current_email = current_app.get("email") if current_app else None
    if not current_email:
        # Check accounts with active flag
        for em, acc in registered.items():
            if acc.get("is_current_desktop_session") or acc.get("is_current_ide_session"):
                current_email = em
                break

    if current_email and current_email.lower() == target_clean:
        return True, f"Account '{target_clean}' is already the currently active account."

    if current_email:
        snapshot_account(current_email)

    # 2. Check if we have a saved desktop snapshot for the target account
    target_session_dir = get_account_session_dir(target_clean)
    has_desktop_snap = os.path.isdir(os.path.join(target_session_dir, "desktop"))
    has_ide_snap = os.path.isfile(os.path.join(target_session_dir, "ide_tokens.json"))

    was_running = is_antigravity_running()

    restored_desktop = False
    if has_desktop_snap:
        restored_desktop = restore_desktop_session(target_clean)

    restored_ide = False
    if has_ide_snap:
        restored_ide = restore_ide_session(target_clean)

    # 3. Handle relaunch if requested and previously running
    if restart and was_running:
        restart_antigravity()
        time.sleep(2.0)  # Brief grace period for app to re-initialize

    # 4. Refresh registry status
    accounts.sync_from_antigravity()

    if restored_desktop or restored_ide:
        msg = f"✓ Switched active session to {target_clean}!"
        if not has_desktop_snap and was_running:
            msg += "\n(Note: Target account had no Desktop App snapshot yet. Please sign into Antigravity once to capture it)."
        return True, msg
    else:
        return False, (
            f"No saved session snapshot found for {target_clean}.\n"
            f"Please sign in once in Antigravity or Antigravity IDE so the tracker can capture the session."
        )
