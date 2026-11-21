# System Tray Applet (`src/tray`)

## Overview
Provides the Linux desktop top-panel StatusNotifierItem (SNI) system tray applet using D-Bus, rendering dynamic quota progress icons, single-instance lock management, and interactive dropdown menus.

## Interface
- `pub async fn run_tray(interval_secs: u64, lock_path: Option<&Path>) -> Result<(), Box<dyn std::error::Error + Send + Sync>>`: Initializes and runs the singleton tray event loop `[src/tray/mod.rs:run_tray]`
- `pub struct TrayState`: Shared state model storing minimum quota, exhaustion status, active account summary, egress geo info, and toggle states `[src/tray/applet.rs:TrayState]`
- `pub struct AntigravityTray`: KSNI tray implementation providing menus, tooltip, title, and action handlers `[src/tray/applet.rs:AntigravityTray]`
- `pub fn render_tray_icon(pct: f64, is_exhausted: bool) -> (ksni::Icon, String)`: Procedurally renders a 48x48 ARGB32 circular quota meter icon `[src/tray/icon.rs:render_tray_icon]`
- `pub struct TrayLock`: RAII singleton lock guard ensuring only one tray instance executes concurrently `[src/tray/lock.rs:TrayLock]`
- `pub fn acquire(custom_path: Option<&Path>) -> Result<Self, Option<u32>>`: Attempts to acquire an exclusive file lock `[src/tray/lock.rs:TrayLock::acquire]`

## Invariants
- Acquires exclusive file lock via `fs2::try_lock_exclusive()`; returns `Err(Some(existing_pid))` if locked by another instance `[src/tray/lock.rs:TrayLock::acquire]`
- Truncates lock file to 0 bytes and seeks to start before writing current process ID `[src/tray/lock.rs:TrayLock::acquire]`
- File lock is released and lock file is removed from disk on drop `[src/tray/lock.rs:TrayLock::drop]`
- Fixed 48x48 pixel resolution with ARGB32 buffer and ring radius between 17.0 and 21.0 from center (24.0, 24.0) `[src/tray/icon.rs:render_tray_icon]`
- Color grading: Red (234, 67, 53) if `is_exhausted` or `pct <= 10.0`; Yellow (251, 188, 4) if `pct <= 25.0`; Blue (66, 133, 244) if `pct <= 50.0`; Green (52, 168, 83) otherwise `[src/tray/icon.rs:render_tray_icon]`
- Fill fraction is 0.0 if `is_exhausted` is true, else clamped between 0.0 and 1.0 via `(pct / 100.0).clamp(0.0, 1.0)` `[src/tray/icon.rs:render_tray_icon]`
- Icon name is `'quota_exhausted'` if `is_exhausted` is true, else `'quota_{pct.round() as i64}'` `[src/tray/icon.rs:render_tray_icon]`
- D-Bus tray identifier returns constant `'antigravity-token-tracker'` `[src/tray/applet.rs:AntigravityTray::id]`
- Title returns `'Antigravity: Quota Exhausted'` if `state.is_exhausted`, else `'Antigravity: {min_pct:.1}%'` `[src/tray/applet.rs:AntigravityTray::title]`
- Ensures singleton applet execution via `TrayLock::acquire`; exits early with `Ok(())` if locked `[src/tray/mod.rs:run_tray]`
- Sets `min_pct = 0.0` and `is_exhausted = true` if no active session is detected or `min_pct <= 0.0` `[src/tray/mod.rs:run_tray]`

## Side Effects
- Creates lock file with exclusive flock, writes PID, and deletes file on Drop `[src/tray/lock.rs:TrayLock::acquire]`
- Generates PNG icon files under `~/.config/antigravity-token-tracker/icons` `[src/tray/icon.rs:render_tray_icon]`
- Spawns web browser to `http://localhost:8765` via `open::that` `[src/tray/applet.rs:AntigravityTray::activate]`
- Menu callbacks trigger account switching via `switcher::switch_to_account`, dispatch desktop alerts, mutate shield and balancer configs, and trigger quota refreshes `[src/tray/applet.rs:AntigravityTray::menu]`
- Registers StatusNotifierItem service on session D-Bus and spawns background web server `[src/tray/mod.rs:run_tray]`

## Verification
- Run: `cargo test`
