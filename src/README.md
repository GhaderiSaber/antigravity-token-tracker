# Rust Backend Core & CLI Entrypoint (`src`)

## Overview
Root package for the Rust implementation of Antigravity Token Tracker (`run_tracker` binary and `tracker_lib` library). Coordinates the command-line interface, argument routing, fallback logging, and exposes the 9 domain subsystems.

## Interface
- `pub mod auth`: Authentication, session discovery, and OAuth2 refresh `[src/lib.rs:auth]`
- `pub mod balancer`: Multi-account quota balancing and atomic switching `[src/lib.rs:balancer]`
- `pub mod cli`: Terminal dashboard formatting and interactive menus `[src/lib.rs:cli]`
- `pub mod daemon`: Background monitoring, doctor suite, and auto-failover `[src/lib.rs:daemon]`
- `pub mod quota`: Quota extraction, caching, and burn rate velocity calculation `[src/lib.rs:quota]`
- `pub mod shield`: Egress IP geolocation detection and emergency killswitch `[src/lib.rs:shield]`
- `pub mod storage`: Local persistence for accounts registry and historical logs `[src/lib.rs:storage]`
- `pub mod tray`: System tray StatusNotifierItem (SNI) applet `[src/lib.rs:tray]`
- `pub mod web`: Axum HTTP REST server and Server-Sent Events service `[src/lib.rs:web]`

### Subsystem Contracts
- [auth](./auth/README.md): Authentication and session discovery
- [balancer](./balancer/README.md): Quota balancing algorithms and process switching
- [cli](./cli/README.md): Terminal dashboard formatting and interactive menus
- [daemon](./daemon/README.md): Background monitoring daemon and failover loop
- [quota](./quota/README.md): Quota fetching, model limits, and velocity metrics
- [shield](./shield/README.md): Geo-Shield egress monitor and process killswitch
- [storage](./storage/README.md): Accounts registry and lifecycle persistence
- [tray](./tray/README.md): Linux top-bar StatusNotifierItem applet
- [web](./web/README.md): Embedded Axum REST API and web dashboard server

## Invariants
- Shield subcommand action must match one of `[status, on, enable, off, disable, mode, allow, action, trigger]` or command exits with code 2 `[src/main.rs:run_shield]`
- Shield mode parameter must be `'restricted'` or `'whitelist'` or command exits with code 1 `[src/main.rs:run_shield]`
- Shield allow parameter requires at least one non-empty country code or command exits with code 1 `[src/main.rs:run_shield]`
- Shield action parameter must be `'kill_process'`, `'kill_network'`, or `'both'` or command exits with code 1 `[src/main.rs:run_shield]`
- Check command exits with code 1 if any account quota is exhausted (`quota_snap.is_gemini_exhausted == Some(true)`), otherwise exits with SUCCESS `[src/main.rs:run_pace_check]`
- Doctor command exits with SUCCESS (code 0) only if all diagnostic test items report status `'PASS'`, otherwise exits with code 1 `[src/main.rs:run_daemon_doctor]`
- CLI returns `ExitCode::SUCCESS` when arguments count is <= 1 or `'-h'`/`'--help'` is requested `[src/main.rs:main]`
- CLI exits with code 1 when argument parsing fails or command is unrecognized `[src/main.rs:main]`

## Side Effects
- Creates or appends to log file at path specified by `RUST_ANYTHING_FALLBACK_LOG` `[src/main.rs:report_fallbacks]`
- Writes updated shield configuration to disk `[src/main.rs:run_shield]`
- Terminates active Antigravity processes using SIGKILL/SIGTERM `[src/main.rs:run_shield]`
- Performs external network HTTP queries to fetch egress IP geolocation and quota snapshots `[src/main.rs:run_shield]`
- Synchronizes active sessions with local SQLite databases and persistent JSON storage `[src/main.rs:run_sync]`
- Binds TCP listener and serves HTTP web dashboard on specified port `[src/main.rs:run_web]`
- Initializes D-Bus connection and runs KSNI system tray event loop `[src/main.rs:run_tray]`
- Writes CLI outputs and formatted error messages to standard output and standard error `[src/main.rs:main]`

## Verification
- Run: `cargo test`
