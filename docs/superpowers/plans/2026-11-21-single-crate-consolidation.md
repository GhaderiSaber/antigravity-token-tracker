# Single-Crate Rust Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consolidate the 9 separate sub-crates and `rust/app` front door into a single, cohesive, idiomatic Rust crate with one `Cargo.toml` in `rust/`, maintaining 100% trycmd replay parity (24/24 cases passing) and producing the same standalone 15 MB binary.

**Architecture:** Replace the multi-crate Cargo workspace layout with a standard Rust module hierarchy (`src/storage/`, `src/shield/`, `src/auth/`, `src/quota/`, `src/balancer/`, `src/daemon/`, `src/web/`, `src/cli/`, `src/tray/`, `src/main.rs`). All dependencies are centralized into a single root `rust/Cargo.toml` with BeautifulRust lint configurations.

**Tech Stack:** Rust 2024 / Edition 2021, Tokio, Axum, rust-embed, ksni (DBus SNI), zbus 5 (SecretService), rusqlite, reqwest, image, ratatui, inquire, trycmd.

**Spec:** The architecture and parity requirements defined in [rust/MAP.md](../../../rust/MAP.md) and [rust/DECISIONS.md](../../../rust/DECISIONS.md).

## Global Constraints

- Standalone single binary target: `run_tracker` (or `agy-token`).
- Pure Rust: Zero Python runtime dependencies, zero C/GTK shared library dependencies.
- 100% on-disk schema and format compatibility: `~/.config/antigravity-token-tracker/accounts.json`, `token_history.jsonl`, and SQLite `state.vscdb`.
- 100% trycmd test replay parity: All 24 recorded test cases in `rust/app-cases/` must pass on `cargo test`.
- All `unsafe` blocks must have explicit `// SAFETY:` justifications.

## Review Focus

- **NaN in Quota Comparison**: Ensure all floating-point sorting uses `total_cmp` instead of `.partial_cmp().unwrap()` to prevent crashes.
- **Embedded Asset Resolution**: Ensure `rust-embed` in `src/web/assets.rs` properly resolves `dist/` relative to the crate root.
- **DBus System Tray Registration**: Ensure `ksni` StatusNotifierItem registers without timing out on desktop environments.
- **Process Discovery Path Traversal**: Ensure `/proc` scanning directly reads `DirEntry::file_name()` without redundant `PathBuf` allocations.
- **Trycmd Sandbox Path**: Ensure `tests/app_cases.rs` finds `../app-cases/*.toml` and registers the newly located binary.

---

### Task 1: Scaffolding Root `rust/Cargo.toml` and Module Architecture

**Files:**
- Create: `rust/Cargo.toml`
- Create: `rust/src/lib.rs`
- Create: `rust/src/main.rs`

**Interfaces:**
- Produces: Unified dependency manifest and root crate structure.

- [ ] **Step 1: Write root `rust/Cargo.toml`**
Assemble all unified dependencies and `[lints]` into `rust/Cargo.toml`:
```toml
[package]
name = "run_tracker"
version = "0.1.0"
edition = "2024"
publish = false
description = "Antigravity Token & Quota Lifecycle Tracker"
rust-version = "1.85"

[[bin]]
name = "run_tracker"
path = "src/main.rs"

[lib]
name = "tracker_lib"
path = "src/lib.rs"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.12", features = ["json"] }
rusqlite = { version = "0.33", features = ["bundled"] }
zbus = { version = "5.19", default-features = false, features = ["tokio"] }
axum = { version = "0.8", features = ["tokio"] }
tokio-stream = { version = "0.1", features = ["sync"] }
rust-embed = { version = "8.14", features = ["mime-guess"] }
mime_guess = "2.0"
image = { version = "0.25", default-features = false, features = ["png"] }
ksni = "0.3.7"
fs2 = "0.4"
inquire = "0.9"
ratatui = "0.30"
crossterm = "0.29"
clap = { version = "4", features = ["derive"] }
open = "5.4"
regex = "1.10"
dirs = "5.0"
sysinfo = "0.33"
libc = "0.2"

[dev-dependencies]
trycmd = "1"
tempfile = "3"

[profile.release]
lto = true
codegen-units = 1

[lints.rust]
unsafe_code = "allow"

[lints.clippy]
manual_let_else = "warn"
map_unwrap_or = "warn"
```

- [ ] **Step 2: Scaffold `rust/src/lib.rs` and verify package compilation**
Create `rust/src/lib.rs` declaring the domain modules:
```rust
pub mod auth;
pub mod balancer;
pub mod cli;
pub mod daemon;
pub mod quota;
pub mod shield;
pub mod storage;
pub mod tray;
pub mod web;
```
Run: `cargo check --manifest-path rust/Cargo.toml`  
Expected: PASS (once submodules are populated).

---

### Task 2: Migrate Storage & Geo-Shield Modules

**Files:**
- Create: `rust/src/storage/mod.rs`
- Create: `rust/src/storage/models.rs` (from `rust/core-storage/src/models.rs`)
- Create: `rust/src/storage/storage.rs` (from `rust/core-storage/src/storage.rs`)
- Create: `rust/src/storage/lifecycle.rs` (from `rust/core-storage/src/lifecycle.rs`)
- Create: `rust/src/shield/mod.rs`
- Create: `rust/src/shield/models.rs` (from `rust/geo-shield/src/models.rs`)
- Create: `rust/src/shield/geo.rs` (from `rust/geo-shield/src/geo.rs`)
- Create: `rust/src/shield/shield.rs` (from `rust/geo-shield/src/shield.rs`)

**Interfaces:**
- Produces: `crate::storage::StorageManager`, `crate::storage::models::*`, `crate::shield::geo::*`, `crate::shield::shield::*`.

- [ ] **Step 1: Copy and adapt storage files**
Migrate source files into `rust/src/storage/` adjusting internal imports from `core_storage::*` to `crate::storage::*`.
- [ ] **Step 2: Copy and adapt geo-shield files**
Migrate source files into `rust/src/shield/` adjusting internal imports from `geo_shield::*` to `crate::shield::*`.
- [ ] **Step 3: Port and run storage/shield unit tests**
Include unit tests in `rust/src/storage/mod.rs` and `rust/src/shield/mod.rs`.  
Run: `cargo test --manifest-path rust/Cargo.toml storage shield`  
Expected: All unit tests pass.

---

### Task 3: Migrate Auth & Telemetry Modules

**Files:**
- Create: `rust/src/auth/mod.rs`
- Create: `rust/src/auth/models.rs` (from `rust/auth-discovery/src/models.rs`)
- Create: `rust/src/auth/sqlite.rs` (from `rust/auth-discovery/src/sqlite.rs`)
- Create: `rust/src/auth/keyring.rs` (from `rust/auth-discovery/src/keyring.rs`)
- Create: `rust/src/auth/oauth.rs` (from `rust/auth-discovery/src/oauth.rs`)
- Create: `rust/src/auth/process_discovery.rs` (from `rust/auth-discovery/src/process_discovery.rs`)
- Create: `rust/src/auth/sync.rs` (from `rust/auth-discovery/src/sync.rs`)
- Create: `rust/src/quota/mod.rs`
- Create: `rust/src/quota/models.rs` (from `rust/quota-metrics/src/models.rs`)
- Create: `rust/src/quota/quota.rs` (from `rust/quota-metrics/src/quota.rs`)
- Create: `rust/src/quota/burnrate.rs` (from `rust/quota-metrics/src/burnrate.rs`)

**Interfaces:**
- Produces: `crate::auth::sync::*`, `crate::auth::process_discovery::*`, `crate::quota::quota::*`, `crate::quota::burnrate::*`.

- [ ] **Step 1: Migrate `auth` files with static regex optimization**
Apply the `LazyLock` regex fix to `process_discovery.rs` and update internal references to `crate::storage::*`.
- [ ] **Step 2: Migrate `quota` files**
Update internal references to `crate::storage::*` and `crate::auth::*`.
- [ ] **Step 3: Run unit tests**
Run: `cargo test --manifest-path rust/Cargo.toml auth quota`  
Expected: All unit tests pass.

---

### Task 4: Migrate Balancer & Daemon Modules

**Files:**
- Create: `rust/src/balancer/mod.rs`
- Create: `rust/src/balancer/models.rs` (from `rust/session-balancer/src/models.rs`)
- Create: `rust/src/balancer/switcher.rs` (from `rust/session-balancer/src/switcher.rs`)
- Create: `rust/src/balancer/balancer.rs` (from `rust/session-balancer/src/balancer.rs`)
- Create: `rust/src/daemon/mod.rs`
- Create: `rust/src/daemon/notifier.rs` (from `rust/daemon-notify/src/notifier.rs`)
- Create: `rust/src/daemon/failover.rs` (from `rust/daemon-notify/src/failover.rs`)
- Create: `rust/src/daemon/doctor.rs` (from `rust/daemon-notify/src/doctor.rs`)
- Create: `rust/src/daemon/daemon.rs` (from `rust/daemon-notify/src/daemon.rs`)

**Interfaces:**
- Produces: `crate::balancer::switcher::*`, `crate::balancer::balancer::*`, `crate::daemon::daemon::*`, `crate::daemon::doctor::*`.

- [ ] **Step 1: Migrate `balancer` with `total_cmp` safety fix**
Replace `.partial_cmp().unwrap()` with `total_cmp` in `balancer.rs`.
- [ ] **Step 2: Migrate `daemon` with `total_cmp` safety fix**
Replace `.partial_cmp().unwrap()` with `total_cmp` in `failover.rs`.
- [ ] **Step 3: Run unit tests**
Run: `cargo test --manifest-path rust/Cargo.toml balancer daemon`  
Expected: All unit tests pass.

---

### Task 5: Migrate Web, CLI, and System Tray Modules

**Files:**
- Create: `rust/src/web/mod.rs`
- Create: `rust/src/web/assets.rs`
- Create: `rust/src/web/server.rs`
- Copy: `rust/web-api/dist/` -> `rust/src/web/dist/`
- Create: `rust/src/cli/mod.rs`
- Create: `rust/src/cli/display.rs`
- Create: `rust/src/cli/interactive.rs`
- Create: `rust/src/tray/mod.rs`
- Create: `rust/src/tray/icon.rs`
- Create: `rust/src/tray/applet.rs`
- Create: `rust/src/tray/lock.rs`

**Interfaces:**
- Produces: `crate::web::server::*`, `crate::cli::display::*`, `crate::tray::run_tray`.

- [ ] **Step 1: Migrate `web` with embedded assets**
Copy `dist/` into `rust/src/web/dist/` and configure `#[folder = "src/web/dist/"]`.
- [ ] **Step 2: Migrate `cli` formatting and interactive prompts**
Update internal references to `crate::storage::*`, `crate::quota::*`, etc.
- [ ] **Step 3: Migrate `tray` StatusNotifierItem implementation**
Update internal references to `crate::storage::*`, `crate::web::*`, etc.
- [ ] **Step 4: Run unit tests**
Run: `cargo test --manifest-path rust/Cargo.toml web cli tray`  
Expected: All unit tests pass.

---

### Task 6: Consolidate Main Entrypoint & Trycmd Replay Verification

**Files:**
- Create: `rust/src/main.rs` (integrating subcommands from `rust/app/src/main.rs` and `rust/app/src/units/*`)
- Create: `rust/tests/app_cases.rs` (from `rust/app/tests/app_cases.rs`)

**Interfaces:**
- Produces: Standalone binary `run_tracker`.

- [ ] **Step 1: Write `rust/src/main.rs`**
Directly route subcommands to internal modules without intermediate crate wrappers.
- [ ] **Step 2: Set up `rust/tests/app_cases.rs`**
Configure trycmd to point to `../app-cases/*.toml`.
- [ ] **Step 3: Run trycmd replay suite**
Run: `cargo test --manifest-path rust/Cargo.toml --test app_cases`  
Expected: 24/24 cases pass.
- [ ] **Step 4: Verify with `rust-anything app replay`**
Run: `rust-anything app replay . --build rust/target/debug/run_tracker`  
Expected: 24/24 cases passed, 0.0% fallback rate.

---

### Task 7: Cleanup of Multi-Crate Subdirectories and Final Validation

**Files:**
- Remove: `rust/core-storage/`
- Remove: `rust/geo-shield/`
- Remove: `rust/auth-discovery/`
- Remove: `rust/quota-metrics/`
- Remove: `rust/session-balancer/`
- Remove: `rust/daemon-notify/`
- Remove: `rust/web-api/`
- Remove: `rust/cli/`
- Remove: `rust/tray-applet/`
- Remove: `rust/app/`
- Update: `rust/MAP.md`

- [ ] **Step 1: Archive or delete the old 9 subdirectories**
Remove the 9 sub-crate directories now that their code lives in `rust/src/`.
- [ ] **Step 2: Build release binary**
Run: `cargo build --release --manifest-path rust/Cargo.toml`  
Expected: Compiles cleanly and produces `rust/target/release/run_tracker`.
- [ ] **Step 3: Run live subcommands**
Run: `./rust/target/release/run_tracker ip`  
Run: `./rust/target/release/run_tracker list`  
Run: `./rust/target/release/run_tracker doctor`  
Expected: All output matches previous behavior exactly.
- [ ] **Step 4: Update `rust/MAP.md`**
Record the single-crate structure as the permanent architectural layout.
