# Code Review Round 2 Remediations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement remediations for all findings identified in the second `/rust-review` audit, including background daemon execution loop, atomic shield state, type-safe enums, safe process detachment, and pedantic lint configuration.

**Architecture:** Refactor runtime entrypoints, global state synchronization, and stringly-typed domain models across daemon, shield, and balancer modules. Replace blocking or unsafe patterns with safe standard library abstractions and lock-free atomics.

**Tech Stack:** Rust 2024 (1.85), Tokio async runtime, `thiserror`, `serde`, `nix`, `trycmd`.

**Spec:** `/rust-review` findings (2026-11-21)

## Global Constraints
- Edition 2024, MSRV 1.85.
- Pure Rust: Zero Python runtime dependencies, zero GTK/GObject C dependencies (`ksni` over DBus).
- 100% trycmd replay parity (24/24 integration scenarios passing).
- 0 warnings under `cargo clippy --all-targets -- -D warnings`.
- Non-blocking Tokio async operations throughout.
- Code properly formatted with `cargo fmt --check`.

## Review Focus
1. **Daemon graceful shutdown**: A SIGINT / SIGTERM signal sent to `agy-token daemon` must cleanly exit without orphaning background tasks or corrupting history logs.
2. **Atomic float bit-casting**: Converting between `f64` and `u64` via `f64::to_bits` and `f64::from_bits` must correctly preserve timestamps and handle 0.0 values without NaN traps.
3. **Shield deserialization backward-compatibility**: Existing `shield.json` files storing `"mode": "restricted"` and `"action": "kill_process"` must deserialize cleanly into new enum types without breaking user configurations.
4. **Surface string backwards-compatibility**: CLI commands and DBus activations passing strings `"desktop"`, `"ide"`, and `"both"` must parse cleanly into `SwitchSurface` variants.
5. **Process group detachment on Linux**: `CommandExt::process_group(0)` must correctly isolate spawned Antigravity processes without requiring `unsafe pre_exec`.

---

### Task 1: Safe Atomic Shield State & Anti-Poisoning

**Files:**
- Modify: `src/shield/shield.rs:10-25`
- Modify: `src/shield/shield.rs:205-265`
- Test: `src/shield/mod.rs`

**Interfaces:**
- Produces: Lock-free `AtomicU64` and `AtomicBool` globals replacing `Mutex<f64>` and `Mutex<bool>`, eliminating lock poisoning hazards.

- [ ] **Step 1: Write unit test in `src/shield/mod.rs` for concurrent shield state**

```rust
#[tokio::test]
async fn test_shield_evaluation_no_poisoning() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path();
    let geo = EgressGeo {
        ip: "1.1.1.1".into(),
        country_code: "US".into(),
        country_name: "United States".into(),
        is_restricted: false,
        ..Default::default()
    };
    // Run multiple parallel evaluations
    let mut handles = Vec::new();
    for _ in 0..10 {
        let g = geo.clone();
        handles.push(tokio::spawn(async move {
            let temp_dir = std::env::temp_dir();
            evaluate_and_enforce_shield(&temp_dir, Some(g)).await
        }));
    }
    for h in handles {
        let res = h.await.unwrap();
        assert!(!res.violation);
    }
}
```

- [ ] **Step 2: Replace Mutex globals with Atomic primitives in `src/shield/shield.rs`**

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static LAST_TRIGGER_TIMESTAMP_BITS: AtomicU64 = AtomicU64::new(0);
static WAS_VIOLATING: AtomicBool = AtomicBool::new(false);
```

Update read/write logic using `f64::from_bits` / `f64::to_bits`:
- `now_bits = now.to_bits();`
- `let last_trigger = f64::from_bits(LAST_TRIGGER_TIMESTAMP_BITS.load(Ordering::Relaxed));`
- `LAST_TRIGGER_TIMESTAMP_BITS.store(now.to_bits(), Ordering::Relaxed);`
- `WAS_VIOLATING.store(true, Ordering::SeqCst);`
- `WAS_VIOLATING.swap(false, Ordering::SeqCst);`

- [ ] **Step 3: Run shield unit tests**

Run: `cargo test --lib shield`  
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/shield/shield.rs src/shield/mod.rs
git commit -m "fix(shield): use atomic primitives for shield cooldown and violation tracking"
```

---

### Task 2: Type-Safe `ShieldMode` and `ShieldAction` Enums

**Files:**
- Modify: `src/shield/models.rs:45-75`
- Modify: `src/shield/shield.rs`
- Modify: `src/web/server.rs`
- Modify: `src/main.rs:290-390`
- Test: `src/shield/mod.rs`

**Interfaces:**
- Produces: `pub enum ShieldMode` (`Restricted`, `Whitelist`) and `pub enum ShieldAction` (`KillProcess`, `KillNetwork`, `Both`) with serde snake_case mapping.

- [ ] **Step 1: Write failing unit tests for `ShieldMode` and `ShieldAction` in `src/shield/mod.rs`**

```rust
#[test]
fn test_shield_enums_serde_and_from_str() {
    let mode: ShieldMode = serde_json::from_str("\"whitelist\"").unwrap();
    assert_eq!(mode, ShieldMode::Whitelist);
    assert_eq!(serde_json::to_string(&mode).unwrap(), "\"whitelist\"");

    let action: ShieldAction = serde_json::from_str("\"kill_process\"").unwrap();
    assert_eq!(action, ShieldAction::KillProcess);
    assert_eq!(action.to_string(), "kill_process");

    assert_eq!("kill_network".parse::<ShieldAction>().unwrap(), ShieldAction::KillNetwork);
    assert_eq!("both".parse::<ShieldAction>().unwrap(), ShieldAction::Both);
}
```

- [ ] **Step 2: Define `ShieldMode` and `ShieldAction` in `src/shield/models.rs`**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShieldMode {
    #[default]
    Restricted,
    Whitelist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShieldAction {
    #[default]
    KillProcess,
    KillNetwork,
    Both,
}
```
Implement `Display` and `FromStr` for both enums. Update `ShieldConfig`:
```rust
pub struct ShieldConfig {
    pub enabled: bool,
    pub mode: ShieldMode,
    pub allowed_countries: Vec<String>,
    pub action: ShieldAction,
    pub auto_relaunch: bool,
    pub cooldown_seconds: u64,
}
```

- [ ] **Step 3: Update pattern matches across `src/shield/shield.rs`, `src/web/server.rs`, `src/main.rs`**

Replace string matches (`cfg.action == "kill_process"`) with enum matches (`cfg.action == ShieldAction::KillProcess || cfg.action == ShieldAction::Both`).

- [ ] **Step 4: Run test suite**

Run: `cargo test --lib shield`  
Run: `cargo test --test app_cases`  
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/shield/models.rs src/shield/shield.rs src/shield/mod.rs src/web/server.rs src/main.rs
git commit -m "feat(shield): replace stringly-typed mode and action with strongly-typed enums"
```

---

### Task 3: Safe Process Group Launching (Eliminate `unsafe pre_exec`)

**Files:**
- Modify: `src/balancer/switcher.rs:660-680`
- Test: `src/balancer/mod.rs`

**Interfaces:**
- Produces: Safe process detachment using `std::os::unix::process::CommandExt::process_group(0)` with zero `unsafe` blocks.

- [ ] **Step 1: Write test in `src/balancer/mod.rs` verifying process launching helper**

Verify `launch_antigravity` executes without panic.

- [ ] **Step 2: Replace `unsafe pre_exec` in `src/balancer/switcher.rs`**

In `launch_antigravity()`:
Replace:
```rust
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid safely detaches the spawned process into its own session.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
```
with:
```rust
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
```

- [ ] **Step 3: Verify zero `unsafe` remains in `src/`**

Run: `grep -rn "unsafe" src/`  
Expected: 0 occurrences.

- [ ] **Step 4: Run balancer tests**

Run: `cargo test --lib balancer`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/balancer/switcher.rs src/balancer/mod.rs
git commit -m "refactor(balancer): replace unsafe pre_exec setsid with safe CommandExt::process_group"
```

---

### Task 4: Type-Safe `SwitchSurface` and `Result<String, SwitchError>`

**Files:**
- Modify: `src/balancer/switcher.rs:685-777`
- Modify: `src/balancer/balancer.rs`
- Modify: `src/main.rs:900-950`
- Modify: `src/tray/applet.rs`
- Modify: `src/web/server.rs`
- Test: `src/balancer/mod.rs`

**Interfaces:**
- Produces: `pub enum SwitchSurface` (`Both`, `Desktop`, `Ide`), `pub enum SwitchError` (using `thiserror::Error`), and `switch_to_account(...) -> Result<String, SwitchError>`.

- [ ] **Step 1: Write failing unit test in `src/balancer/mod.rs`**

```rust
#[test]
fn test_switch_surface_display_and_parse() {
    assert_eq!("desktop".parse::<SwitchSurface>().unwrap(), SwitchSurface::Desktop);
    assert_eq!("ide".parse::<SwitchSurface>().unwrap(), SwitchSurface::Ide);
    assert_eq!("both".parse::<SwitchSurface>().unwrap(), SwitchSurface::Both);
    assert_eq!(SwitchSurface::Both.to_string(), "both");
}
```

- [ ] **Step 2: Define `SwitchSurface` and `SwitchError` in `src/balancer/switcher.rs`**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SwitchSurface {
    #[default]
    Both,
    Desktop,
    Ide,
}

#[derive(Debug, thiserror::Error)]
pub enum SwitchError {
    #[error("Account not found: {0}")]
    AccountNotFound(String),
    #[error("Failed to update credentials: {0}")]
    UpdateFailed(String),
    #[error("I/O error during session switch: {0}")]
    Io(#[from] std::io::Error),
}
```

Implement `Display` and `FromStr` for `SwitchSurface`. Update signature:
```rust
pub async fn switch_to_account(
    target_email: &str,
    restart: bool,
    surface: SwitchSurface,
) -> Result<String, SwitchError>
```

- [ ] **Step 3: Update callers across `src/main.rs`, `src/balancer/balancer.rs`, `src/tray/applet.rs`, `src/web/server.rs`**

Update callers to pass `SwitchSurface` and handle `Result<String, SwitchError>`.

- [ ] **Step 4: Run full test suite**

Run: `cargo test`  
Expected: All unit tests and trycmd tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/balancer/switcher.rs src/balancer/balancer.rs src/balancer/mod.rs src/main.rs src/tray/applet.rs src/web/server.rs
git commit -m "feat(balancer): use type-safe SwitchSurface enum and Result return type in switch_to_account"
```

---

### Task 5: Complete `agy-token daemon` Service Execution Loop

**Files:**
- Modify: `src/daemon/daemon.rs`
- Modify: `src/daemon/mod.rs`
- Modify: `src/main.rs:809-815`
- Test: `src/daemon/mod.rs`

**Interfaces:**
- Produces: `pub async fn run_daemon(interval_secs: u64, auto_switch: bool, shutdown: Option<tokio::sync::watch::Receiver<bool>>) -> Result<(), DaemonError>` executing the background polling loop with signal handling.

- [ ] **Step 1: Write unit test in `src/daemon/mod.rs` testing `run_daemon` with shutdown signal**

```rust
#[tokio::test]
async fn test_run_daemon_loop_and_graceful_shutdown() {
    let (tx, rx) = tokio::sync::watch::channel(false);
    let handle = tokio::spawn(async move {
        run_daemon(1, false, Some(rx)).await
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    tx.send(true).unwrap();
    let res = handle.await.unwrap();
    assert!(res.is_ok());
}
```

- [ ] **Step 2: Implement `run_daemon` in `src/daemon/daemon.rs`**

```rust
pub async fn run_daemon(
    interval_secs: u64,
    _auto_switch: bool,
    mut shutdown: Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let storage = StorageManager::default();
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let _ = run_daemon_step(&storage, None).await;
            }
            _ = async {
                if let Some(ref mut rx) = shutdown {
                    let _ = rx.changed().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                break;
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Update `src/main.rs` to run the daemon loop**

In `run_daemon_doctor`, when `command == "daemon"`:
```rust
    if command == "daemon" {
        println!("Antigravity Token Tracker Daemon started.");
        let Ok(rt) = tokio::runtime::Runtime::new() else {
            return ExitCode::from(1);
        };
        let _ = rt.block_on(async {
            let (tx, rx) = tokio::sync::watch::channel(false);
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                let _ = tx.send(true);
            });
            tracker_lib::daemon::run_daemon(60, true, Some(rx)).await
        });
        return ExitCode::SUCCESS;
    }
```

- [ ] **Step 4: Run daemon tests and trycmd replay**

Run: `cargo test --lib daemon`  
Run: `cargo test --test app_cases`  
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/daemon/daemon.rs src/daemon/mod.rs src/main.rs
git commit -m "feat(daemon): implement full asynchronous daemon loop with graceful shutdown"
```

---

### Task 6: Numeric Separators, Pedantic Lints & Full Regression Verification

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/balancer/balancer.rs`
- Modify: `src/balancer/mod.rs`
- Modify: `src/quota/mod.rs`
- Test: All tests + Release build

**Interfaces:**
- Produces: 0 warnings under extended clippy rules and updated production binary in `~/.local/bin/agy-token`.

- [ ] **Step 1: Add numeric separators to long literals**

In `src/balancer/balancer.rs:109`: `604800` -> `604_800`.  
In `src/balancer/mod.rs`: `300000` -> `300_000`, `200000` -> `200_000`, `400000` -> `400_000`, etc.  
In `src/quota/mod.rs`: `1700000000.0` -> `1_700_000_000.0`.

- [ ] **Step 2: Configure `[lints.clippy]` in `Cargo.toml`**

Add:
```toml
unreadable_literal = "warn"
needless_raw_string_hashes = "warn"
```

- [ ] **Step 3: Run full verification suite**

Run: `cargo fmt --check`  
Run: `cargo clippy --all-targets -- -D warnings`  
Run: `cargo test`  
Run: `cargo build --release`  
Expected: All checks pass cleanly with 0 warnings.

- [ ] **Step 4: Smoke test release binary and update `~/.local/bin/agy-token`**

Run: `./target/release/run_tracker ip`  
Run: `./target/release/run_tracker list`  
Run: `./target/release/run_tracker doctor`  
Update binary: `cp target/release/run_tracker ~/.local/bin/agy-token`

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/
git commit -m "chore(lints): format numeric literals, enable unreadable_literal lint, and complete verification"
```
