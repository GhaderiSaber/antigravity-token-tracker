import os
import json
import time
from typing import Dict, Any, Optional, Tuple, List

try:
    from . import accounts
    from . import switcher
    from . import failover
    from . import notifier
    from . import lifecycle
except (ImportError, ValueError):
    import sys
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from antigravity_tracker import accounts, switcher, failover, notifier, lifecycle

CONFIG_FILE = os.path.expanduser("~/.config/antigravity-token-tracker/balancer.json")
STATE_FILE = os.path.expanduser("~/.config/antigravity-token-tracker/balancer_state.json")

SUPPORTED_STRATEGIES = ["watermark", "expiry_first", "round_robin", "reactive"]

DEFAULT_CONFIG: Dict[str, Any] = {
    "enabled": False,
    "strategy": "watermark",
    "watermark_spread_pct": 20.0,
    "round_robin_interval_minutes": 60,
    "expiry_window_hours": 36,
    "min_quota_pct": 5.0,
    "cooldown_seconds": 300,
    "auto_restart": True
}


def load_balancer_config() -> Dict[str, Any]:
    """Loads balancer configuration, merged with defaults."""
    cfg = dict(DEFAULT_CONFIG)
    if os.path.isfile(CONFIG_FILE):
        try:
            with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                saved = json.load(f)
                if isinstance(saved, dict):
                    cfg.update(saved)
        except Exception:
            pass
    return cfg


def save_balancer_config(cfg: Dict[str, Any]):
    """Saves balancer configuration."""
    os.makedirs(os.path.dirname(CONFIG_FILE), exist_ok=True)
    try:
        with open(CONFIG_FILE, "w", encoding="utf-8") as f:
            json.dump(cfg, f, indent=2)
    except Exception as e:
        print(f"[Balancer] Error saving config: {e}")


def load_balancer_state() -> Dict[str, Any]:
    """Loads balancer execution state and history."""
    default_state = {
        "last_rotation_timestamp": 0.0,
        "last_from_email": None,
        "last_to_email": None,
        "last_strategy_used": None,
        "round_robin_index": 0,
        "history": []
    }
    if os.path.isfile(STATE_FILE):
        try:
            with open(STATE_FILE, "r", encoding="utf-8") as f:
                saved = json.load(f)
                if isinstance(saved, dict):
                    default_state.update(saved)
        except Exception:
            pass
    return default_state


def save_balancer_state(state: Dict[str, Any]):
    """Persists balancer state."""
    os.makedirs(os.path.dirname(STATE_FILE), exist_ok=True)
    try:
        with open(STATE_FILE, "w", encoding="utf-8") as f:
            json.dump(state, f, indent=2)
    except Exception as e:
        print(f"[Balancer] Error saving state: {e}")


def get_account_reset_seconds(account_data: Dict[str, Any]) -> int:
    """Extracts remaining seconds until weekly quota reset from analyzed account data."""
    for g in account_data.get("groups", []):
        w = g.get("weekly")
        if w and "secondsRemaining" in w:
            return int(w.get("secondsRemaining", 604800))
    return 604800


def get_eligible_pool_accounts(
    analyzed_quotas: Dict[str, Dict[str, Any]],
    min_quota_pct: float = 5.0
) -> List[Dict[str, Any]]:
    """Returns a list of registered accounts eligible for rotation.
    Requires saved session tokens/credentials and parsed quota data."""
    registered = accounts.load_accounts()
    pool = []

    for email, q in analyzed_quotas.items():
        email_clean = email.strip().lower()
        session_dir = switcher.get_account_session_dir(email_clean)
        has_credentials = (
            os.path.isdir(os.path.join(session_dir, "desktop"))
            or os.path.isfile(os.path.join(session_dir, "keyring_token.json"))
            or os.path.isfile(os.path.join(session_dir, "ide_tokens.json"))
            or bool(registered.get(email_clean, {}).get("refresh_token"))
        )
        if not has_credentials:
            continue

        rem_pct, is_exh = failover.get_account_primary_quota(q)
        reset_secs = get_account_reset_seconds(q)

        is_active = bool(q.get("is_current_desktop_session") or q.get("is_current_ide_session"))

        pool.append({
            "email": email,
            "quota_pct": rem_pct,
            "is_exhausted": is_exh,
            "reset_seconds": reset_secs,
            "is_active": is_active,
            "is_eligible": (rem_pct >= min_quota_pct and not is_exh)
        })

    return pool


def evaluate_watermark_strategy(
    active_acc: Optional[Dict[str, Any]],
    candidates: List[Dict[str, Any]],
    spread_pct: float
) -> Optional[Tuple[Dict[str, Any], str]]:
    """Balanced Leveling: Rotates if another healthy candidate has >= spread_pct more quota than active."""
    if not candidates:
        return None

    # Sort candidates by remaining quota descending
    best_candidate = max(candidates, key=lambda x: x["quota_pct"])
    active_pct = active_acc["quota_pct"] if active_acc else 0.0

    spread = best_candidate["quota_pct"] - active_pct
    if spread >= spread_pct:
        reason = f"Quota spread of {spread:.1f}% exceeds watermark threshold ({spread_pct:.1f}%): candidate has {best_candidate['quota_pct']:.1f}% vs active {active_pct:.1f}%"
        return best_candidate, reason

    return None


def evaluate_expiry_first_strategy(
    active_acc: Optional[Dict[str, Any]],
    candidates: List[Dict[str, Any]],
    expiry_window_hours: int,
    min_quota_pct: float
) -> Optional[Tuple[Dict[str, Any], str]]:
    """Reset Optimizer: Prioritizes burning accounts whose weekly quota resets soonest within expiry_window."""
    if not candidates:
        return None

    window_secs = expiry_window_hours * 3600
    expiring_soon = [
        c for c in candidates
        if c["reset_seconds"] <= window_secs and c["quota_pct"] >= min_quota_pct
    ]

    if expiring_soon:
        # Pick the one resetting earliest
        earliest = min(expiring_soon, key=lambda x: x["reset_seconds"])
        hours_left = earliest["reset_seconds"] / 3600.0
        reason = f"Quota resets in {hours_left:.1f}h with {earliest['quota_pct']:.1f}% remaining (prioritizing before weekly reset)"
        return earliest, reason

    # If no candidates are in the immediate reset window, fall back to healthiest candidate if active is low
    if active_acc and active_acc["quota_pct"] <= min_quota_pct:
        healthiest = max(candidates, key=lambda x: x["quota_pct"])
        reason = f"Active account is low ({active_acc['quota_pct']:.1f}%); rotating to healthiest account ({healthiest['quota_pct']:.1f}%)"
        return healthiest, reason

    return None


def evaluate_round_robin_strategy(
    active_acc: Optional[Dict[str, Any]],
    pool: List[Dict[str, Any]],
    interval_minutes: int,
    last_rotation_ts: float,
    now: float
) -> Optional[Tuple[Dict[str, Any], str]]:
    """Round-Robin: Rotates in circular order across eligible accounts after interval_minutes."""
    eligible_candidates = [acc for acc in pool if acc["is_eligible"]]
    if len(eligible_candidates) < 2:
        return None

    time_elapsed = now - last_rotation_ts
    interval_secs = interval_minutes * 60

    if time_elapsed < interval_secs:
        return None

    # Find active account in eligible pool
    current_email = active_acc["email"].lower() if active_acc else ""
    current_idx = -1
    for i, acc in enumerate(eligible_candidates):
        if acc["email"].lower() == current_email:
            current_idx = i
            break

    next_idx = (current_idx + 1) % len(eligible_candidates)
    next_acc = eligible_candidates[next_idx]

    if next_acc["email"].lower() == current_email:
        return None

    elapsed_min = int(time_elapsed // 60)
    reason = f"Round-robin rotation window reached ({elapsed_min}m elapsed >= {interval_minutes}m target)"
    return next_acc, reason


def evaluate_and_execute_balancer(
    analyzed_quotas: Dict[str, Dict[str, Any]],
    dry_run: bool = False,
    force: bool = False,
    strategy_override: Optional[str] = None
) -> Dict[str, Any]:
    """Evaluates the configured pool rotation policy and switches accounts if beneficial."""
    cfg = load_balancer_config()
    state = load_balancer_state()
    now = time.time()

    is_enabled = cfg.get("enabled", False)
    if not is_enabled and not force:
        return {
            "triggered": False,
            "reason": "BALANCER_DISABLED",
            "message": "Auto-balancer is disabled. Enable via 'agy-token balance on'."
        }

    strategy = strategy_override or cfg.get("strategy", "watermark")
    if strategy not in SUPPORTED_STRATEGIES:
        strategy = "watermark"

    if strategy == "reactive":
        return {
            "triggered": False,
            "reason": "STRATEGY_REACTIVE",
            "message": "Balancer strategy is set to 'reactive' (emergency failover handles depletion)."
        }

    # Anti-flapping cooldown
    cooldown = cfg.get("cooldown_seconds", 300)
    last_ts = state.get("last_rotation_timestamp", 0.0)
    time_since = now - last_ts
    if time_since < cooldown and not force:
        remaining_cd = int(cooldown - time_since)
        return {
            "triggered": False,
            "reason": "COOLDOWN",
            "cooldown_remaining": remaining_cd,
            "message": f"Rotation cooldown active ({remaining_cd}s remaining). Skipping rotation."
        }

    # Discover pool & active account
    min_quota = cfg.get("min_quota_pct", 5.0)
    pool = get_eligible_pool_accounts(analyzed_quotas, min_quota_pct=min_quota)

    if len(pool) < 2:
        return {
            "triggered": False,
            "reason": "INSUFFICIENT_POOL",
            "message": f"Auto-balancer requires at least 2 registered switchable accounts (found {len(pool)})."
        }

    active_acc = next((acc for acc in pool if acc["is_active"]), None)
    candidates = [acc for acc in pool if acc["is_eligible"] and not acc["is_active"]]

    rotation_decision = None

    if strategy == "watermark":
        spread = cfg.get("watermark_spread_pct", 20.0)
        rotation_decision = evaluate_watermark_strategy(active_acc, candidates, spread)
    elif strategy == "expiry_first":
        window_hrs = cfg.get("expiry_window_hours", 36)
        rotation_decision = evaluate_expiry_first_strategy(active_acc, candidates, window_hrs, min_quota)
    elif strategy == "round_robin":
        interval_min = cfg.get("round_robin_interval_minutes", 60)
        rotation_decision = evaluate_round_robin_strategy(active_acc, pool, interval_min, last_ts, now)

    if not rotation_decision:
        return {
            "triggered": False,
            "reason": "OPTIMAL",
            "strategy": strategy,
            "active_email": active_acc["email"] if active_acc else None,
            "message": f"Account pool is balanced under '{strategy}' strategy. No rotation needed."
        }

    target_acc, reason_msg = rotation_decision
    target_email = target_acc["email"]
    target_pct = target_acc["quota_pct"]
    from_email = active_acc["email"] if active_acc else "None"
    from_pct = active_acc["quota_pct"] if active_acc else 0.0

    if dry_run:
        return {
            "triggered": True,
            "dry_run": True,
            "strategy": strategy,
            "from_account": from_email,
            "from_quota_pct": from_pct,
            "to_account": target_email,
            "to_quota_pct": target_pct,
            "reason": reason_msg,
            "message": f"[DRY RUN] Would rotate from {from_email} ({from_pct:.1f}%) to {target_email} ({target_pct:.1f}%): {reason_msg}"
        }

    # Execute Rotation
    auto_restart = cfg.get("auto_restart", True)
    notifier.send_desktop_notification(
        f"🔄 Auto-Balancer ({strategy.replace('_', ' ').title()})",
        f"Rotating session to {target_email} ({target_pct:.1f}% available).\nReason: {reason_msg}",
        urgency="normal"
    )

    success, switch_msg = switcher.switch_to_account(target_email, restart=auto_restart)

    if success:
        state["last_rotation_timestamp"] = now
        state["last_from_email"] = from_email
        state["last_to_email"] = target_email
        state["last_strategy_used"] = strategy
        history = state.get("history", [])
        history.append({
            "timestamp": now,
            "strategy": strategy,
            "from": from_email,
            "to": target_email,
            "from_pct": round(from_pct, 1),
            "to_pct": round(target_pct, 1),
            "reason": reason_msg
        })
        state["history"] = history[-50:]
        save_balancer_state(state)

        notifier.send_desktop_notification(
            "✓ Auto-Balancer Rotation Complete",
            f"Active Antigravity account is now {target_email} ({target_pct:.1f}% quota).",
            urgency="normal"
        )

        return {
            "triggered": True,
            "strategy": strategy,
            "from_account": from_email,
            "to_account": target_email,
            "to_quota_pct": target_pct,
            "reason": reason_msg,
            "message": f"Successfully rotated to {target_email} ({target_pct:.1f}% quota): {reason_msg}"
        }
    else:
        notifier.send_desktop_notification(
            "❌ Auto-Balancer Rotation Failed",
            f"Could not switch to {target_email}: {switch_msg}",
            urgency="critical"
        )
        return {
            "triggered": False,
            "reason": "SWITCH_FAILED",
            "error": switch_msg,
            "message": f"Rotation switch failed: {switch_msg}"
        }
