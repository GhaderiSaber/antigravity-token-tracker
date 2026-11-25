# System Tray Applet (`src/tray`)

## Overview
Provides the Linux desktop top-panel StatusNotifierItem (SNI) system tray applet using D-Bus, rendering dynamic `QuotaIndicator` progress icons, managing single-instance advisory locks, and dispatching non-blocking `TrayCommand` menu actions.

## Interface
- Module exports (`src/tray/mod.rs`): `pub mod applet`, `pub mod icon`, `pub mod lock`, with explicit re-exports of `AntigravityTray`, `TrayCommand`, `TrayState`, `switchable_accounts`, `QuotaIndicator`, `default_icon_cache_dir`, `ensure_cached_icon_png`, `render_tray_icon`, `TrayError`, `TrayLock`, `TrayLockError`, and `default_lock_path` `[src/tray/mod.rs]`
- `pub struct TrayStateInput<'a>` / `pub fn build_tray_state_from_data(input: TrayStateInput<'_>) -> TrayState`: Builds a `TrayState` snapshot from quotas, accounts, egress geo, shield/balancer settings, backup candidate, and icon cache directory `[src/tray/mod.rs:build_tray_state_from_data]`
- `pub async fn run_tray(interval_secs: u64, lock_path: Option<&Path>, mut shutdown: Option<tokio::sync::watch::Receiver<bool>>) -> Result<(), TrayError>`: Initializes and runs the singleton tray event loop until `SIGINT`, `SIGTERM`, `shutdown` watch `true`, or `TrayCommand::Quit` `[src/tray/mod.rs:run_tray]`
- `pub enum TrayCommand`: Non-blocking commands emitted by menu callbacks (`RefreshQuotas`, `ToggleShield`, `ToggleBalancer`, `SwitchAccount(String)`, `OpenDashboard`, `Quit`) `[src/tray/applet.rs:TrayCommand]`
- `pub struct TrayState`: Shared state model storing `indicator: QuotaIndicator`, `active_email`, `active_summary`, `icon_name`, `icon_data`, `icon_width`, `icon_height`, `accounts`, `geo_summary`, `is_restricted`, `is_shield_enabled`, `is_balancer_enabled`, `balancer_strategy`, and `backup_candidate` `[src/tray/applet.rs:TrayState]`
- `pub fn switchable_accounts<'a>(accounts: &'a [AccountProfile], active_email: Option<&str>) -> Vec<&'a AccountProfile>`: Filters registered accounts eligible for manual switching `[src/tray/applet.rs:switchable_accounts]`
- `pub struct AntigravityTray`: `ksni::Tray` implementation holding `state: Arc<Mutex<TrayState>>` and `cmd_tx: UnboundedSender<TrayCommand>` `[src/tray/applet.rs:AntigravityTray]`
- `pub enum QuotaIndicator`: `Available(f64)` | `Exhausted` with `from_percent(pct: f64)`, `icon_name(self) -> String`, `title_text(self) -> String`, and `is_exhausted(self) -> bool` `[src/tray/icon.rs:QuotaIndicator]`
- `pub fn default_icon_cache_dir() -> PathBuf`: Resolves `dirs::config_dir()` joined with `antigravity-token-tracker/icons` `[src/tray/icon.rs:default_icon_cache_dir]`
- `pub fn render_tray_icon(indicator: QuotaIndicator) -> (Icon, String)`: Purely renders a 48x48 ARGB32 circular quota meter icon with center lightning bolt without disk I/O `[src/tray/icon.rs:render_tray_icon]`
- `pub fn ensure_cached_icon_png(indicator: QuotaIndicator, cache_dir: &Path) -> io::Result<PathBuf>`: Writes cached PNG icon to `cache_dir` only on cache miss `[src/tray/icon.rs:ensure_cached_icon_png]`
- `pub enum TrayError`: Top-level tray error enum (`Lock(#[from] TrayLockError)`, `Dbus(#[from] ksni::Error)`, `Io(#[from] io::Error)`) `[src/tray/lock.rs:TrayError]`
- `pub enum TrayLockError`: Lock error enum with `Io(#[from] io::Error)` and `AlreadyRunning(Option<u32>)` `[src/tray/lock.rs:TrayLockError]`
- `pub fn default_lock_path() -> PathBuf`: Resolves `dirs::config_dir()` joined with `antigravity-token-tracker/tray.lock` `[src/tray/lock.rs:default_lock_path]`
- `pub struct TrayLock`: RAII singleton lock guard exposing `pub path: PathBuf` and `pub fn acquire(custom_path: Option<&Path>) -> Result<Self, TrayLockError>` `[src/tray/lock.rs:TrayLock]`

## Invariants
- `TrayLock::acquire` uses `fs2::FileExt::try_lock_exclusive()`, returning `Err(TrayLockError::AlreadyRunning(existing_pid))` if locked by another instance, and truncates/rewrites PID before returning; `Drop` calls `FileExt::unlock(&self._file)` without unlinking the lockfile to avoid inode races `[src/tray/lock.rs:TrayLock::acquire, src/tray/lock.rs:TrayLock::drop]`
- Fixed 48x48 pixel resolution with ARGB32 buffer, white center lightning bolt polygon, and ring track radius `15.0..=22.0` from center `(24.0, 24.0)` `[src/tray/icon.rs:render_tray_icon]`
- Color grading: Red `(234, 67, 53)` if `is_exhausted` or `pct <= 10.0`; Yellow `(251, 188, 4)` if `pct <= 25.0`; Blue `(66, 133, 244)` if `pct <= 50.0`; Green `(52, 168, 83)` otherwise; `QuotaIndicator::from_percent` maps non-positive or non-finite values to `Exhausted` and clamps positive values to `0.0..=100.0` `[src/tray/icon.rs:QuotaIndicator::from_percent, src/tray/icon.rs:render_tray_icon]`
- Icon name is `'quota_exhausted'` for `QuotaIndicator::Exhausted` or `'quota_{pct.round() as i64}'` for `QuotaIndicator::Available(pct)`; title is `'Antigravity: Quota Exhausted'` or `'Antigravity: {pct:.1}%'` `[src/tray/icon.rs:QuotaIndicator::icon_name, src/tray/icon.rs:QuotaIndicator::title_text]`
- D-Bus tray identifier returns `'antigravity-token-tracker'`; synchronous `ksni::Tray` callbacks (`activate`, `menu`) enqueue `TrayCommand` via `cmd_tx` without blocking `[src/tray/applet.rs:AntigravityTray]`
- `switchable_accounts` excludes `active_email` (case-insensitive trimmed) and accounts with `is_current_desktop_session == Some(true)` or `is_current_ide_session == Some(true)`, ignoring legacy `active == Some(true)` `[src/tray/applet.rs:switchable_accounts]`
- `run_tray` exits early with `Ok(())` on `TrayLockError::AlreadyRunning`, returns `Err(TrayError::Lock(_))` on lock I/O errors, calls `service.shutdown().await` on clean exit (`SIGINT`, `SIGTERM`, `shutdown`, or `TrayCommand::Quit`), and maps missing active sessions to `QuotaIndicator::Exhausted` / `"No active session detected"` `[src/tray/mod.rs:run_tray, src/tray/mod.rs:select_active_session_summary]`
- `refresh_tray_state` populates `TrayState::backup_candidate` via `find_best_failover_candidate(&q_map, active_email.as_deref(), &exclude, 5.0)` `[src/tray/mod.rs:refresh_tray_state]`

## Side Effects
- Creates lock file with exclusive flock, writes PID, and unlocks without unlinking on `Drop` `[src/tray/lock.rs:TrayLock::acquire]`
- Creates icon cache directory and writes PNG icon files on cache miss under `<config_dir>/antigravity-token-tracker/icons` `[src/tray/icon.rs:ensure_cached_icon_png]`
- Opens `http://localhost:8765` in the default web browser via `open::that` on `TrayCommand::OpenDashboard` `[src/tray/mod.rs:handle_tray_command]`
- Async command handlers switch accounts via `switch_to_account`, dispatch desktop notifications, mutate shield/balancer configs on disk, and trigger forced quota refreshes `[src/tray/mod.rs:handle_tray_command]`
- Spawns background web server on port `8765`, registers StatusNotifierItem service on session D-Bus (`assume_sni_available(true)`), and periodically syncs auth, fetches quotas, checks IP geo, and enforces shield policy `[src/tray/mod.rs:run_tray]`

## Verification
- Run: `cargo test`
