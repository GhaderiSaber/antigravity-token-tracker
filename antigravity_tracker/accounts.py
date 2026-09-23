import os
import json
import time
from typing import Dict, Any, List, Optional, Tuple
from . import auth

CONFIG_DIR = os.path.expanduser("~/.config/antigravity-token-tracker")
ACCOUNTS_FILE = os.path.join(CONFIG_DIR, "accounts.json")


def _ensure_config_dir():
    if not os.path.exists(CONFIG_DIR):
        os.makedirs(CONFIG_DIR, mode=0o700, exist_ok=True)
    elif (os.stat(CONFIG_DIR).st_mode & 0o777) != 0o700:
        os.chmod(CONFIG_DIR, 0o700)


def load_accounts() -> Dict[str, Dict[str, Any]]:
    """Loads all saved account profiles from accounts.json."""
    _ensure_config_dir()
    if not os.path.isfile(ACCOUNTS_FILE):
        return {}

    try:
        with open(ACCOUNTS_FILE, "r", encoding="utf-8") as f:
            data = json.load(f)
            if isinstance(data, dict):
                return data
    except Exception as e:
        print(f"[Accounts] Error reading {ACCOUNTS_FILE}: {e}")

    return {}


def save_accounts(accounts: Dict[str, Dict[str, Any]]):
    """Saves accounts dictionary to accounts.json with secure 0600 file permissions."""
    _ensure_config_dir()
    tmp_file = ACCOUNTS_FILE + ".tmp"
    with open(tmp_file, "w", encoding="utf-8") as f:
        json.dump(accounts, f, indent=2)
    os.chmod(tmp_file, 0o600)
    os.replace(tmp_file, ACCOUNTS_FILE)


def get_account(email: str) -> Optional[Dict[str, Any]]:
    """Retrieves an account profile by email."""
    accounts = load_accounts()
    return accounts.get(email.strip().lower())


def upsert_account(account_data: Dict[str, Any]) -> Dict[str, Any]:
    """Adds or updates an account profile."""
    email = account_data.get("email", "").strip().lower()
    if not email:
        raise ValueError("Account must contain a valid email.")

    accounts = load_accounts()
    existing = accounts.get(email, {})
    # Merge existing with new fields, preserving existing refresh_token if new is empty
    merged = {**existing, **account_data}
    if not merged.get("refresh_token") and existing.get("refresh_token"):
        merged["refresh_token"] = existing["refresh_token"]

    merged["updated_at"] = time.time()
    accounts[email] = merged
    save_accounts(accounts)
    return merged


def remove_account(email: str) -> bool:
    """Removes an account profile from tracking."""
    email = email.strip().lower()
    accounts = load_accounts()
    if email in accounts:
        del accounts[email]
        save_accounts(accounts)
        return True
    return False


def list_accounts() -> List[Dict[str, Any]]:
    """Returns a list of all registered account profiles."""
    accounts = load_accounts()
    return list(accounts.values())


def sync_from_antigravity() -> Tuple[Optional[str], Dict[str, Dict[str, Any]]]:
    """Inspects Antigravity Desktop App and Antigravity IDE sessions,
    discovers active credentials, and updates the account registry without erasing stored accounts.
    Returns (active_email, all_accounts)."""
    accounts = load_accounts()
    active_email = None

    # 1. Discover Antigravity Desktop App
    desktop_session = auth.discover_antigravity_desktop_app()
    desktop_email = None
    if desktop_session and desktop_session.get("email"):
        desktop_email = desktop_session["email"].lower()
        active_email = desktop_email
        existing = accounts.get(desktop_email, {})

        # Extract OAuth tokens from Linux Keyring if available
        access_tok = existing.get("access_token", "")
        refresh_tok = existing.get("refresh_token", "")
        try:
            kr_email, kr_data, _ = auth.get_desktop_keyring_token()
            if kr_data and isinstance(kr_data, dict) and "token" in kr_data:
                t = kr_data["token"]
                if t.get("access_token"):
                    access_tok = t["access_token"]
                if t.get("refresh_token"):
                    refresh_tok = t["refresh_token"]
        except Exception:
            pass

        accounts[desktop_email] = {
            **existing,
            "email": desktop_email,
            "name": desktop_session.get("name", existing.get("name", "")),
            "tier": desktop_session.get("tier", existing.get("tier", "Standard")),
            "is_current_desktop_session": True,
            "app_port": desktop_session.get("port"),
            "app_csrf_token": desktop_session.get("csrf_token"),
            "access_token": access_tok,
            "refresh_token": refresh_tok,
            "last_synced": time.time()
        }
        if access_tok and not accounts[desktop_email].get("name"):
            try:
                uinfo = auth.fetch_user_info(access_tok)
                if uinfo:
                    accounts[desktop_email]["name"] = uinfo.get("name", "")
                    accounts[desktop_email]["picture"] = uinfo.get("picture", "")
            except Exception:
                pass

        # Automatically snapshot active desktop session for seamless switching
        try:
            from . import switcher
            switcher.snapshot_desktop_session(desktop_email)
        except Exception:
            pass

    # 2. Discover Antigravity IDE
    # Primary: query live running Antigravity IDE language server processes
    ide_session = auth.discover_antigravity_ide()
    ide_email = None
    tokens = None
    if ide_session and ide_session.get("email"):
        ide_email = ide_session["email"].lower()
        if not active_email:
            active_email = ide_email
        existing_ide = accounts.get(ide_email, {})
        accounts[ide_email] = {
            **existing_ide,
            "email": ide_email,
            "name": ide_session.get("name", existing_ide.get("name", "")),
            "tier": ide_session.get("tier", existing_ide.get("tier", "Standard")),
            "is_current_ide_session": True,
            "last_synced": time.time()
        }
    else:
        # Fallback: inspect state.vscdb only when no live IDE language server is active
        tokens = auth.extract_tokens_from_state_db()
        if tokens and (tokens.get("access_token") or tokens.get("refresh_token")):
            acc_temp = {
                "access_token": tokens.get("access_token"),
                "refresh_token": tokens.get("refresh_token")
            }
            valid_token, acc_temp = auth.ensure_valid_token(acc_temp)
            email = acc_temp.get("email")

            if email:
                ide_email = email.lower()
                if not active_email:
                    active_email = ide_email
                tier = auth.extract_user_tier()
                existing_ide = accounts.get(ide_email, {})
                account_entry = {
                    "email": ide_email,
                    "name": acc_temp.get("name", existing_ide.get("name", "")),
                    "picture": acc_temp.get("picture", existing_ide.get("picture", "")),
                    "access_token": valid_token or tokens.get("access_token", ""),
                    "refresh_token": tokens.get("refresh_token") or acc_temp.get("refresh_token", existing_ide.get("refresh_token", "")),
                    "tier": tier if tier != "Standard" else existing_ide.get("tier", tier),
                    "is_current_ide_session": True,
                    "last_synced": time.time()
                }
                accounts[ide_email] = {**existing_ide, **account_entry}

    # Update activity flags for all registered accounts
    for k in accounts:
        if not desktop_email or k != desktop_email:
            accounts[k]["is_current_desktop_session"] = False
        if not ide_email or k != ide_email:
            accounts[k]["is_current_ide_session"] = False

    if desktop_session or ide_session or tokens:
        save_accounts(accounts)

    return active_email, accounts

