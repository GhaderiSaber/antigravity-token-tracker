# Daemon & Auto-Failover Service (`src/daemon`)

## Overview
Provides continuous background quota monitoring, auto-failover execution, system health diagnostic audits (`doctor`), token history snapshot logging, SSE/watch-channel tick publishing, and native desktop notifications (`org.freedesktop.Notifications` / `notify-send`).

## Interface
- Module exports (`src/daemon/mod.rs`): `pub mod daemon`, `pub mod doctor`, `pub mod failover`, `pub mod models`, `pub mod notifier`, with explicit re-exports of all public daemon, doctor, failover, model, and notifier types/functions `[src/daemon/mod.rs]`
- `pub struct FailoverState`: Persistent state tracking failover events, timestamps, and active account history `[src/daemon/models.rs:FailoverState]`
- `pub struct FailoverEvent`: Recorded failover transition event (`timestamp`, `from`, `to`, `from_pct`, `to_pct`, `reason`) `[src/daemon/models.rs:FailoverEvent]`
- `pub struct FailoverDecision`: Decision report recording the result of failover evaluations `[src/daemon/models.rs:FailoverDecision]`
- `pub enum DiagnosticStatus`: Diagnostic check status (`Pass`, `Warn`, `Fail`, `Info`) with uppercase serde serialization and predicates `is_pass(self) -> bool`, `is_warn(self) -> bool`, `is_fail(self) -> bool`, `is_info(self) -> bool` `[src/daemon/models.rs:DiagnosticStatus]`
- `pub struct DiagnosticItem`: Individual component health check item (`component`, `status`, `details`) `[src/daemon/models.rs:DiagnosticItem]`
- `pub struct DoctorReport`: Health report aggregating `items`, `overall_status`, and `timestamp` `[src/daemon/models.rs:DoctorReport]`
- `pub struct DaemonTickSnapshot`: Tick snapshot containing `quotas: HashMap<String, AccountQuotaResult>`, `geo: EgressGeo`, `balancer_triggered: bool`, and `failover_triggered: bool` `[src/daemon/models.rs:DaemonTickSnapshot]`
- `pub async fn send_desktop_notification_with_actions(summary: &str, body: &str, urgency: &str, actions: &[(&str, &str)]) -> Option<u32>`: Dispatches D-Bus notifications with interactive action buttons `[src/daemon/notifier.rs:send_desktop_notification_with_actions]`
- `pub async fn send_desktop_notification(summary: &str, body: &str, urgency: &str) -> bool`: Dispatches desktop alerts via D-Bus or `notify-send` fallback `[src/daemon/notifier.rs:send_desktop_notification]`
- `pub fn spawn_notification_action_listener(on_action: impl Fn(String) + Send + Sync + 'static) -> tokio::task::JoinHandle<()>`: Spawns a background task listening for `org.freedesktop.Notifications` `ActionInvoked` D-Bus signals `[src/daemon/notifier.rs:spawn_notification_action_listener]`
- `pub fn build_geo_shield_diagnostic(cfg: &ShieldConfig, geo: &EgressGeo) -> DiagnosticItem` / `pub fn compute_overall_status(items: &[DiagnosticItem]) -> DiagnosticStatus` / `pub async fn run_doctor_suite() -> DoctorReport`: Evaluates Geo-Shield, aggregates diagnostic items, and executes system environment health checks `[src/daemon/doctor.rs:run_doctor_suite]`
- `pub fn default_failover_state_file() -> PathBuf`: Returns the default path to `<config_dir>/antigravity-token-tracker/failover_state.json` `[src/daemon/failover.rs:default_failover_state_file]`
- `pub fn load_failover_state() -> FailoverState` / `pub fn save_failover_state(state: &FailoverState) -> std::io::Result<()>`: Loads and persists failover state JSON `[src/daemon/failover.rs:load_failover_state]`
- `pub fn account_primary_quota(q: &AccountQuotaResult) -> (f64, bool)` / `get_account_primary_quota`: Extracts primary weekly quota percentage and exhaustion flag `[src/daemon/failover.rs:account_primary_quota]`
- `pub fn active_sessions(analyzed_quotas: &HashMap<String, AccountQuotaResult>) -> HashMap<String, (String, AccountQuotaResult)>` / `get_active_sessions`: Maps `"desktop"` and `"ide"` keys to active session entries `[src/daemon/failover.rs:active_sessions]`
- `pub fn active_account(analyzed_quotas: &HashMap<String, AccountQuotaResult>) -> Option<(String, AccountQuotaResult)>` / `get_active_account`: Returns the primary active account `[src/daemon/failover.rs:active_account]`
- `pub fn find_best_failover_candidate(analyzed_quotas: &HashMap<String, AccountQuotaResult>, current_email: Option<&str>, exclude_emails: &[String], min_threshold_pct: f64) -> Option<(String, f64)>`: Finds the healthiest candidate account for failover `[src/daemon/failover.rs:find_best_failover_candidate]`
- `pub fn failover_status(analyzed_quotas: &HashMap<String, AccountQuotaResult>) -> serde_json::Value` / `get_failover_status`: Builds JSON summary of active quota, backup candidate, and last failover `[src/daemon/failover.rs:failover_status]`
- `pub async fn evaluate_and_execute_failover(analyzed_quotas: &HashMap<String, AccountQuotaResult>, threshold: f64, restart: bool, cooldown_seconds: f64, dry_run: bool) -> FailoverDecision`: Evaluates active session exhaustion and executes automatic failover `[src/daemon/failover.rs:evaluate_and_execute_failover]`
- `pub fn default_token_history_file() -> PathBuf`: Returns default path to `~/.gemini/antigravity/token_history.jsonl` `[src/daemon/daemon.rs:default_token_history_file]`
- `pub fn record_token_history_snapshot(analyzed_quotas: &HashMap<String, AccountQuotaResult>, history_file: Option<&Path>) -> std::io::Result<()>`: Appends a JSONL quota snapshot `[src/daemon/daemon.rs:record_token_history_snapshot]`
- `pub enum FailoverRestart`: `Restart` | `NoRestart` with `should_restart(self) -> bool` `[src/daemon/daemon.rs:FailoverRestart]`
- `pub struct DaemonRunOptions`: Configuration struct (`interval_secs`, `auto_switch`, `threshold`, `cooldown_seconds`, `restart`) with `DaemonRunOptions::new()` and `Default` (`60s`, `false`, `1.0%`, `120.0s`, `Restart`) `[src/daemon/daemon.rs:DaemonRunOptions]`
- `pub enum DaemonError`: Typed daemon error (`Io(#[from] std::io::Error)`) `[src/daemon/daemon.rs:DaemonError]`
- `pub async fn run_daemon_step_with_snapshot(storage: &StorageManager, history_file: Option<&Path>, options: &DaemonRunOptions, snapshot_tx: Option<&tokio::sync::watch::Sender<Option<DaemonTickSnapshot>>>) -> (bool, bool)`: Executes one daemon iteration, broadcasts SSE events, and optionally publishes a `DaemonTickSnapshot` `[src/daemon/daemon.rs:run_daemon_step_with_snapshot]`
- `pub async fn run_daemon_step_with_options(storage: &StorageManager, history_file: Option<&Path>, options: &DaemonRunOptions) -> (bool, bool)` / `pub async fn run_daemon_step(storage: &StorageManager, history_file: Option<&Path>) -> (bool, bool)`: Convenience wrappers around `run_daemon_step_with_snapshot` `[src/daemon/daemon.rs:run_daemon_step]`
- `pub async fn run_daemon_with_snapshot(options: DaemonRunOptions, mut shutdown: Option<tokio::sync::watch::Receiver<bool>>, snapshot_tx: Option<tokio::sync::watch::Sender<Option<DaemonTickSnapshot>>>) -> Result<(), DaemonError>` / `run_daemon_with_options` / `run_daemon`: Runs the periodic daemon loop until `shutdown` signals `true` `[src/daemon/daemon.rs:run_daemon_with_snapshot]`

## Invariants
- Urgency string is mapped to integer priority: `"low"` -> `0`, `"critical"` -> `2`, any other value defaults to `1` `[src/daemon/notifier.rs:send_desktop_notification_with_actions]`
- `DoctorReport::overall_status` is `DiagnosticStatus::Fail` if `has_fail` is set (e.g., `accounts.json` read failure or Geo-Shield policy violation `DiagnosticStatus::Fail`), `DiagnosticStatus::Warn` if any check warned without failure, and `DiagnosticStatus::Pass` otherwise `[src/daemon/doctor.rs:run_doctor_suite]`
- Primary quota prioritizes weekly quota groups matching case-insensitive `"gemini"` over other groups, falling back to the first weekly group; `is_exhausted` is true when `final_pct <= 1.0` or the `"gemini"` group's `weekly.is_exhausted` is `true` (ignored for non-Gemini fallback) `[src/daemon/failover.rs:account_primary_quota]`
- Active session detection prioritizes an active desktop session over an active IDE session `[src/daemon/failover.rs:active_account]`
- Candidate filtering excludes `current_email` and `exclude_emails` (case-insensitive trimmed), requires `has_account_credentials`, `rem_pct >= min_threshold_pct`, and `!is_exh`, selecting the candidate with maximum `rem_pct` `[src/daemon/failover.rs:find_best_failover_candidate]`
- Failover returns `NO_ACTIVE_SESSION` if no active session is found, `HEALTHY` if `active_pct > threshold && !is_exhausted`, `COOLDOWN` if `now - last_failover_timestamp < cooldown_seconds`, `ALL_EXHAUSTED` (dispatching a critical desktop notification even when `dry_run == true`) if no candidate meets `threshold.max(5.0)`, `EXHAUSTED` (`dry_run: Some(true)`) on dry run, `SUCCESS` on switch completion, or `SWITCH_FAILED` on switch error `[src/daemon/failover.rs:evaluate_and_execute_failover]`
- Failover event history retains a maximum of 50 events, draining oldest records beyond 50 `[src/daemon/failover.rs:evaluate_and_execute_failover]`
- `run_daemon_step_with_snapshot` runs Geo-Shield enforcement, syncs auth, fetches quotas, logs history, evaluates balancer (`evaluate_and_execute_balancer(&all_quotas, false, false, None)`), evaluates failover with `options.threshold`, `options.restart.should_restart()`, and `options.cooldown_seconds`, emits `"quota_updated"` (plus `"balancer_rotated"` / `"failover_triggered"` when triggered) SSE events, and updates `snapshot_tx` if provided `[src/daemon/daemon.rs:run_daemon_step_with_snapshot]`

## Side Effects
- Calls D-Bus method `Notify` on `org.freedesktop.Notifications` (or listens for `ActionInvoked` signals) with fallback to `notify-send` binary execution in `send_desktop_notification` `[src/daemon/notifier.rs:send_desktop_notification]`
- Establishes D-Bus connection to `org.freedesktop.secrets`, sends outbound HTTP HEAD requests to `https://daily-cloudcode-pa.googleapis.com` and `https://ipwho.is`, discovers desktop/IDE processes, reads `accounts.json`, and evaluates Geo-Shield (`load_shield_config`, which creates default `shield.json` if missing, and `get_ip_geo(false)`) `[src/daemon/doctor.rs:run_doctor_suite]`
- Creates missing parent directories and reads/writes failover state JSON at `default_failover_state_file` `[src/daemon/failover.rs:save_failover_state]`
- Triggers desktop alerts on exhaustion, switch, completion, and failure `[src/daemon/failover.rs:evaluate_and_execute_failover]`
- Switches credentials and terminates/restarts Antigravity processes across `SwitchSurface::Both` via `switch_to_account` `[src/daemon/failover.rs:evaluate_and_execute_failover]`
- Creates missing parent directories and appends timestamped quota snapshots to `token_history.jsonl` `[src/daemon/daemon.rs:record_token_history_snapshot]`
- Runs Geo-Shield enforcement, `sync_from_antigravity`, quota refresh, snapshot logging, auto-balancing, failover, SSE broadcast, and `DaemonTickSnapshot` publishing on each tick `[src/daemon/daemon.rs:run_daemon_step_with_snapshot]`

## Verification
- Run: `cargo test`
