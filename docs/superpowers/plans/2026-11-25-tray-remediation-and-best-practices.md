# System Tray Remediation & Rust Best Practices Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all runtime failures preventing the Linux system tray from showing accurate account quotas, interactive menu updates, and `daemon --tray` startup, while aligning `src/tray/` with `rust-best-practices`.

**Architecture:** Fix Antigravity v2.35+ desktop session discovery (`agy.onboarding.lastLoginUsername` and `--read_host_bridge_token_from_stdin`) so active sessions are detected accurately; refactor `src/tray/` around a typed `QuotaIndicator` enum, race-free `TrayLock`, pure icon rendering with cache-miss-only PNG persistence, and a command-channel event loop (`TrayCommand`) supporting immediate menu-driven state refreshes, non-blocking browser launches, and clean `SIGINT`/`SIGTERM` shutdown; wire `--tray` in `agy-token daemon` and remove the systemd/autostart boot race in `install.sh`.

**Tech Stack:** Rust 2024 (`rustc` >= 1.85), Tokio (`signal`, `sync::mpsc`, `task::spawn_blocking`), `ksni` 0.3.7, `image` 0.25, `thiserror` 2.0, `dirs` 5.0, `fs2` 0.4, `tempfile`, `trycmd`.

**Spec:** Audit findings from `/rust-best-practices` tray audit (2026-11-25).

## Global Constraints
- Rust Edition 2024, MSRV 1.85 (`Cargo.toml`).
- Pure Rust implementation: zero GTK/GObject C bindings, zero Python dependencies.
- `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --lib` must pass with 0 warnings/failures.
- All files created or modified on disk must use ASCII-only filenames.
- No `unwrap()` outside tests/main; no `std::process::exit` inside library callbacks; no `Box<dyn Error>` in public library signatures; no glob re-exports (`pub use foo::*;`) in modified modules; no `bool` flags in new/refactored public signatures.

## Review Focus
1. **Antigravity v2.35+ `language_server` without `--csrf_token` (`--read_host_bridge_token_from_stdin`)**: Desktop session discovery must still detect the active desktop account from `agy.onboarding.lastLoginUsername` (or keyring) when `--csrf_token` is absent so the tray does not falsely report `"No active session detected"` / `"Quota Exhausted"`. (Pinned in Task 1.)
2. **Multiple accounts carrying legacy `active: Some(true)` in `accounts.json`**: The `Switch Account` submenu must exclude only the current active session account, not every account with `active == Some(true)`. (Pinned in Task 4.)
3. **Lockfile `unlink` race and clean `SIGTERM` / `Quit` teardown**: `TrayLock::drop` must not unlink the lockfile path, and `Quit` / `SIGTERM` must exit the `run_tray` loop cleanly so `TrayLock::drop` and `ksni::Handle::shutdown` execute. (Pinned in Task 3 & Task 4.)
4. **Menu actions (`ToggleShield`, `ToggleBalancer`, `RefreshQuotas`, `SwitchAccount`) mutating state**: Every interactive menu command must recompute/update `TrayState` and notify `ksni` immediately rather than discarding results or leaving stale labels until the next 60s tick. (Pinned in Task 4.)
5. **Port `8765` conflict & `daemon --tray` flag handling**: A bound port `8765` must log an error without aborting the tray, and `agy-token daemon --tray` must parse `--tray` alongside `--interval`, `--auto-switch`, `--threshold`, `--cooldown`, and `--no-restart`. (Pinned in Task 4 & Task 5.)

---

### Task 1: Fix Antigravity v2.35+ Desktop Session Discovery

**Files:**
- Modify: `src/auth/process_discovery.rs:130-275`
- Modify: `src/auth/sync.rs:11-140`
- Test: `src/auth/mod.rs`

**Interfaces:**
- Produces in `src/auth/process_discovery.rs`:
  - `pub fn is_desktop_language_server_cmdline(cmd: &str) -> bool`
  - `pub fn read_desktop_last_login_username(app_storage_path: &Path) -> Option<String>`
  - `pub fn resolve_desktop_session_email(live_email: &str, keyring_email: Option<&str>, app_storage_email: Option<&str>) -> Option<String>`
- Consumes in `src/auth/sync.rs`: `discover_antigravity_desktop_app() -> Option<DiscoveredSession>`

- [ ] **Step 1: Write failing unit tests in `src/auth/mod.rs`**

Add unit tests covering v2.35+ `app_storage.json` key migration (`agy.onboarding.lastLoginUsername` preferred, `jetski.onboarding.lastLoginUsername` fallback), v2.35+ `--read_host_bridge_token_from_stdin` command line detection, and fallback email resolution when `query_local_user_status` cannot run:

```rust
    #[test]
    fn test_read_desktop_last_login_username_supports_agy_and_jetski_keys() {
        let dir = tempdir().unwrap();
        let agy_path = dir.path().join("app_storage_agy.json");
        std::fs::write(
            &agy_path,
            r#"{"agy.onboarding.lastLoginUsername": "User@Example.COM "}"#,
        )
        .unwrap();
        assert_eq!(
            read_desktop_last_login_username(&agy_path).as_deref(),
            Some("user@example.com")
        );

        let jetski_path = dir.path().join("app_storage_jetski.json");
        std::fs::write(
            &jetski_path,
            r#"{"jetski.onboarding.lastLoginUsername": "legacy@example.com"}"#,
        )
        .unwrap();
        assert_eq!(
            read_desktop_last_login_username(&jetski_path).as_deref(),
            Some("legacy@example.com")
        );
    }

    #[test]
    fn test_is_desktop_language_server_cmdline_v235() {
        let v235_cmd = "/snap/antigravity/80/opt/antigravity/resources/bin/language_server --standalone --override_ide_name antigravity --subclient_type hub --app_data_dir antigravity --read_host_bridge_token_from_stdin";
        assert!(is_desktop_language_server_cmdline(v235_cmd));

        let ide_cmd = "/snap/antigravity-ide/bin/language_server --subclient_type ide --app_data_dir antigravity-ide";
        assert!(!is_desktop_language_server_cmdline(ide_cmd));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib auth::tests`
Expected: FAIL (`cannot find function read_desktop_last_login_username` and `is_desktop_language_server_cmdline`).

- [ ] **Step 3: Implement helpers and fallback discovery in `src/auth/process_discovery.rs`**

1. Extract `pub fn is_desktop_language_server_cmdline(cmd: &str) -> bool` from lines 161–167 of `src/auth/process_discovery.rs`.
2. Extract `pub fn read_desktop_last_login_username(app_storage_path: &Path) -> Option<String>` checking `"agy.onboarding.lastLoginUsername"` first, then `"jetski.onboarding.lastLoginUsername"`, trimming and lowercasing non-empty strings.
3. Add `pub fn resolve_desktop_session_email(live_email: &str, keyring_email: Option<&str>, app_storage_email: Option<&str>) -> Option<String>` returning the first non-empty trimmed lowercase email.
4. In `discover_antigravity_desktop_app()`:
   - Read `fallback_email` via `read_desktop_last_login_username(&app_storage_path)`.
   - When `is_desktop_language_server_cmdline(&cmd)` matches for a `/proc/{pid}/cmdline`:
     - First try `query_local_user_status(port, &csrf)` when `!csrf.is_empty()` across `pid_ports`.
     - If `csrf` is empty (e.g., `--read_host_bridge_token_from_stdin` in v2.35+) or no port responded, still resolve the email using `get_desktop_keyring_token().await` and `fallback_email`, returning `Some(DiscoveredSession { email: final_email, name: None, port: None, csrf_token: None, tier: None, surface: Some("Antigravity Desktop App".into()), pid: Some(pid) })` so the running desktop session is still detected.
5. In `src/auth/sync.rs:43-52`: only overwrite `existing.app_port` / `existing.app_csrf_token` when `s.port.is_some()` / `s.csrf_token.is_some()` so a headless v2.35 discovery does not wipe previously recorded fields needlessly, while `is_current_desktop_session = Some(true)` and `active_email` are preserved.

- [ ] **Step 4: Run auth unit tests**

Run: `cargo test --lib auth::tests`
Expected: PASS (all auth tests pass).

- [ ] **Step 5: Commit**

```bash
git add src/auth/process_discovery.rs src/auth/sync.rs src/auth/mod.rs
git commit -m "fix(auth): detect Antigravity v2.35+ desktop sessions without cmdline csrf_token"
```

---

### Task 2: Type-Safe `QuotaIndicator` & Pure Icon Rendering with Cache-Miss Disk Writes

**Files:**
- Modify: `src/tray/icon.rs:1-134`
- Test: `src/tray/icon.rs`

**Interfaces:**
- Produces in `src/tray/icon.rs`:
  - `#[derive(Debug, Clone, Copy, PartialEq)] pub enum QuotaIndicator { Available(f64), Exhausted }`
  - `impl QuotaIndicator { pub fn from_percent(pct: f64) -> Self; pub fn icon_name(self) -> String; pub fn title_text(self) -> String; pub fn is_exhausted(self) -> bool; }`
  - `pub fn default_icon_cache_dir() -> PathBuf` (using `dirs::config_dir()`)
  - `pub fn render_tray_icon(indicator: QuotaIndicator) -> (Icon, String)` (pure, no filesystem I/O)
  - `pub fn ensure_cached_icon_png(indicator: QuotaIndicator, cache_dir: &Path) -> std::io::Result<PathBuf>` (writes PNG only if missing)

- [ ] **Step 1: Write failing unit tests in `src/tray/icon.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_quota_indicator_variants_and_formatting() {
        let ok = QuotaIndicator::from_percent(85.0);
        assert_eq!(ok, QuotaIndicator::Available(85.0));
        assert_eq!(ok.icon_name(), "quota_85");
        assert_eq!(ok.title_text(), "Antigravity: 85.0%");
        assert!(!ok.is_exhausted());

        let zero = QuotaIndicator::from_percent(0.0);
        assert_eq!(zero, QuotaIndicator::Exhausted);
        assert_eq!(zero.icon_name(), "quota_exhausted");
        assert_eq!(zero.title_text(), "Antigravity: Quota Exhausted");
        assert!(zero.is_exhausted());
    }

    #[test]
    fn test_render_tray_icon_is_pure_and_cache_skips_existing_png() {
        let (icon, name) = render_tray_icon(QuotaIndicator::Available(72.0));
        assert_eq!(icon.width, 48);
        assert_eq!(icon.height, 48);
        assert_eq!(icon.data.len(), 48 * 48 * 4);
        assert_eq!(name, "quota_72");

        let dir = tempdir().unwrap();
        let path = ensure_cached_icon_png(QuotaIndicator::Available(72.0), dir.path()).unwrap();
        assert!(path.is_file());
        let first_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        let path2 = ensure_cached_icon_png(QuotaIndicator::Available(72.0), dir.path()).unwrap();
        let second_mtime = std::fs::metadata(&path2).unwrap().modified().unwrap();
        assert_eq!(first_mtime, second_mtime, "existing cached PNG must not be rewritten");
    }

    #[test]
    fn test_default_icon_cache_dir_uses_config_dir() {
        let dir = default_icon_cache_dir();
        assert!(dir.ends_with("antigravity-token-tracker/icons"));
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --lib tray::icon`
Expected: FAIL (`cannot find type QuotaIndicator` / `ensure_cached_icon_png`).

- [ ] **Step 3: Implement `QuotaIndicator`, pure `render_tray_icon`, and `ensure_cached_icon_png` in `src/tray/icon.rs`**

1. Define `QuotaIndicator` with `from_percent(pct: f64) -> Self` (`Exhausted` if `pct <= 0.0 || !pct.is_finite()`, else `Available(pct.clamp(0.0, 100.0))`), `icon_name`, `title_text`, `is_exhausted`, and `///` doc comments.
2. Update `default_icon_cache_dir() -> PathBuf` to use `dirs::config_dir()` (matching `TrayLock::default_lock_path()`).
3. Extract shared pixel generation helper returning `(Vec<u8>, ImageBuffer<Rgba<u8>, Vec<u8>>)` or convert ARGB32 to RGBA inside `ensure_cached_icon_png` so `render_tray_icon(indicator: QuotaIndicator) -> (Icon, String)` performs zero filesystem I/O.
4. Implement `ensure_cached_icon_png(indicator: QuotaIndicator, cache_dir: &Path) -> std::io::Result<PathBuf>` which returns `Ok(png_path)` immediately if `png_path.is_file()`, otherwise creates `cache_dir` and saves the PNG.

- [ ] **Step 4: Run icon unit tests**

Run: `cargo test --lib tray::icon`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/tray/icon.rs
git commit -m "refactor(tray): introduce QuotaIndicator enum and pure icon rendering with cache-miss PNG writes"
```

---

### Task 3: Typed `TrayError` & Race-Free Lockfile

**Files:**
- Modify: `src/tray/lock.rs:1-109`
- Test: `src/tray/lock.rs`

**Interfaces:**
- Produces in `src/tray/lock.rs`:
  - `#[derive(Debug, thiserror::Error)] pub enum TrayError { Lock(#[from] TrayLockError), Dbus(#[from] ksni::Error), Io(#[from] std::io::Error) }`
  - `pub struct TrayLock` (unchanged public struct, `Drop` unlocks without unlinking to eliminate lockfile inode race; `acquire` seeks to 0 before reading contending PID)

- [ ] **Step 1: Write failing unit tests in `src/tray/lock.rs`**

Add tests for `TrayError` formatting/source and race-free lock re-acquisition across `Drop`:

```rust
    #[test]
    fn test_tray_error_variants() {
        let lock_err = TrayError::from(TrayLockError::AlreadyRunning(Some(42)));
        assert!(lock_err.to_string().contains("42"));

        let io_err = TrayError::from(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ));
        assert!(io_err.to_string().contains("denied"));
    }

    #[test]
    fn test_tray_lock_contends_reads_pid_and_reacquires_after_drop() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("tray.lock");

        let lock1 = TrayLock::acquire(Some(&lock_path)).unwrap();
        let lock2 = TrayLock::acquire(Some(&lock_path));
        match lock2 {
            Err(TrayLockError::AlreadyRunning(Some(pid))) => {
                assert_eq!(pid, std::process::id());
            }
            other => panic!("expected AlreadyRunning(Some(self_pid)), got {other:?}"),
        }

        drop(lock1);
        assert!(lock_path.exists(), "lockfile stays on disk to avoid unlink race");
        let lock3 = TrayLock::acquire(Some(&lock_path));
        assert!(lock3.is_ok());
    }
```

- [ ] **Step 2: Run lock tests to verify failure**

Run: `cargo test --lib tray::lock`
Expected: FAIL (`cannot find type TrayError`, and `lock_path.exists()` fails if `Drop` unlinks).

- [ ] **Step 3: Implement `TrayError` and harden `TrayLock` in `src/tray/lock.rs`**

1. Define `#[derive(Debug, thiserror::Error)] pub enum TrayError` with `Lock(#[from] TrayLockError)`, `Dbus(#[from] ksni::Error)`, and `Io(#[from] std::io::Error)`, plus `///` doc comments.
2. In `TrayLock::acquire`:
   - On lock acquisition, handle `set_len(0)`, `seek(SeekFrom::Start(0))`, `writeln!(file, "{pid}")`, and `flush()` without silent `let _ =` (propagate `std::io::Error` via `?`).
   - On `Err(_)`: call `let _ = file.seek(SeekFrom::Start(0));` before `file.read_to_string(&mut content)` so the PID is read deterministically.
3. In `TrayLock::drop`: call `let _ = fs2::FileExt::unlock(&self._file);` and omit `std::fs::remove_file(&self.path)` so concurrent contenders always lock the same inode.

- [ ] **Step 4: Run lock tests**

Run: `cargo test --lib tray::lock`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/tray/lock.rs
git commit -m "refactor(tray): add typed TrayError and eliminate TrayLock drop unlink race"
```

---

### Task 4: Reactive Tray State, Command-Driven Menu Callbacks & Clean Shutdown

**Files:**
- Modify: `src/tray/applet.rs:1-328`
- Modify: `src/tray/mod.rs:1-187`
- Test: `src/tray/mod.rs`

**Interfaces:**
- Produces in `src/tray/applet.rs`:
  - `pub enum TrayCommand { RefreshQuotas, ToggleShield, ToggleBalancer, SwitchAccount(String), OpenDashboard, Quit }`
  - `pub struct TrayState` (uses `indicator: QuotaIndicator`, `active_email: Option<String>`, `active_summary: String`, `icon_name: String`, `icon_data: Arc<Vec<u8>>`, `icon_width: i32`, `icon_height: i32`, `accounts: Vec<AccountProfile>`, `geo_summary: String`, `is_restricted: bool`, `is_shield_enabled: bool`, `is_balancer_enabled: bool`, `balancer_strategy: BalancerStrategy`, `backup_candidate: Option<(String, f64)>`)
  - `pub struct AntigravityTray { pub state: Arc<Mutex<TrayState>>, pub cmd_tx: tokio::sync::mpsc::UnboundedSender<TrayCommand> }`
- Produces in `src/tray/mod.rs`:
  - Explicit re-exports (no glob `pub use *;`!)
  - `pub fn build_tray_state_from_data(...) -> TrayState`
  - `pub async fn run_tray(interval_secs: u64, lock_path: Option<&Path>, shutdown: Option<tokio::sync::watch::Receiver<bool>>) -> Result<(), TrayError>`

- [ ] **Step 1: Write failing unit tests in `src/tray/mod.rs`**

Test `build_tray_state_from_data` (active session selection, `backup_candidate` population, `QuotaIndicator` derivation), `Switch Account` menu filtering when multiple accounts have `active: Some(true)`, and `TrayCommand` dispatch for `ToggleShield`, `ToggleBalancer`, `RefreshQuotas`, and `Quit`:

```rust
    #[test]
    fn test_build_tray_state_populates_active_and_backup_candidate() {
        let mut q_map = std::collections::HashMap::new();
        q_map.insert(
            "active@example.com".to_string(),
            crate::quota::AccountQuotaResult {
                email: "active@example.com".into(),
                name: "Active".into(),
                tier: "Pro".into(),
                is_current_desktop_session: true,
                is_current_ide_session: false,
                groups: vec![crate::storage::models::QuotaGroup {
                    display_name: "Gemini".into(),
                    weekly: Some(crate::storage::models::QuotaBucket {
                        remaining_percent: Some(42.5),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                models: vec![],
                checked_at: 1000.0,
                is_cached: false,
                error: None,
            },
        );
        q_map.insert(
            "backup@example.com".to_string(),
            crate::quota::AccountQuotaResult {
                email: "backup@example.com".into(),
                name: "Backup".into(),
                tier: "Pro".into(),
                is_current_desktop_session: false,
                is_current_ide_session: false,
                groups: vec![crate::storage::models::QuotaGroup {
                    display_name: "Gemini".into(),
                    weekly: Some(crate::storage::models::QuotaBucket {
                        remaining_percent: Some(95.0),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                models: vec![],
                checked_at: 1000.0,
                is_cached: false,
                error: None,
            },
        );

        let dir = tempfile::tempdir().unwrap();
        let state = build_tray_state_from_data(
            &q_map,
            vec![],
            &crate::shield::models::EgressGeo::default(),
            false,
            false,
            crate::balancer::BalancerStrategy::ExpiryFirst,
            Some(("backup@example.com".into(), 95.0)),
            dir.path(),
        );
        assert_eq!(state.indicator, QuotaIndicator::Available(42.5));
        assert_eq!(state.active_email.as_deref(), Some("active@example.com"));
        assert_eq!(
            state.backup_candidate,
            Some(("backup@example.com".to_string(), 95.0))
        );
    }

    #[test]
    fn test_switch_submenu_excludes_only_current_session_not_legacy_active_flag() {
        let accounts = vec![
            crate::storage::models::AccountProfile {
                email: "current@example.com".into(),
                active: Some(true),
                is_current_desktop_session: Some(true),
                ..Default::default()
            },
            crate::storage::models::AccountProfile {
                email: "other@example.com".into(),
                active: Some(true),
                is_current_desktop_session: Some(false),
                ..Default::default()
            },
        ];
        let candidates = switchable_accounts(&accounts, Some("current@example.com"));
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].email, "other@example.com");
    }

    #[tokio::test]
    async fn test_run_tray_graceful_shutdown_via_watch() {
        let dir = tempfile::tempdir().unwrap();
        let lock_file = dir.path().join("tray.lock");
        let (tx, rx) = tokio::sync::watch::channel(true);
        drop(tx);
        // Should exit immediately when shutdown is already signaled (before or after lock)
        let res = run_tray(60, Some(&lock_file), Some(rx)).await;
        // On headless CI without session D-Bus, either Ok(()) if shutdown checked early or TrayError::Dbus
        assert!(res.is_ok() || matches!(res, Err(TrayError::Dbus(_)) | Err(TrayError::Lock(_))));
    }
```

- [ ] **Step 2: Run tray tests to verify failure**

Run: `cargo test --lib tray`
Expected: FAIL (`build_tray_state_from_data`, `switchable_accounts`, `TrayCommand` not found, `run_tray` signature mismatch).

- [ ] **Step 3: Refactor `src/tray/applet.rs`**

1. Define `pub enum TrayCommand { RefreshQuotas, ToggleShield, ToggleBalancer, SwitchAccount(String), OpenDashboard, Quit }` with `///` docs.
2. Update `TrayState` to store `indicator: QuotaIndicator`, `active_email: Option<String>`, `icon_data: Arc<Vec<u8>>`, `icon_width: i32`, `icon_height: i32`, and keep `accounts`, `geo_summary`, `is_restricted`, `is_shield_enabled`, `is_balancer_enabled`, `balancer_strategy`, `backup_candidate`.
3. Add `pub fn switchable_accounts<'a>(accounts: &'a [AccountProfile], active_email: Option<&str>) -> Vec<&'a AccountProfile>` excluding only accounts matching `active_email` (case-insensitive) or with `is_current_desktop_session == Some(true)` / `is_current_ide_session == Some(true)`.
4. In `AntigravityTray`:
   - Lock `self.state` helper returning a clone/snapshot of needed fields cleanly (no `expect` panic on poisoned lock if avoidable, or `expect("tray state lock poisoned")` once per method; keep `Arc<Vec<u8>>` cheap to share and clone bytes only for `ksni::Icon`).
   - All `activate` handlers (`AntigravityTray::activate` and every `StandardItem::activate`) simply call `let _ = self.cmd_tx.send(TrayCommand::...);` — zero blocking `open::that`, zero `tokio::spawn` inside `Tray::menu()`, and zero `std::process::exit(0)` in `Quit Applet` (sends `TrayCommand::Quit`).

- [ ] **Step 4: Refactor `src/tray/mod.rs`**

1. Replace glob `pub use *;` with explicit re-exports:
   - `pub use applet::{AntigravityTray, TrayCommand, TrayState, switchable_accounts};`
   - `pub use icon::{QuotaIndicator, default_icon_cache_dir, ensure_cached_icon_png, render_tray_icon};`
   - `pub use lock::{TrayError, TrayLock, TrayLockError, default_lock_path};`
2. Implement `pub fn build_tray_state_from_data(...) -> TrayState` computing `indicator`, `active_email`, `active_summary`, calling `render_tray_icon` + `ensure_cached_icon_png`, and storing `backup_candidate`.
3. Update `pub async fn run_tray(interval_secs: u64, lock_path: Option<&Path>, mut shutdown: Option<tokio::sync::watch::Receiver<bool>>) -> Result<(), TrayError>`:
   - Check `shutdown` early if already `true`.
   - Acquire `TrayLock::acquire(lock_path)?`.
   - Spawn web server with error logging: `if let Err(e) = crate::web::server::start_web_server(8765).await { eprintln!("[Tray] Web server failed on port 8765: {e}"); }`.
   - Create `(cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<TrayCommand>()`.
   - Spawn `ksni` tray service. Warn clearly if `watcher_offline` triggers instead of silently pretending registration succeeded.
   - Extract `async fn refresh_tray_state(...)` using `QuotaRefreshPolicy::ForceRefresh` (so 60s polls and manual `RefreshQuotas` always fetch live quotas instead of 10-minute stale cache), computing `find_best_failover_candidate(&q_map, active_email.as_deref(), &exclude, 5.0)`, updating `shared_state`, and calling `service.update(|_| {}).await`.
   - Handle `TrayCommand`:
     - `OpenDashboard`: `tokio::task::spawn_blocking(|| { if let Err(e) = open::that("http://localhost:8765") { eprintln!("[Tray] Failed to open browser: {e}"); } });`
     - `ToggleShield` / `ToggleBalancer`: update config on disk, update `shared_state`, call `service.update(|_| {}).await`, and send notification.
     - `SwitchAccount(email)`: spawn async task to call `switch_to_account`, send notification, and send `TrayCommand::RefreshQuotas` back on `cmd_tx`!
     - `RefreshQuotas`: run `refresh_tray_state(..., ForceRefresh)`.
     - `Quit`: break loop.
   - Listen to `tokio::signal::ctrl_c()`, `tokio::signal::unix::signal(SignalKind::terminate())`, and optional `shutdown` watch receiver; on exit, call `service.shutdown().await;` and return `Ok(())` so `TrayLock::drop` executes cleanly.
4. Update `src/main.rs:1058` to pass `None` as the 3rd argument (`shutdown`) to `tracker_lib::tray::run_tray(interval, None, None)`.

- [ ] **Step 5: Run tray tests & clippy**

Run: `cargo test --lib tray && cargo clippy --all-targets --all-features -- -D warnings`
Expected: PASS, 0 warnings.

- [ ] **Step 6: Commit**

```bash
git add src/tray/applet.rs src/tray/mod.rs src/main.rs
git commit -m "refactor(tray): make menu actions reactive via command channel and add clean SIGTERM shutdown"
```

---

### Task 5: Wire `daemon --tray` CLI Flag & Fix Systemd / Autostart Service Packaging

**Files:**
- Modify: `src/main.rs:785-866`
- Modify: `antigravity-token-tracker.service:1-16`
- Modify: `install.sh:32-85`
- Test: `src/main.rs`

**Interfaces:**
- Produces in `src/main.rs`:
  - `struct DaemonCliArgs { interval_secs: u64, auto_switch: bool, launch_tray: bool }`
  - `fn parse_daemon_args(argv: &[OsString]) -> DaemonCliArgs`

- [ ] **Step 1: Write failing unit test in `src/main.rs`**

Add a `#[cfg(test)] mod tests` block in `src/main.rs` testing `parse_daemon_args`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_daemon_args_reads_tray_interval_and_auto_switch() {
        let args = vec![
            OsString::from("agy-token"),
            OsString::from("daemon"),
            OsString::from("--interval"),
            OsString::from("300"),
            OsString::from("--auto-switch"),
            OsString::from("--tray"),
        ];
        let parsed = parse_daemon_args(&args);
        assert_eq!(parsed.interval_secs, 300);
        assert!(parsed.auto_switch);
        assert!(parsed.launch_tray);
    }

    #[test]
    fn test_parse_daemon_args_defaults() {
        let args = vec![OsString::from("agy-token"), OsString::from("daemon")];
        let parsed = parse_daemon_args(&args);
        assert_eq!(parsed.interval_secs, 60);
        assert!(!parsed.auto_switch);
        assert!(!parsed.launch_tray);
    }
}
```

- [ ] **Step 2: Run test to verify failure**

Run: `cargo test --bin run_tracker test_parse_daemon_args`
Expected: FAIL (`cannot find function parse_daemon_args`).

- [ ] **Step 3: Implement `parse_daemon_args` and `--tray` wiring in `src/main.rs`**

1. Extract `parse_daemon_args(argv: &[OsString]) -> DaemonCliArgs` supporting `--interval N`, `--interval=N`, `--auto-switch`, and `--tray`.
2. In `run_daemon_doctor` (`command == "daemon"`):
   - Create a single `tokio::sync::watch::channel(false)` shared between `SIGINT`/`SIGTERM` handler and `run_daemon`.
   - If `parsed.launch_tray` is `true`, run `tracker_lib::daemon::run_daemon(...)` and `tracker_lib::tray::run_tray(parsed.interval_secs, None, Some(rx.clone()))` concurrently via `tokio::join!` (or trigger `tx.send(true)` if either exits).
3. Update `antigravity-token-tracker.service` and `install.sh`:
   - In `install.sh` and `antigravity-token-tracker.service`: order the user service `After=graphical-session.target` (`PartOf=graphical-session.target`, `WantedBy=graphical-session.target`) so GNOME Shell and `org.kde.StatusNotifierWatcher` are ready before launch, and remove `~/.config/autostart/agy-token.desktop` when `systemd --user` successfully enables the service (keeping autostart only in the non-systemd fallback branch, eliminating duplicate process races at login).

- [ ] **Step 4: Run full verification suite**

Run:
```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --lib
cargo test --bin run_tracker
```
Expected: All commands exit 0 with 0 warnings. (Note: `cargo test --test app_cases` depends on `app-cases/list.toml`, which checks live local `accounts.json` session output; re-verify `cargo test --test app_cases` after desktop session discovery in Task 1.)

- [ ] **Step 5: Commit**

```bash
git add src/main.rs antigravity-token-tracker.service install.sh
git commit -m "fix(daemon,install): wire daemon --tray flag and eliminate systemd/autostart boot race"
```
