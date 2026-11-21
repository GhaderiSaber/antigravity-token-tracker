# Architecture Decision Records (ADRs)

This document tracks all formal architectural decisions for the Rust port of `antigravity-token-tracker`. Decisions are numbered sequentially and never renumbered.

---

## ADR-001: Unified Standalone Binary Architecture
- **Status**: accepted (2026-11-20)
- **Context**: The existing Python application comprises a CLI (`antigravity_tracker/cli.py`), a background daemon (`failover.py`), an HTTP web server (`server.py`), and a desktop system tray applet (`tray.py`). In Python, these were invoked as separate subcommands or standalone scripts.
- **Decision**: Port the entire application into a single unified binary (`agy-token`) with modular feature flags (`--features tray`). When invoked with subcommands (`status`, `watch`, `web`, `daemon`, `tray`, etc.), the single binary dispatches to the corresponding subsystem.
- **Consequences**:
  - Single zero-dependency executable for Linux and macOS.
  - Eliminates Python 3.8+ runtime, virtualenvs, pip installations, and packaging issues.
  - Headless/server builds can disable the system tray via `default-features = false`.

---

## ADR-002: Embedded Web UI via Compile-Time Assets
- **Status**: accepted (2026-11-20)
- **Context**: The web dashboard relies on a pre-built React + Vite SPA located in `frontend/dist/`, with an HTML template fallback in `templates/dashboard.html`. In Python, static files are read from disk at runtime, which breaks when the package is installed in arbitrary locations without data files.
- **Decision**: Use `rust-embed` to compile `frontend/dist/` directly into the binary at build time. If `frontend/dist/` is missing during development, fall back to embedded `templates/dashboard.html` or runtime disk lookup when debug mode is enabled.
- **Consequences**:
  - Running `agy-token web` serves the full modern dashboard without needing any external asset files on disk.
  - The binary remains completely self-contained.

---

## ADR-003: Pure Rust Linux System Tray via DBus (ksni)
- **Status**: accepted (2026-11-20)
- **Context**: `antigravity_tracker/tray.py` uses `gi.repository` (PyGObject, GTK3, AppIndicator3), `dbus-python`, and `PIL` (Pillow) to draw dynamic icon badges. In C/Python, this introduces heavy GTK and libappindicator shared library dependencies that frequently break across desktop environments (GNOME, KDE, Sway).
- **Decision**: Use `ksni` (pure Rust StatusNotifierItem implementation over `zbus`) and the pure-Rust `image` crate to generate dynamic badge icons in-memory without GTK dependencies.
- **Consequences**:
  - Native integration into modern Linux desktop status panels (GNOME with AppIndicator extension, KDE Plasma, XFCE).
  - Eliminates GTK3/GObject runtime shared library linkage.

---

## ADR-004: HTTP Framework & Real-Time Server-Sent Events (Axum + Tokio)
- **Status**: accepted (2026-11-20)
- **Context**: The Python web server uses Python's standard `http.server.ThreadingHTTPServer` with custom manual HTTP header/SSE formatting and a threading queue.
- **Decision**: Use `axum` built on `tokio` and `tower-http` for the local web server and API layer. Implement `/api/events` using `axum::response::sse::Sse` backed by a `tokio::sync::broadcast` channel.
- **Consequences**:
  - High concurrency, asynchronous non-blocking I/O, robust error handling, and type-safe routing.
  - Standard compatibility with the rust-anything front-door and reverse proxy patterns.

---

## ADR-005: Credential Discovery & Secret Storage Compatibility
- **Status**: accepted (2026-11-20)
- **Context**: The app discovers credentials across multiple sources:
  1. `~/.config/antigravity-token-tracker/accounts.json` (persistent profile registry).
  2. Antigravity IDE SQLite databases (`state.vscdb`, `ItemTable`).
  3. Linux SecretService keyring (`org.freedesktop.secrets`).
  4. Antigravity Desktop App process inspection (`/proc` and `ss -tulpn`).
- **Decision**: Maintain 100% byte-level and path-level compatibility with existing configuration files, SQLite keys, and SecretService items:
  - Use `rusqlite` for `state.vscdb`.
  - Use `zbus` / `secret-service` for SecretService.
  - Maintain exact schema compatibility with `~/.config/antigravity-token-tracker/accounts.json` (including both legacy `{email: acc}` and modern `{"active_email": ..., "accounts": ...}`).
- **Consequences**:
  - Zero migration friction: existing logged-in accounts and tokens continue working immediately without re-authentication.

---

## ADR-006: Terminal Dashboard & Rich Formatting (tabled / ratatui)
- **Status**: accepted (2026-11-20)
- **Context**: `cli.py` uses the Python `rich` library for colorful tables, progress bars, and full-screen watch mode.
- **Decision**: Use `tabled` (or `comfy-table`) with ANSI colors for standard CLI outputs (`status`, `list`, `doctor`, `ip`), `indicatif` for progress bars and spinners, and `ratatui` + `crossterm` for full-screen live watch mode (`agy-token watch`) and the interactive switcher menu.
- **Consequences**:
  - Preserves the visual polish of the Python CLI while reducing execution startup time from ~180ms (Python startup + rich imports) to < 5ms (native Rust).
