import time
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
CACHE_TTL_SECONDS = 60  # 1 minute local cache


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
        return {
            "email": email,
            "error": "Could not retrieve quota summary. Ensure account is logged into Antigravity or Antigravity IDE.",
            "groups": [],
            "models": []
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
        "checked_at": now
    }

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
