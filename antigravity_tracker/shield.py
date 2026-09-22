import os
import sys
import json
import time
import signal
import subprocess
from typing import Dict, Any, List, Optional, Tuple

from . import geo
from . import notifier
from . import switcher

CONFIG_DIR = os.path.expanduser("~/.config/antigravity-token-tracker")
SHIELD_CONFIG_FILE = os.path.join(CONFIG_DIR, "shield.json")

DEFAULT_SHIELD_CONFIG: Dict[str, Any] = {
    "enabled": True,
    "mode": "restricted",  # "restricted" or "whitelist"
    "allowed_countries": ["US", "DE", "GB", "NL", "CA", "FR", "JP", "CH"],
    "action": "kill_process",  # "kill_process", "kill_network", or "both"
    "auto_relaunch": False,
    "cooldown_seconds": 60
}

_LAST_TRIGGER_TIMESTAMP: float = 0.0
_WAS_VIOLATING: bool = False


def load_shield_config() -> Dict[str, Any]:
    """Loads shield configuration with default fallback."""
    os.makedirs(CONFIG_DIR, mode=0o700, exist_ok=True)
    if not os.path.isfile(SHIELD_CONFIG_FILE):
        save_shield_config(DEFAULT_SHIELD_CONFIG)
        return dict(DEFAULT_SHIELD_CONFIG)

    try:
        with open(SHIELD_CONFIG_FILE, "r", encoding="utf-8") as f:
            data = json.load(f)
            merged = {**DEFAULT_SHIELD_CONFIG, **data}
            return merged
    except Exception as e:
        print(f"[Shield] Error reading {SHIELD_CONFIG_FILE}: {e}")
        return dict(DEFAULT_SHIELD_CONFIG)


def save_shield_config(cfg: Dict[str, Any]) -> None:
    """Saves shield configuration with secure file permissions."""
    os.makedirs(CONFIG_DIR, mode=0o700, exist_ok=True)
    tmp_file = SHIELD_CONFIG_FILE + ".tmp"
    with open(tmp_file, "w", encoding="utf-8") as f:
        json.dump(cfg, f, indent=2)
    os.chmod(tmp_file, 0o600)
    os.replace(tmp_file, SHIELD_CONFIG_FILE)


def set_shield_enabled(enabled: bool) -> Dict[str, Any]:
    """Enables or disables the IP Killswitch Shield."""
    cfg = load_shield_config()
    cfg["enabled"] = bool(enabled)
    save_shield_config(cfg)
    return cfg


def get_running_antigravity_pids() -> List[int]:
    """Finds all PIDs of running Antigravity processes (GUI and Language Server)."""
    pids = set()
    current_pid = os.getpid()

    # Match patterns for Antigravity desktop binary and language_server
    patterns = [
        r"/snap/antigravity/.*/antigravity",
        r"/antigravity\b",
        r"language_server.*--app_data_dir.*antigravity",
        r"language_server.*antigravity"
    ]

    for pat in patterns:
        try:
            out = subprocess.check_output(["pgrep", "-f", pat], stderr=subprocess.DEVNULL)
            for line in out.splitlines():
                pid = int(line.strip())
                if pid != current_pid:
                    # Avoid killing ourselves or our parent python runner
                    try:
                        with open(f"/proc/{pid}/cmdline", "rb") as f:
                            cmdline = f.read().decode("utf-8", errors="ignore")
                            if "antigravity-token-tracker" in cmdline or "agy-token" in cmdline:
                                continue
                    except Exception:
                        pass
                    pids.add(pid)
        except Exception:
            continue

    return sorted(list(pids))


def kill_antigravity_processes(force: bool = True) -> Tuple[int, List[int]]:
    """Instantly terminates all running Antigravity and Language Server instances."""
    pids = get_running_antigravity_pids()
    if not pids:
        return 0, []

    # 1. Send SIGTERM first for graceful flush
    for p in pids:
        try:
            os.kill(p, signal.SIGTERM)
        except OSError:
            pass

    # 2. Wait up to 1.5 seconds for processes to exit
    dead_pids = []
    for _ in range(15):
        time.sleep(0.1)
        alive = [p for p in pids if os.path.exists(f"/proc/{p}")]
        if not alive:
            break

    # 3. Apply SIGKILL on any remaining processes if force is True
    if force:
        alive = [p for p in pids if os.path.exists(f"/proc/{p}")]
        for p in alive:
            try:
                os.kill(p, signal.SIGKILL)
            except OSError:
                pass

    return len(pids), pids


def kill_network() -> bool:
    """Disables OS networking via NetworkManager (emergency cut)."""
    try:
        res = subprocess.run(["nmcli", "networking", "off"], capture_output=True, timeout=3, check=False)
        return res.returncode == 0
    except Exception as e:
        print(f"[Shield] Error running nmcli networking off: {e}")
        return False


def is_ip_allowed(geo_info: Dict[str, Any], cfg: Optional[Dict[str, Any]] = None) -> Tuple[bool, str]:
    """Determines if the egress IP complies with current Shield policy."""
    if not cfg:
        cfg = load_shield_config()

    if not cfg.get("enabled", True):
        return True, "Shield is disabled"

    country_code = (geo_info.get("country_code") or "").upper()
    country_name = geo_info.get("country_name") or "Unknown"
    ip = geo_info.get("ip") or "Unknown"
    mode = cfg.get("mode", "restricted")

    # If IP is unknown/offline, do not trigger false positive kill unless configured
    if ip == "Unknown" or not country_code:
        return True, "IP detection pending or offline"

    if mode == "restricted":
        if geo_info.get("is_restricted") or geo.is_restricted_country(country_code):
            return False, f"Egress IP {ip} is located in restricted region: {country_name} ({country_code})"
        return True, f"Country {country_code} is safe"

    elif mode == "whitelist":
        allowed = [c.upper() for c in cfg.get("allowed_countries", [])]
        if country_code not in allowed:
            return False, f"Country {country_code} ({country_name}) is outside allowed whitelist: {', '.join(allowed)}"
        return True, f"Country {country_code} is in allowed whitelist"

    return True, "Policy check passed"


def evaluate_and_enforce_shield(geo_info: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
    """Checks egress IP and immediately enforces killswitch if a violation is detected."""
    global _LAST_TRIGGER_TIMESTAMP, _WAS_VIOLATING

    cfg = load_shield_config()
    if not cfg.get("enabled", True):
        return {"enabled": False, "violation": False, "message": "Shield disabled"}

    if geo_info is None:
        geo_info = geo.get_ip_geo(force_refresh=False)

    allowed, reason = is_ip_allowed(geo_info, cfg)
    now = time.time()

    if not allowed:
        # Violation detected!
        _WAS_VIOLATING = True
        action = cfg.get("action", "kill_process")
        killed_count = 0
        killed_pids: List[int] = []
        network_killed = False

        if action in ("kill_process", "both"):
            killed_count, killed_pids = kill_antigravity_processes(force=True)

        if action in ("kill_network", "both"):
            network_killed = kill_network()

        # Send alert if cooldown passed
        cooldown = cfg.get("cooldown_seconds", 60)
        if (now - _LAST_TRIGGER_TIMESTAMP) > cooldown:
            _LAST_TRIGGER_TIMESTAMP = now
            flag = geo_info.get("flag", "⚠️")
            ip = geo_info.get("ip", "Unknown")
            action_desc = "Terminated Antigravity processes" if action == "kill_process" else ("Blocked Internet" if action == "kill_network" else "Terminated Antigravity & Blocked Internet")
            notifier.send_desktop_notification(
                "🚨 EMERGENCY IP KILLSWITCH TRIGGERED",
                f"{flag} Reason: {reason}\n🛡️ Action: {action_desc} ({killed_count} processes killed)\n⚠️ Reconnect your VPN to restore Antigravity!",
                urgency="critical"
            )

        return {
            "enabled": True,
            "violation": True,
            "reason": reason,
            "action_taken": action,
            "processes_killed": killed_count,
            "pids_killed": killed_pids,
            "network_killed": network_killed,
            "geo": geo_info
        }

    else:
        # Safe egress
        if _WAS_VIOLATING:
            _WAS_VIOLATING = False
            flag = geo_info.get("flag", "🌐")
            ip = geo_info.get("ip", "Unknown")
            country = geo_info.get("country_name", "Unknown")
            notifier.send_desktop_notification(
                "✓ Safe Egress Restored",
                f"{flag} Egress IP is now safe: {ip} ({country})\nAntigravity is safe to use.",
                urgency="normal"
            )
            if cfg.get("auto_relaunch", False):
                switcher.restart_antigravity()

        return {
            "enabled": True,
            "violation": False,
            "message": "Safe egress",
            "geo": geo_info
        }
