import os
import json
import time
import threading
import urllib.request
from typing import Dict, Any, Optional

_GEO_CACHE: Optional[Dict[str, Any]] = None
_CACHE_TIMESTAMP: float = 0.0
_GEO_LOCK = threading.Lock()
CACHE_TTL = 60.0  # 60 seconds cache

# Countries where Google Antigravity / Gemini Code Assist APIs are restricted / blocked
RESTRICTED_COUNTRIES = {"IR", "RU", "BY", "KP", "CU", "SY", "CN"}


def get_flag_emoji(country_code: str) -> str:
    """Converts a 2-letter ISO country code into a unicode flag emoji."""
    if not country_code or len(country_code) != 2:
        return "🌐"
    return "".join(chr(127397 + ord(c)) for c in country_code.upper())


def is_restricted_country(country_code: str) -> bool:
    """Returns True if the country code is in Antigravity's restricted list."""
    return (country_code or "").upper() in RESTRICTED_COUNTRIES


def format_ip_summary(geo: Dict[str, Any], include_ip: bool = True) -> str:
    """Formats a concise single-line representation of the egress geolocation."""
    flag = geo.get("flag") or get_flag_emoji(geo.get("country_code", ""))
    cc = geo.get("country_code") or "Unknown"
    ip = geo.get("ip") or "Unknown"
    city = geo.get("city") or ""
    
    loc_part = f"{city}, {cc}" if city else cc
    if include_ip and ip != "Unknown":
        return f"{flag} {loc_part} ({ip})"
    return f"{flag} {loc_part}"


def query_ip_geo() -> Optional[Dict[str, Any]]:
    """Queries external IP geolocation services with fallback."""
    endpoints = [
        "https://ipwho.is/",
        "https://ipapi.co/json/"
    ]
    for url in endpoints:
        try:
            req = urllib.request.Request(
                url,
                headers={"User-Agent": "antigravity-token-tracker/1.0"}
            )
            with urllib.request.urlopen(req, timeout=3.5) as resp:
                data = json.loads(resp.read().decode("utf-8"))
                ip = data.get("ip")
                if not ip:
                    continue

                country_code = (data.get("country_code") or data.get("country") or "").upper()
                country_name = data.get("country") or data.get("country_name") or "Unknown"
                if len(country_code) > 2 and len(country_name) <= 2:
                    # Swap if fields were inverted
                    country_code, country_name = country_name.upper(), country_code
                if len(country_code) != 2:
                    country_code = (data.get("country_code") or "")[:2].upper()

                city = data.get("city") or ""
                region = data.get("region") or data.get("region_name") or ""
                
                # Connection / ISP
                connection = data.get("connection") or {}
                isp = connection.get("isp") or data.get("org") or connection.get("org") or ""
                org = connection.get("org") or data.get("org") or ""

                flag = data.get("flag", {}).get("emoji") if isinstance(data.get("flag"), dict) else None
                if not flag:
                    flag = get_flag_emoji(country_code)

                is_restricted = is_restricted_country(country_code)

                info = {
                    "ip": ip,
                    "country_code": country_code,
                    "country_name": country_name,
                    "region": region,
                    "city": city,
                    "isp": isp,
                    "org": org,
                    "flag": flag,
                    "is_restricted": is_restricted,
                    "checked_at": time.time()
                }
                info["summary"] = format_ip_summary(info)
                return info
        except Exception:
            continue
    return None


def get_ip_geo(force_refresh: bool = False) -> Dict[str, Any]:
    """Returns cached IP geolocation data, refreshing in background or directly if expired."""
    global _GEO_CACHE, _CACHE_TIMESTAMP
    now = time.time()

    with _GEO_LOCK:
        if not force_refresh and _GEO_CACHE and (now - _CACHE_TIMESTAMP < CACHE_TTL):
            return _GEO_CACHE

    res = query_ip_geo()
    with _GEO_LOCK:
        if res:
            _GEO_CACHE = res
            _CACHE_TIMESTAMP = now
            return res

        # Return cached data if available on network failure
        if _GEO_CACHE:
            return _GEO_CACHE

    fallback = {
        "ip": "Unknown",
        "country_code": "",
        "country_name": "Checking...",
        "region": "",
        "city": "",
        "isp": "",
        "org": "",
        "flag": "🌐",
        "is_restricted": False,
        "checked_at": now,
        "summary": "🌐 Egress IP Checking..."
    }
    return fallback


def refresh_ip_geo_async():
    """Triggers an asynchronous background update without blocking the caller."""
    def worker():
        get_ip_geo(force_refresh=True)
    threading.Thread(target=worker, daemon=True).start()
