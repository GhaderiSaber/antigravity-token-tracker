# Quota Balancer & Session Switcher (`src/balancer`)

## Overview
Coordinates multi-account quota balancing algorithms (watermark spread, expiry-first, round-robin), session credential isolation, desktop/IDE session snapshotting, and atomic hot-swapping between accounts.

## Interface
- `pub struct BalancerConfig`: Configuration model for balancing strategy, spread thresholds, intervals, and cooldowns `[src/balancer/models.rs:BalancerConfig]`
- `pub struct BalancerState`: Persistent state tracking rotation counts, timestamps, and round-robin indexes `[src/balancer/models.rs:BalancerState]`
- `pub struct PoolAccount`: Account representation within the balancing candidate pool `[src/balancer/balancer.rs:PoolAccount]`
- `pub struct BalancerDecision`: Decision payload recording evaluation reasons, metrics, and switch targets `[src/balancer/balancer.rs:BalancerDecision]`
- `pub fn evaluate_watermark_strategy(active_acc: Option<&PoolAccount>, candidates: &[PoolAccount], spread_pct: f64) -> Option<(PoolAccount, String)>`: Evaluates whether quota spread between active and candidates exceeds threshold `[src/balancer/balancer.rs:evaluate_watermark_strategy]`
- `pub fn evaluate_expiry_first_strategy(active_acc: Option<&PoolAccount>, candidates: &[PoolAccount], expiry_window_hours: u64, min_quota_pct: f64) -> Option<(PoolAccount, String)>`: Prioritizes accounts whose weekly quota expires soonest `[src/balancer/balancer.rs:evaluate_expiry_first_strategy]`
- `pub fn evaluate_round_robin_strategy(active_acc: Option<&PoolAccount>, pool: &[PoolAccount], interval_minutes: u64, last_rotation_ts: f64, now: f64) -> Option<(PoolAccount, String)>`: Rotates accounts on a fixed schedule `[src/balancer/balancer.rs:evaluate_round_robin_strategy]`
- `pub async fn evaluate_and_execute_balancer(analyzed_quotas: &HashMap<String, AccountQuotaResult>, dry_run: bool, force: bool, strategy_override: Option<&str>) -> BalancerDecision`: Main orchestrator executing balancing policies and rotations `[src/balancer/balancer.rs:evaluate_and_execute_balancer]`
- `pub async fn snapshot_account(email: &str) -> (bool, bool)`: Snapshots active Desktop App and IDE sessions for an account `[src/balancer/switcher.rs:snapshot_account]`
- `pub async fn switch_to_account(target_email: &str, restart: bool, surface: &str) -> (bool, String)`: Switches active session credentials across desktop, IDE, or both `[src/balancer/switcher.rs:switch_to_account]`
- `pub fn restart_antigravity() -> bool`: Gracefully restarts running Antigravity processes `[src/balancer/switcher.rs:restart_antigravity]`

## Invariants
- `evaluate_watermark_strategy` triggers rotation only when spread between highest candidate quota and active quota is >= `spread_pct` `[src/balancer/balancer.rs:evaluate_watermark_strategy]`
- `evaluate_expiry_first_strategy` prioritizes candidates with `reset_seconds <= expiry_window_hours * 3600` and `quota_pct >= min_quota_pct`, or rotates if active `quota_pct <= min_quota_pct` `[src/balancer/balancer.rs:evaluate_expiry_first_strategy]`
- `evaluate_round_robin_strategy` requires at least 2 eligible accounts, elapsed time >= `interval_minutes * 60`, and advances sequentially `[src/balancer/balancer.rs:evaluate_round_robin_strategy]`
- `evaluate_and_execute_balancer` skips rotation if balancer is disabled unless `force` is true; enforces cooldown (`now - last_rotation_timestamp >= cooldown_seconds`) unless `force` is true; requires eligible candidate pool >= 2 `[src/balancer/balancer.rs:evaluate_and_execute_balancer]`
- `get_account_reset_seconds` defaults to 604800 (7 days) if weekly countdown is absent `[src/balancer/balancer.rs:get_account_reset_seconds]`
- `get_eligible_pool_accounts` marks `is_eligible = true` only if `quota_pct >= min_quota_pct` and `!is_exhausted` `[src/balancer/balancer.rs:get_eligible_pool_accounts]`
- `ensure_sessions_dir` and `get_account_session_dir` explicitly enforce POSIX permission mode 0o700 on Unix `[src/balancer/switcher.rs:ensure_sessions_dir]`
- `snapshot_ide_session` rejects snapshot if IDE DB refresh token is mapped to another registered account or email does not match `[src/balancer/switcher.rs:snapshot_ide_session]`
- `switch_to_account` rejects switch if target has no credentials; returns early if target is already active; always invokes `snapshot_account` on current active account before switching `[src/balancer/switcher.rs:switch_to_account]`

## Side Effects
- Creates directories and writes serialized configuration and state files to `balancer_config.json` and `balancer_state.json` `[src/balancer/balancer.rs:save_balancer_config]`
- Interacts with system Secret Service over D-Bus (`org.freedesktop.secrets`) to set or clear keyring tokens `[src/balancer/switcher.rs:set_keyring_secret]`
- Copies configuration files, directories, and session metadata to `~/.config/antigravity-token-tracker/sessions/` `[src/balancer/switcher.rs:snapshot_desktop_session]`
- Executes SQLite `INSERT OR REPLACE` statements into IDE `state.vscdb` `[src/balancer/switcher.rs:restore_ide_session]`
- Sends POSIX signals (SIGTERM, SIGKILL) to Antigravity OS processes and deletes Singleton locks `[src/balancer/switcher.rs:stop_antigravity]`
- Spawns detached OS process for Antigravity executable `[src/balancer/switcher.rs:launch_antigravity]`

## Verification
- Run: `cargo test`
