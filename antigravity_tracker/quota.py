import time
import os
import json
import urllib.request
import urllib.error
from typing import Dict, Any, Optional, List
from . import auth
from . import accounts

QUOTA_SUMMARY_URL = "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary"
QUOTA_MODELS_URL = "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota"

# In-memory cache per email: {email: (timestamp, data)}
_QUOTA_CACHE: Dict[str, tuple[float, Dict[str, Any]]] = {}
CACHE_TTL_SECONDS = 600  # 10 minutes local cache for inactive accounts


def get_historical_quota(email: str) -> Optional[Dict[str, Any]]:
    """Recovers last logged quota from token_history.jsonl if available."""
    history_file = os.path.expanduser("~/.gemini/antigravity/token_history.jsonl")
    if not os.path.isfile(history_file):
        return None

    email_clean = email.strip().lower()
    last_record = None
    last_timestamp = None

    try:
        with open(history_file, "r", encoding="utf-8") as f:
            for line in f:
                try:
                    data = json.loads(line)
                    accs = data.get("accounts", {})
                    if email_clean in accs:
                        g_data = accs[email_clean].get("groups", {})
                        if g_data and any(v.get("weekly_pct") is not None for v in g_data.values()):
                            last_record = accs[email_clean]
                            last_timestamp = data.get("timestamp")
                except Exception:
                    continue
    except Exception:
        return None

    if not last_record:
        return None

    parsed_groups = []
    for g_name, g_vals in last_record.get("groups", {}).items():
        w_pct = g_vals.get("weekly_pct")
        w_reset = g_vals.get("weekly_reset")
        f_pct = g_vals.get("5h_pct")

        w_bucket = None
        if w_pct is not None:
            w_bucket = {
                "bucketId": f"{g_name.lower().replace(' ', '-')}-weekly",
                "window": "weekly",
                "displayName": g_name,
                "remainingFraction": round(float(w_pct) / 100.0, 4),
                "remainingPercent": round(float(w_pct), 2),
                "resetTime": w_reset or "",
                "description": ""
            }

        f_bucket = None
        if f_pct is not None:
            f_bucket = {
                "bucketId": f"{g_name.lower().replace(' ', '-')}-5h",
                "window": "5h",
                "displayName": g_name,
                "remainingFraction": round(float(f_pct) / 100.0, 4),
                "remainingPercent": round(float(f_pct), 2),
                "resetTime": "",
                "description": ""
            }

        parsed_groups.append({
            "displayName": g_name,
            "description": "",
            "weekly": w_bucket,
            "fiveHour": f_bucket
        })

    return {
        "groups": parsed_groups,
        "models": [],
        "checked_at": last_timestamp or time.time()
    }


def fetch_account_quota(account: Dict[str, Any], force_refresh: bool = False) -> Optional[Dict[str, Any]]:
    """Fetches real-time quota status for the specified account profile.
    Uses in-memory cache if queried within CACHE_TTL_SECONDS unless force_refresh is True."""
    email = account.get("email", "").lower()
    now = time.time()

    if not force_refresh and email in _QUOTA_CACHE:
        cached_time, cached_data = _QUOTA_CACHE[email]
        if now - cached_time < CACHE_TTL_SECONDS:
            return cached_data

    quota_summary_data = None
    models_data = None

    # Option A: Check if account is actively running in Antigravity Desktop App
    desktop_session = auth.discover_antigravity_desktop_app()
    if desktop_session and desktop_session.get("email") == email:
        port = desktop_session.get("port")
        csrf_token = desktop_session.get("csrf_token")
        quota_summary_data = auth.fetch_quota_from_desktop_app(port, csrf_token)

    # Option B: Query Google CloudCode directly using OAuth token
    if not quota_summary_data and (account.get("access_token") or account.get("refresh_token")):
        token, updated_acc = auth.ensure_valid_token(account)
        if token:
            if updated_acc != account:
                accounts.upsert_account(updated_acc)

            headers = {
                "Authorization": f"Bearer {token}",
                "Content-Type": "application/json",
                "User-Agent": "antigravity/2.15.1"
            }

            try:
                req = urllib.request.Request(QUOTA_SUMMARY_URL, data=b"{}", headers=headers)
                with urllib.request.urlopen(req, timeout=12) as resp:
                    quota_summary_data = json.loads(resp.read().decode("utf-8"))
            except urllib.error.HTTPError as e:
                print(f"[Quota] Error fetching quota summary for {email}: HTTP {e.code}")
            except Exception as e:
                print(f"[Quota] Network exception for {email}: {e}")

            try:
                req = urllib.request.Request(QUOTA_MODELS_URL, data=b"{}", headers=headers)
                with urllib.request.urlopen(req, timeout=12) as resp:
                    models_data = json.loads(resp.read().decode("utf-8"))
            except Exception:
                pass

    if not quota_summary_data:
        # Fallback to persistent last-known quota if available
        last_known = account.get("last_known_quota")
        if not last_known or not last_known.get("groups"):
            last_known = get_historical_quota(email)
            if last_known:
                account["last_known_quota"] = last_known
                accounts.upsert_account(account)

        if last_known and last_known.get("groups"):
            result = {
                "email": email,
                "name": account.get("name", ""),
                "tier": account.get("tier", "Standard"),
                "is_current_ide_session": account.get("is_current_ide_session", False),
                "is_current_desktop_session": account.get("is_current_desktop_session", False),
                "groups": last_known.get("groups", []),
                "models": last_known.get("models", []),
                "checked_at": last_known.get("checked_at", now),
                "is_cached": True
            }
            _QUOTA_CACHE[email] = (now, result)
            return result

        return {
            "email": email,
            "name": account.get("name", ""),
            "tier": account.get("tier", "Standard"),
            "is_current_ide_session": account.get("is_current_ide_session", False),
            "is_current_desktop_session": account.get("is_current_desktop_session", False),
            "error": "Account session inactive. Log into Antigravity to refresh quota.",
            "groups": [],
            "models": [],
            "is_cached": False
        }

    # Parse and structure groups
    parsed_groups = []
    raw_groups = quota_summary_data.get("groups", [])
    for rg in raw_groups:
        group_name = rg.get("displayName", "Unnamed Group")
        group_desc = rg.get("description", "")
        buckets = rg.get("buckets", [])

        weekly_bucket = None
        five_hour_bucket = None

        for b in buckets:
            bucket_id = b.get("bucketId", "")
            window = b.get("window", "")
            remaining_fraction = float(b.get("remainingFraction", 0.0))
            remaining_percent = round(remaining_fraction * 100, 2)
            reset_time = b.get("resetTime", "")
            desc = b.get("description", "")

            bucket_info = {
                "bucketId": bucket_id,
                "window": window,
                "displayName": b.get("displayName", bucket_id),
                "remainingFraction": remaining_fraction,
                "remainingPercent": remaining_percent,
                "resetTime": reset_time,
                "description": desc
            }

            if "weekly" in bucket_id or window == "weekly":
                weekly_bucket = bucket_info
            elif "5h" in bucket_id or window == "5h":
                five_hour_bucket = bucket_info

        parsed_groups.append({
            "displayName": group_name,
            "description": group_desc,
            "weekly": weekly_bucket,
            "fiveHour": five_hour_bucket
        })

    # Parse individual models
    parsed_models = []
    if models_data and "buckets" in models_data:
        for mb in models_data["buckets"]:
            model_id = mb.get("modelId")
            if not model_id:
                continue
            frac = float(mb.get("remainingFraction", 0.0))
            parsed_models.append({
                "modelId": model_id,
                "remainingFraction": frac,
                "remainingPercent": round(frac * 100, 2),
                "resetTime": mb.get("resetTime", "")
            })

    result = {
        "email": email,
        "name": account.get("name", ""),
        "tier": account.get("tier", "Standard"),
        "is_current_ide_session": account.get("is_current_ide_session", False),
        "is_current_desktop_session": account.get("is_current_desktop_session", False),
        "groups": parsed_groups,
        "models": parsed_models,
        "checked_at": now,
        "is_cached": False
    }

    # Automatically persist quota to accounts.json
    account["last_known_quota"] = {
        "groups": parsed_groups,
        "models": parsed_models,
        "checked_at": now
    }
    accounts.upsert_account(account)

    _QUOTA_CACHE[email] = (now, result)
    return result



def fetch_all_accounts_quota(force_refresh: bool = False) -> Dict[str, Dict[str, Any]]:
    """Fetches quota for all registered accounts."""
    all_accounts = accounts.list_accounts()
    results = {}
    for acc in all_accounts:
        email = acc.get("email")
        if email:
            results[email] = fetch_account_quota(acc, force_refresh=force_refresh)
    return results
