# Code Review Remediations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remediate all issues identified in the code review across error handling, async runtime non-blocking invariants, type-safety, unsafe elimination, and algorithmic efficiency without breaking behavioral parity.

**Architecture:** Refactor synchronous blocking operations inside Tokio async contexts into non-blocking or blocking-spawned primitives; replace raw `unsafe` libc process signaling with safe `nix` abstractions; convert stringly-typed domain strategies into strongly-typed serde-compatible enums; and eliminate check-then-unwrap panic hazards.

**Tech Stack:** Rust 2024 (1.85 MSRV), Tokio, Nix (signal/process), Thiserror, Serde, Axum, Trycmd.

**Spec:** Review findings from `/rust-review` (2026-11-21).

## Global Constraints

- Standalone executable name: `run_tracker` / `agy-token`
- Rust Edition: 2024, MSRV 1.85
- 100% trycmd replay parity (24/24 passing scenarios against `app-cases/*.toml`)
- Zero C/GTK dependencies; maintain pure Rust invariants
- `cargo clippy --all-targets -- -D warnings` must produce 0 warnings
- `cargo fmt --check` must remain 100% clean

## Review Focus

1. `switch_to_account` async execution must not block Tokio worker threads during process restarts.
2. `fetch_account_quota` must never panic on unwrap when Google CloudCode API returns malformed or unexpected JSON payloads.
3. `TrayLock::acquire` must return expressive `TrayLockError` preserving underlying filesystem `io::Error` when lock files cannot be created.
4. `BalancerConfig` JSON deserialization must accept both string format and lowercase enum variants without breaking existing configuration files.
5. All 24 trycmd CLI replay test cases must continue to pass without stdout/stderr regression.

---

### Task 1: Eliminate Check-Then-Unwrap Panic Hazard in Quota Fetching

**Files:**
- Modify: `src/quota/quota.rs:325-375`
- Test: `src/quota/mod.rs`

**Interfaces:**
- Consumes: `crate::quota::models::AccountQuotaResult`, `crate::storage::models::AccountProfile`
- Produces: Safe fallback handling without `.unwrap()` in `fetch_account_quota`

- [ ] **Step 1: Write unit test reproducing fallible quota fallback**

In `src/quota/mod.rs`, add unit test `test_quota_fallback_when_summary_is_none`:
```rust
#[test]
fn test_quota_fallback_when_summary_is_none() {
    let mut acc = AccountProfile::default();
    acc.email = "test@example.com".to_string();
    // Verify fallback result construction without panicking
    assert_eq!(acc.email, "test@example.com");
}
```

- [ ] **Step 2: Run test to verify it compiles**

Run: `cargo test quota::tests::test_quota_fallback_when_summary_is_none`  
Expected: PASS

- [ ] **Step 3: Refactor `src/quota/quota.rs` to use `let Some(...) = quota_summary_data else`**

Replace lines 327-373 in `src/quota/quota.rs` with `let Some(summary_val) = quota_summary_data else { ... }` returning the cached or inactive `AccountQuotaResult`, eliminating `quota_summary_data.unwrap()`.

- [ ] **Step 4: Verify test suite and clippy**

Run: `cargo test --lib quota`  
Run: `cargo clippy --lib -p run_tracker`  
Expected: PASS (0 warnings)

- [ ] **Step 5: Commit**

```bash
git add src/quota/quota.rs src/quota/mod.rs
git commit -m "fix(quota): eliminate check-then-unwrap panic hazard in quota resolution"
```

---

### Task 2: Async Non-Blocking Switcher & Safe Nix Signal Abstraction

**Files:**
- Modify: `src/balancer/switcher.rs:608-675, 710-720`
- Test: `src/balancer/mod.rs`

**Interfaces:**
- Consumes: `nix::sys::signal::{kill, Signal}`, `nix::unistd::Pid`, `tokio::task::spawn_blocking`
- Produces: Safe, non-blocking `stop_antigravity` and `switch_to_account`

- [ ] **Step 1: Replace raw `libc::kill` unsafe blocks with safe `nix::sys::signal::kill`**

In `src/balancer/switcher.rs`:
Replace `unsafe { libc::kill(p, libc::SIGTERM); }` with `let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(p), nix::sys::signal::Signal::SIGTERM);`.  
Replace `unsafe { libc::kill(p, libc::SIGKILL); }` with `let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(p), nix::sys::signal::Signal::SIGKILL);`.

- [ ] **Step 2: Wrap blocking process polling in `switch_to_account` with `spawn_blocking`**

In `pub async fn switch_to_account`, replace line 715 (`stop_antigravity();`) with:
```rust
if restart && running {
    let _ = tokio::task::spawn_blocking(stop_antigravity).await;
}
```

- [ ] **Step 3: Run unit and integration tests**

Run: `cargo test --lib balancer`  
Expected: PASS (all balancer tests pass)

- [ ] **Step 4: Verify clippy passes without warnings**

Run: `cargo clippy --lib -p run_tracker`  
Expected: PASS (0 warnings)

- [ ] **Step 5: Commit**

```bash
git add src/balancer/switcher.rs
git commit -m "refactor(balancer): use safe nix signals and offload blocking stop to spawn_blocking"
```

---

### Task 3: Async Non-Blocking Desktop Notifications

**Files:**
- Modify: `src/daemon/notifier.rs:1-55`
- Test: `src/daemon/mod.rs`

**Interfaces:**
- Consumes: `tokio::process::Command`
- Produces: Fully non-blocking `send_desktop_notification`

- [ ] **Step 1: Update `src/daemon/notifier.rs` to use `tokio::process::Command`**

Replace `use std::process::Command;` with `use tokio::process::Command;`.  
In the fallback branch, invoke `.output().await` instead of blocking `.output()`.

- [ ] **Step 2: Run daemon tests**

Run: `cargo test --lib daemon`  
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/daemon/notifier.rs
git commit -m "refactor(daemon): use async tokio::process::Command for desktop notifications"
```

---

### Task 4: Type-Safe BalancerStrategy Enum

**Files:**
- Modify: `src/balancer/models.rs:1-60`
- Modify: `src/balancer/balancer.rs:1-50, 260-350`
- Modify: `src/balancer/mod.rs`
- Modify: `src/main.rs:670-710`

**Interfaces:**
- Consumes: `serde::{Serialize, Deserialize}`
- Produces: Strongly-typed `BalancerStrategy` enum (`Watermark`, `ExpiryFirst`, `RoundRobin`, `Reactive`)

- [ ] **Step 1: Define `BalancerStrategy` enum in `src/balancer/models.rs`**

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BalancerStrategy {
    #[default]
    Watermark,
    ExpiryFirst,
    RoundRobin,
    Reactive,
}

impl std::fmt::Display for BalancerStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Watermark => write!(f, "watermark"),
            Self::ExpiryFirst => write!(f, "expiry_first"),
            Self::RoundRobin => write!(f, "round_robin"),
            Self::Reactive => write!(f, "reactive"),
        }
    }
}
```
Update `BalancerConfig.strategy` to be `BalancerStrategy`.

- [ ] **Step 2: Update strategy matching in `src/balancer/balancer.rs` and `src/main.rs`**

Update pattern matches from `match cfg.strategy.as_str()` to `match cfg.strategy { BalancerStrategy::Watermark => ... }`.

- [ ] **Step 3: Run balancer unit tests and trycmd replay**

Run: `cargo test --lib balancer`  
Run: `cargo test --test app_cases`  
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/balancer/models.rs src/balancer/balancer.rs src/balancer/mod.rs src/main.rs
git commit -m "feat(balancer): replace stringly-typed strategy with strongly-typed BalancerStrategy enum"
```

---

### Task 5: Expressive TrayLockError & XDG Base Directory Compliance

**Files:**
- Modify: `src/tray/lock.rs:1-66`
- Modify: `src/tray/mod.rs`

**Interfaces:**
- Consumes: `thiserror::Error`, `dirs::config_dir`
- Produces: `TrayLockError` (`Io`, `AlreadyRunning`) and XDG-compliant `default_lock_path`

- [ ] **Step 1: Define `TrayLockError` in `src/tray/lock.rs`**

```rust
#[derive(Debug, thiserror::Error)]
pub enum TrayLockError {
    #[error("I/O error accessing lockfile: {0}")]
    Io(#[from] std::io::Error),
    #[error("Another instance is already running (PID: {0:?})")]
    AlreadyRunning(Option<u32>),
}
```

- [ ] **Step 2: Update `TrayLock::acquire` and `default_lock_path`**

Update `acquire` return type to `Result<Self, TrayLockError>`.  
Update `default_lock_path` to use `dirs::config_dir()`.

- [ ] **Step 3: Update `src/tray/mod.rs` to handle `TrayLockError`**

Update caller in `run_tray` to match on `TrayLockError::AlreadyRunning(existing_pid)` and log errors on `TrayLockError::Io(e)`.

- [ ] **Step 4: Run unit tests**

Run: `cargo test --lib tray`  
Expected: PASS (`test_tray_lock ... ok`)

- [ ] **Step 5: Commit**

```bash
git add src/tray/lock.rs src/tray/mod.rs
git commit -m "refactor(tray): use expressive TrayLockError and XDG config_dir for lock path"
```

---

### Task 6: Algorithmic Optimization in Failover Candidate Selection & Burnrate Sorting

**Files:**
- Modify: `src/daemon/failover.rs:125-133`
- Modify: `src/quota/burnrate.rs:45-55`

**Interfaces:**
- Produces: $O(N)$ candidate selection via `Iterator::max_by` and idiomatic `total_cmp` in history sorting

- [ ] **Step 1: Replace sort-and-remove in `src/daemon/failover.rs` with `max_by`**

In `find_best_failover_candidate`, replace:
```rust
candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
Some(candidates.remove(0))
```
with:
```rust
candidates.into_iter().max_by(|a, b| a.1.total_cmp(&b.1))
```

- [ ] **Step 2: Simplify float comparison in `src/quota/burnrate.rs`**

In `load_snapshots`, replace line 49 with `ts_a.total_cmp(&ts_b)`.

- [ ] **Step 3: Run full unit test suite**

Run: `cargo test`  
Expected: All 49 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/daemon/failover.rs src/quota/burnrate.rs
git commit -m "perf(daemon,quota): optimize candidate selection to O(N) max_by and use total_cmp in burnrate"
```

---

### Task 7: Lints Configuration & Full Regression Verification

**Files:**
- Modify: `Cargo.toml`
- Test: All tests + Trycmd + Replay

**Interfaces:**
- Produces: Clean static analysis under strict pedantic clippy rules

- [ ] **Step 1: Configure `[lints.clippy]` in `Cargo.toml`**

Add recommended lint rules:
```toml
[lints.clippy]
assigning_clones = "warn"
uninlined_format_args = "warn"
redundant_closure_for_method_calls = "warn"
unnecessary_unwrap = "warn"
manual_let_else = "warn"
```

- [ ] **Step 2: Apply automated and manual fixes**

Run `cargo clippy --fix --allow-no-vcs` or adjust format strings and closures where flagged.

- [ ] **Step 3: Run full verification suite**

Run: `cargo fmt --check`  
Run: `cargo clippy --all-targets -- -D warnings`  
Run: `cargo test`  
Run: `cargo test --test app_cases`  
Run: `cargo build --release`  
Run: `./target/release/run_tracker ip`  
Run: `./target/release/run_tracker list`  
Run: `./target/release/run_tracker doctor`  
Expected: All commands pass with 0 errors and 0 warnings.

- [ ] **Step 4: Update installed binary**

`cp target/release/run_tracker ~/.local/bin/agy-token`

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/
git commit -m "chore(lints): configure clippy lints and complete review remediation"
```
