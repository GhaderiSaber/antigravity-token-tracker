# Daemon & Auto-Failover Service (`src/daemon`)

## Overview
Provides continuous background quota monitoring, auto-failover execution, system health diagnostic audits (`doctor`), and native desktop notifications (`notify-send` / D-Bus).

## Interface
- `pub struct FailoverState`: Persistent state tracking failover events, timestamps, and active account history `[src/daemon/models.rs:FailoverState]`
- `pub struct FailoverDecision`: Decision report recording the result of failover evaluations `[src/daemon/models.rs:FailoverDecision]`
- `pub struct DoctorReport`: Health report aggregating credentials, process, network, and keyring checks `[src/daemon/models.rs:DoctorReport]`
- `pub async fn send_desktop_notification(title: &str, message: &str, urgency: &str) -> bool`: Dispatches desktop alerts via D-Bus or `notify-send` `[src/daemon/notifier.rs:send_desktop_notification]`
- `pub async fn run_doctor_suite(config_dir: &Path, validate_network: bool) -> DoctorReport`: Executes system environment diagnostic checks `[src/daemon/doctor.rs:run_doctor_suite]`
- `pub fn find_best_failover_candidate(all_quotas: &HashMap<String, AccountQuotaResult>, current_email: Option<&str>, min_threshold_pct: f64, excluded_emails: Option<&[String]>) -> Option<(String, f64)>`: Finds the healthiest candidate account for failover `[src/daemon/failover.rs:find_best_failover_candidate]`
- `pub async fn evaluate_and_execute_failover(all_quotas: &HashMap<String, AccountQuotaResult>, active_email: Option<&str>, threshold: f64, cooldown_seconds: f64, restart: bool, dry_run: bool) -> FailoverDecision`: Evaluates active session exhaustion and executes automatic failover `[src/daemon/failover.rs:evaluate_and_execute_failover]`
- `pub async fn run_daemon_step(interval_secs: u64, auto_switch: bool, threshold: f64, cooldown: f64)`: Executes a single iteration of the daemon service loop `[src/daemon/daemon.rs:run_daemon_step]`

## Invariants
- Urgency string is mapped to integer priority: `'low'` -> 0, `'critical'` -> 2, any other value defaults to 1 `[src/daemon/notifier.rs:send_desktop_notification]`
- Doctor overall status is evaluated hierarchically: `'FAIL'` if any check failed, `'WARN'` if any check warned without failures, and `'PASS'` only when all checks pass `[src/daemon/doctor.rs:run_doctor_suite]`
- Prioritizes weekly quota groups matching case-insensitive `'gemini'` over other groups, falling back to the first weekly group, and flags exhausted if remaining percentage `<= 1.0` or if `is_exhausted` is true `[src/daemon/failover.rs:get_account_primary_quota]`
- Active session detection prioritizes an active desktop session over an active IDE session `[src/daemon/failover.rs:get_active_account]`
- Filters candidate accounts to registered accounts with valid credentials, excluding current and excluded emails, requiring remaining percentage >= `min_threshold_pct` and not exhausted, sorted descending `[src/daemon/failover.rs:find_best_failover_candidate]`
- Failover is skipped with `'HEALTHY'` if active quota > threshold and not exhausted, or with `'COOLDOWN'` if elapsed time since last failover is less than `cooldown_seconds` `[src/daemon/failover.rs:evaluate_and_execute_failover]`
- Candidate search threshold is clamped to a minimum of 5.0% via `threshold.max(5.0)` `[src/daemon/failover.rs:evaluate_and_execute_failover]`
- Failover event history retains a maximum of 50 events, pruning oldest records beyond 50 `[src/daemon/failover.rs:evaluate_and_execute_failover]`
- Failover evaluation is invoked with fixed parameters: threshold 1.0, restart true, cooldown 120.0s, dry_run false `[src/daemon/daemon.rs:run_daemon_step]`

## Side Effects
- Calls D-Bus method `Notify` on `org.freedesktop.Notifications` with fallback to `notify-send` binary execution `[src/daemon/notifier.rs:send_desktop_notification]`
- Outbound HTTP HEAD network requests to `https://daily-cloudcode-pa.googleapis.com` and `https://ipwho.is` `[src/daemon/doctor.rs:run_doctor_suite]`
- Establishes D-Bus connection to query `org.freedesktop.secrets` `[src/daemon/doctor.rs:run_doctor_suite]`
- Scans system processes and reads account storage files `[src/daemon/doctor.rs:run_doctor_suite]`
- Reads and writes failover state JSON at `default_failover_state_file` `[src/daemon/failover.rs:load_failover_state]`
- Triggers desktop alerts on exhaustion, switch, completion, and failure `[src/daemon/failover.rs:evaluate_and_execute_failover]`
- Switches credentials and terminates/restarts Antigravity processes via `switch_to_account` `[src/daemon/failover.rs:evaluate_and_execute_failover]`
- Appends timestamped quota snapshot to `token_history.jsonl` `[src/daemon/daemon.rs:record_token_history_snapshot]`

## Verification
- Run: `cargo test`
