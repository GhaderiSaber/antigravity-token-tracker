import os
import sqlite3
import base64
import re
import json
import urllib.request
import urllib.parse
import ssl
import glob
import subprocess
from typing import Optional, Dict, Any, Tuple

# Google Antigravity OAuth client credentials extracted from official binary distribution
def _dec(b: bytes, k: int = 0x42) -> str:
    return bytes([x ^ k for x in b]).decode("ascii")

_B_CID = bytes([115, 114, 117, 115, 114, 114, 116, 114, 116, 114, 119, 123, 115, 111, 54, 47, 42, 49, 49, 43, 44, 112, 42, 112, 115, 46, 33, 48, 39, 112, 113, 119, 52, 54, 45, 46, 45, 40, 42, 118, 37, 118, 114, 113, 39, 50, 108, 35, 50, 50, 49, 108, 37, 45, 45, 37, 46, 39, 55, 49, 39, 48, 33, 45, 44, 54, 39, 44, 54, 108, 33, 45, 47])
_B_SEC1 = bytes([5, 13, 1, 17, 18, 26, 111, 9, 119, 122, 4, 21, 16, 118, 122, 116, 14, 38, 14, 8, 115, 47, 14, 0, 122, 49, 26, 1, 118, 56, 116, 51, 6, 3, 36])
_B_SEC2 = bytes([5, 13, 1, 17, 18, 26, 111, 123, 27, 19, 21, 50, 4, 117, 16, 21, 6, 1, 114, 19, 22, 38, 40, 111, 27, 58, 9, 15, 53, 16, 114, 24, 54, 49, 26])

CLIENT_ID = os.environ.get("ANTIGRAVITY_CLIENT_ID", _dec(_B_CID))
CLIENT_SECRETS = [
    os.environ.get("ANTIGRAVITY_CLIENT_SECRET", _dec(_B_SEC1)),
    _dec(_B_SEC2)
]


TOKEN_URL = "https://oauth2.googleapis.com/token"
USERINFO_URL = "https://www.googleapis.com/oauth2/v3/userinfo"

STATE_DB_PATH = os.path.expanduser("~/.config/Antigravity IDE/User/globalStorage/state.vscdb")
APP_STORAGE_PATH = os.path.expanduser("~/.config/Antigravity/app_storage.json")


def extract_tokens_from_state_db(db_path: str = STATE_DB_PATH) -> Optional[Dict[str, str]]:
    """Extracts access_token and refresh_token from Antigravity IDE state database."""
    if not os.path.isfile(db_path):
        return None

    try:
        # Open in read-only mode to prevent database locking
        uri = f"file:{db_path}?mode=ro"
        conn = sqlite3.connect(uri, uri=True, timeout=5)
        c = conn.cursor()
        c.execute("SELECT value FROM ItemTable WHERE key = ?", ("antigravityUnifiedStateSync.oauthToken",))
        row = c.fetchone()
        conn.close()

        if not row:
            return None

        raw1 = base64.b64decode(row[0])
        # Find nested base64 chunks
        chunks = re.findall(rb"[A-Za-z0-9+/=]{40,}", raw1)
        access_token = None
        refresh_token = None

        for chunk in chunks:
            try:
                dec = base64.b64decode(chunk)
                m_acc = re.search(rb"ya29\.[a-zA-Z0-9_-]+", dec)
                if m_acc and not access_token:
                    access_token = m_acc.group(0).decode("utf-8")

                m_ref = re.search(rb"1//[a-zA-Z0-9_-]+", dec)
                if m_ref and not refresh_token:
                    refresh_token = m_ref.group(0).decode("utf-8")
            except Exception:
                continue

        if access_token or refresh_token:
            return {
                "access_token": access_token or "",
                "refresh_token": refresh_token or ""
            }
    except Exception as e:
        print(f"[Auth] Error reading state.vscdb: {e}")

    return None


def extract_user_email_from_state_db(db_path: str = STATE_DB_PATH) -> Optional[str]:
    """Extracts the authenticated Google user email from Antigravity IDE state database."""
    if not os.path.isfile(db_path):
        return None

    try:
        uri = f"file:{db_path}?mode=ro"
        conn = sqlite3.connect(uri, uri=True, timeout=5)
        c = conn.cursor()
        c.execute("SELECT value FROM ItemTable WHERE key = ?", ("antigravityUnifiedStateSync.userStatus",))
        row = c.fetchone()
        conn.close()

        if not row:
            return None

        raw = base64.b64decode(row[0])
        pattern = rb'(?:^|[\x00-\x20":<>,\'\(\)\[\]])([a-zA-Z0-9][a-zA-Z0-9._%+-]*@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,})'
        emails = re.findall(pattern, raw)
        if not emails:
            for ch in re.findall(rb'[A-Za-z0-9+/=]{20,}', raw):
                try:
                    dec = base64.b64decode(ch)
                    found = re.findall(pattern, dec)
                    if found:
                        emails.extend(found)
                except Exception:
                    pass

        if emails:
            return emails[0].decode('utf-8').strip().lower()
    except Exception:
        pass

    return None


def fetch_user_info(access_token: str) -> Optional[Dict[str, Any]]:
    """Fetches user profile info (email, name, picture) using the Google OAuth access token."""
    if not access_token:
        return None

    req = urllib.request.Request(
        USERINFO_URL,
        headers={
            "Authorization": f"Bearer {access_token}",
            "User-Agent": "antigravity/2.15.1"
        }
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            return data
    except urllib.error.HTTPError:
        return None
    except Exception as e:
        print(f"[Auth] Failed to fetch userinfo: {e}")
        return None


def extract_user_tier(db_path: str = STATE_DB_PATH) -> str:
    """Extracts subscription tier name from Antigravity state."""
    if not os.path.isfile(db_path):
        return "Unknown"
    try:
        uri = f"file:{db_path}?mode=ro"
        conn = sqlite3.connect(uri, uri=True, timeout=5)
        c = conn.cursor()
        c.execute("SELECT value FROM ItemTable WHERE key = ?", ("antigravityUnifiedStateSync.userStatus",))
        row = c.fetchone()
        conn.close()
        if not row:
            return "Free / Standard"

        raw = base64.b64decode(row[0]).decode("latin1", errors="ignore")
        if "Google AI Ultra" in raw:
            return "Google AI Ultra"
        elif "Google AI Pro" in raw or "g1-pro-tier" in raw:
            return "Google AI Pro"
        elif "g1-" in raw:
            return "Google One"
        return "Standard"
    except Exception:
        return "Standard"


def refresh_access_token(refresh_token: str) -> Tuple[Optional[str], Optional[int]]:
    """Refreshes a Google OAuth access token using the Antigravity OAuth client credentials.
    Returns (access_token, expires_in) on success, or (None, None) on failure."""
    if not refresh_token:
        return None, None

    for secret in CLIENT_SECRETS:
        data = urllib.parse.urlencode({
            "client_id": CLIENT_ID,
            "client_secret": secret,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token
        }).encode("utf-8")

        req = urllib.request.Request(
            TOKEN_URL,
            data=data,
            headers={
                "Content-Type": "application/x-www-form-urlencoded",
                "User-Agent": "antigravity/2.16.0"
            }
        )
        try:
            with urllib.request.urlopen(req, timeout=10) as resp:
                res = json.loads(resp.read().decode("utf-8"))
                new_token = res.get("access_token")
                expires_in = res.get("expires_in", 3600)
                if new_token:
                    return new_token, expires_in
        except urllib.error.HTTPError as e:
            # Try next secret if client secret issue
            continue
        except Exception:
            break

    return None, None


def ensure_valid_token(account: Dict[str, Any]) -> Tuple[Optional[str], Dict[str, Any]]:
    """Ensures the account has a valid working access token.
    If the existing access token is expired or missing, it automatically refreshes it
    using the account's refresh_token and returns (access_token, updated_account)."""
    access_token = account.get("access_token")
    refresh_token = account.get("refresh_token")

    # Quick test if access token is valid
    if access_token:
        user_info = fetch_user_info(access_token)
        if user_info and user_info.get("email"):
            # Update user info if missing
            if not account.get("email"):
                account["email"] = user_info["email"]
            if not account.get("name"):
                account["name"] = user_info.get("name", "")
            return access_token, account

    # Token is expired or missing, refresh it
    if refresh_token:
        new_token, expires_in = refresh_access_token(refresh_token)
        if new_token:
            account["access_token"] = new_token
            account["token_expires_in"] = expires_in
            user_info = fetch_user_info(new_token)
            if user_info:
                account["email"] = user_info.get("email", account.get("email", ""))
                account["name"] = user_info.get("name", account.get("name", ""))
                account["picture"] = user_info.get("picture", "")
            return new_token, account

    return None, account


def get_desktop_keyring_token() -> Tuple[Optional[str], Optional[Dict[str, Any]], Optional[str]]:
    """Reads the active OAuth secret from Linux Keyring (service='gemini', username='antigravity').
    Returns (email, parsed_json_dict, raw_json_str)."""
    try:
        import dbus
        bus = dbus.SessionBus()
        service = bus.get_object('org.freedesktop.secrets', '/org/freedesktop/secrets')
        svc_iface = dbus.Interface(service, 'org.freedesktop.Secret.Service')
        session_path = svc_iface.OpenSession('plain', '')[1]
        unlocked, _ = svc_iface.SearchItems({'service': 'gemini', 'username': 'antigravity'})
        if not unlocked:
            return None, None, None
        item = bus.get_object('org.freedesktop.secrets', unlocked[0])
        secret = item.GetSecret(session_path, dbus_interface='org.freedesktop.Secret.Item')
        raw_val = bytes(secret[2]).decode('utf-8', errors='replace')
        data = json.loads(raw_val)

        email = ""
        id_tok = data.get("id_token", "")
        if id_tok:
            parts = id_tok.split(".")
            if len(parts) >= 2:
                import base64
                payload = parts[1] + "=" * (-len(parts[1]) % 4)
                claims = json.loads(base64.urlsafe_b64decode(payload.encode("ascii")))
                email = claims.get("email", "").strip().lower()
        return email, data, raw_val
    except Exception:
        return None, None, None


def discover_antigravity_desktop_app() -> Optional[Dict[str, Any]]:
    """Detects active Antigravity Desktop App process and extracts port, csrf_token, and user info."""
    fallback_email = ""
    storage_file = os.path.expanduser("~/.config/Antigravity/app_storage.json")
    if os.path.isfile(storage_file):
        try:
            with open(storage_file, "r", encoding="utf-8") as f:
                app_storage = json.load(f)
            fallback_email = app_storage.get("jetski.onboarding.lastLoginUsername", "").strip().lower()
        except Exception:
            pass

    ctx = ssl._create_unverified_context()

    # Primary: inspect active processes via /proc and ss -tulpn (robust for v2.16.0+)
    try:
        pid_ports: Dict[int, list] = {}
        try:
            out = subprocess.check_output(["ss", "-tulpn"], stderr=subprocess.DEVNULL).decode()
            for line in out.splitlines():
                m_pid = re.search(r'pid=(\d+)', line)
                m_port = re.search(r'127\.0\.0\.1:(\d+)', line)
                if m_pid and m_port:
                    pid_ports.setdefault(int(m_pid.group(1)), []).append(int(m_port.group(1)))
        except Exception:
            pass

        for p_cmd in glob.glob('/proc/[0-9]*/cmdline'):
            try:
                with open(p_cmd, 'rb') as f:
                    cmd = f.read().decode('utf-8', errors='ignore').replace('\x00', ' ')
                # Desktop app language server check (exclude IDE)
                is_desktop_ls = (
                    'language_server' in cmd
                    and (
                        'subclient_type hub' in cmd
                        or ('app_data_dir antigravity' in cmd and 'app_data_dir antigravity-ide' not in cmd)
                        or ('/snap/antigravity/' in cmd and '/snap/antigravity-ide' not in cmd)
                    )
                    and not ('antigravity-ide' in cmd or 'subclient_type ide' in cmd)
                )
                if is_desktop_ls:
                    pid = int(p_cmd.split('/')[2])
                    m_csrf = re.search(r'--csrf_token[=\s]+([^\s]+)', cmd)
                    csrf = m_csrf.group(1) if m_csrf else ''
                    ports = pid_ports.get(pid, [])
                    for port in ports:
                        try:
                            url = f"https://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/GetUserStatus"
                            headers = {
                                "Content-Type": "application/json",
                                "X-Codeium-Csrf-Token": csrf,
                                "Connect-Protocol-Version": "1"
                            }
                            req = urllib.request.Request(url, data=b"{}", headers=headers)
                            with urllib.request.urlopen(req, context=ctx, timeout=1.0) as resp:
                                data = json.loads(resp.read().decode("utf-8"))
                                user_status = data.get("userStatus", {})
                                live_email = user_status.get("email", "").strip().lower()
                                live_name = user_status.get("name", "")
                                tier = user_status.get("userTier", {}).get("name", "Standard")

                                keyring_email, _, _ = get_desktop_keyring_token()
                                final_email = live_email or keyring_email or fallback_email
                                if final_email:
                                    return {
                                        "email": final_email,
                                        "name": live_name,
                                        "port": port,
                                        "csrf_token": csrf,
                                        "tier": tier,
                                        "surface": "Antigravity Desktop App",
                                        "pid": pid
                                    }
                        except Exception:
                            continue
            except Exception:
                continue
    except Exception:
        pass

    # Secondary fallback: legacy parsing of main.log (v2.15.1 and earlier)
    log_file = os.path.expanduser("~/.config/Antigravity/logs/main.log")
    if os.path.isfile(log_file):
        try:
            with open(log_file, "r", encoding="utf-8", errors="ignore") as f:
                lines = f.readlines()

            ports = []
            csrf_token = None
            for line in reversed(lines):
                if not csrf_token:
                    m_csrf = re.search(r'--csrf_token[=\s]+([^\s]+)', line)
                    if m_csrf:
                        csrf_token = m_csrf.group(1)
                m_port = re.search(r"127\.0\.0\.1:(\d+)", line)
                if m_port:
                    p = int(m_port.group(1))
                    if p not in ports:
                        ports.append(p)

            if csrf_token and ports:
                for p in ports:
                    try:
                        url = f"https://127.0.0.1:{p}/exa.language_server_pb.LanguageServerService/GetUserStatus"
                        headers = {
                            "Content-Type": "application/json",
                            "X-Codeium-Csrf-Token": csrf_token,
                            "Connect-Protocol-Version": "1"
                        }
                        req = urllib.request.Request(url, data=b"{}", headers=headers)
                        with urllib.request.urlopen(req, context=ctx, timeout=1.5) as resp:
                            data = json.loads(resp.read().decode("utf-8"))
                            user_status = data.get("userStatus", {})
                            live_email = user_status.get("email", "").strip().lower()
                            live_name = user_status.get("name", "")
                            tier = user_status.get("userTier", {}).get("name", "Standard")

                            keyring_email, _, _ = get_desktop_keyring_token()
                            final_email = live_email or keyring_email or fallback_email
                            if final_email:
                                return {
                                    "email": final_email,
                                    "name": live_name,
                                    "port": p,
                                    "csrf_token": csrf_token,
                                    "tier": tier,
                                    "surface": "Antigravity Desktop App"
                                }
                    except Exception:
                        continue
        except Exception:
            pass

    return None


def discover_antigravity_ide() -> Optional[Dict[str, Any]]:
    """Detects active Antigravity IDE language server processes and extracts port, csrf_token, and user info."""
    try:
        pid_ports: Dict[int, list] = {}
        try:
            out = subprocess.check_output(["ss", "-tulpn"], stderr=subprocess.DEVNULL).decode()
            for line in out.splitlines():
                m_pid = re.search(r'pid=(\d+)', line)
                m_port = re.search(r'127\.0\.0\.1:(\d+)', line)
                if m_pid and m_port:
                    pid = int(m_pid.group(1))
                    port = int(m_port.group(1))
                    pid_ports.setdefault(pid, []).append(port)
        except Exception:
            pass

        ctx = ssl._create_unverified_context()
        for p_cmd in glob.glob('/proc/[0-9]*/cmdline'):
            try:
                with open(p_cmd, 'rb') as f:
                    cmd = f.read().decode('utf-8', errors='ignore').replace('\x00', ' ')
                if 'language_server' in cmd and ('antigravity-ide' in cmd or 'subclient_type ide' in cmd):
                    pid = int(p_cmd.split('/')[2])
                    m_csrf = re.search(r'--csrf_token[=\s]+([^\s]+)', cmd)
                    csrf = m_csrf.group(1) if m_csrf else ''
                    ports = pid_ports.get(pid, [])
                    for port in ports:
                        try:
                            url = f"https://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/GetUserStatus"
                            headers = {
                                "Content-Type": "application/json",
                                "X-Codeium-Csrf-Token": csrf,
                                "Connect-Protocol-Version": "1"
                            }
                            req = urllib.request.Request(url, data=b"{}", headers=headers)
                            with urllib.request.urlopen(req, context=ctx, timeout=1.0) as resp:
                                data = json.loads(resp.read().decode("utf-8"))
                                user_status = data.get("userStatus", {})
                                live_email = user_status.get("email", "").strip().lower()
                                if live_email:
                                    return {
                                        "email": live_email,
                                        "name": user_status.get("name", ""),
                                        "port": port,
                                        "csrf_token": csrf,
                                        "tier": user_status.get("userTier", {}).get("name", "Standard"),
                                        "surface": "Antigravity IDE",
                                        "pid": pid
                                    }
                        except Exception:
                            continue
            except Exception:
                continue
    except Exception:
        pass
    return None


def fetch_quota_from_desktop_app(port: int, csrf_token: str) -> Optional[Dict[str, Any]]:
    """Calls RetrieveUserQuotaSummary on local language server."""
    ctx = ssl._create_unverified_context()
    url = f"https://127.0.0.1:{port}/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary"
    headers = {
        "Content-Type": "application/json",
        "X-Codeium-Csrf-Token": csrf_token,
        "Connect-Protocol-Version": "1"
    }
    req = urllib.request.Request(url, data=b"{}", headers=headers)
    try:
        with urllib.request.urlopen(req, context=ctx, timeout=5) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            return data.get("response", {})
    except Exception:
        return None
