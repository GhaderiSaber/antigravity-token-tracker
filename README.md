# ⚡ Antigravity Token & Weekly Quota Lifecycle Tracker

A standalone, multi-account lifecycle monitor and alerting utility for Google Antigravity tokens and weekly quotas.

---

## 🌟 Key Features

- **Multi-Account Profile Registry**:
  - Automatically captures credentials when you switch accounts in Antigravity.
  - Keeps persistent Google OAuth refresh tokens for all accounts in `~/.config/antigravity-token-tracker/accounts.json` (with secure `0600` permissions).
  - **Tracks all accounts concurrently in the background**, even accounts you are currently logged out of.
- **Weekly & 5-Hour Limit Monitoring**:
  - Tracks the exact rolling reset timestamp (`resetTime`) for **Gemini Models** and **Claude/GPT-OSS Models**.
  - Displays human-readable countdowns (e.g. `23h 5m remaining until weekly refresh`).
- **Exhaustion & Finished Token Detection**:
  - Detects when an account's quota is finished (`<= 1.0%` remaining) or low.
  - Automatically recommends alternative accounts that have healthy quota available.
- **⚡ Auto-Failover & Smart Switcher**:
  - Monitors the active session and automatically fails over to the backup account with the highest available quota the moment your tokens hit `0%` (or custom `--threshold`).
  - Features anti-flapping cooldown protection and native desktop alerts (`notify-send`).
- **Native Desktop Notifications (`notify-send`)**:
  - Pops up a system alert when your weekly tokens run out.
  - Pops up a system notification the moment your weekly tokens refresh back to 100%!
- **🖥️ Linux Desktop Top-Bar / System Tray Applet**:
  - Live StatusNotifierItem (SNI) indicator natively integrated into Ubuntu/GNOME's top panel.
  - Dynamic colored quota progress ring and lightning badge.
  - 1-click dropdown menu to hot-swap accounts, trigger auto-failover, and view live quota countdowns.
- **🌐 Egress IP & Country Geolocation Detection**:
  - Live public exit IP, ISO country code, flag emoji, and city displayed directly on the top-bar tray tooltip and menu.
  - **VPN Disconnect & Geo-Block Alerts**: Warns immediately if your IP falls into an Antigravity restricted region (`IR`, `RU`, `BY`, `KP`, `CU`, `SY`, `CN`).
  - Auditable via `agy-token ip`, `agy-token status`, web dashboard, and `agy-token doctor`.
- **🛡️ Antigravity Geo-Shield & Emergency IP Killswitch**:
  - **Instant Account Protection**: Automatically closes Antigravity and Google Language Server processes within < 50ms if your VPN drops or your IP switches to a restricted country.
  - **Relaunch Guard**: Blocks Antigravity from connecting until a safe VPN egress is re-established.
  - **1-Click Tray & Web Toggle**: Turn shield on/off with 1 click directly in the top-bar dropdown or web dashboard.
  - **Custom Policy**: Supports restricted country blacklist or custom allowed country whitelist (`US,DE,GB,...`).
- **Flexible Display Surfaces**:
  - **Top-Bar Tray Indicator**: Always-visible top panel icon with instant switch menu.
  - **Rich Terminal Dashboard**: Beautiful colored progress bars, tables, and countdowns.
  - **Live Watch Mode**: Real-time auto-updating terminal monitor.
  - **Interactive Web UI**: Modern dark-mode browser dashboard at `http://localhost:8765`.
  - **Scriptable Check Mode**: Machine-readable JSON output and exit-codes (0 = healthy, 1 = exhausted) for custom shell prompts and scripts.

---

## 📦 Installation & Setup on Any System

### Requirements
- **OS**: Linux (Ubuntu, Debian, Fedora, Arch, openSUSE, WSL2, etc.) or macOS
- **Rust & Cargo**: Rust 1.85+ (Edition 2024)
- **Antigravity**: Antigravity Desktop App and/or Antigravity IDE installed

### 1. One-Line Automated Install (Recommended)
Clone the repository and run the installer:
```bash
git clone https://github.com/GhaderiSaber/antigravity-token-tracker.git
cd antigravity-token-tracker
./install.sh
```
The installer will:
1. Verify Rust & Cargo are installed.
2. Compile the standalone, optimized release binary (`cargo build --release`).
3. Install the `agy-token` executable to `~/.local/bin/`.
4. Configure, enable, and start the systemd user service (`antigravity-token-tracker.service`), or configure desktop autostart on non-systemd environments.
5. Launch the top-bar tray indicator immediately in the background.

> [!TIP]
> Ensure `~/.local/bin` is in your `PATH` (typically standard on modern Linux). If `agy-token` is not found, add `export PATH="$HOME/.local/bin:$PATH"` to your `~/.bashrc` or `~/.zshrc`.

### 2. Managing the Background Service & Tray Daemon
The installer automatically starts the background service. You can inspect or manage it with standard systemd user commands:

```bash
# Check service and tray status:
systemctl --user status antigravity-token-tracker.service

# View live updating service logs:
journalctl --user -u antigravity-token-tracker.service -f

# Restart background service (e.g. after updating binary or config):
systemctl --user restart antigravity-token-tracker.service

# Stop and disable background service:
systemctl --user disable --now antigravity-token-tracker.service
```

### 3. Manual Build & Install (Alternative)
If you prefer building and installing manually:
```bash
# 1. Compile release binary
cargo build --release

# 2. Install binary to your PATH
mkdir -p ~/.local/bin
cp target/release/run_tracker ~/.local/bin/agy-token
chmod +x ~/.local/bin/agy-token

# 3. Sync initial account
agy-token sync

# 4. Enable and start background tray service
systemctl --user daemon-reload
systemctl --user enable --now antigravity-token-tracker.service
```

---

## 🚀 Quick Start / Commands

### 1. Check Status
Displays current token usage, countdowns, and switch recommendations:
```bash
agy-token status
```
To also see individual model limits:
```bash
agy-token status --models
```

### 2. Live Watch Mode
Full-screen auto-updating terminal dashboard:
```bash
agy-token watch
```

### 3. Web Dashboard
Launches a browser dashboard with live progress meters:
```bash
agy-token web
# Open http://localhost:8765 in your browser
```

### 4. Background Daemon & Zero-Downtime Auto-Failover
Runs as a background service that continuously tracks token life. With `--auto-switch`, it automatically swaps credentials and restarts Antigravity when your active quota exhausts:
```bash
# Monitoring + desktop alerts only:
agy-token daemon

# Zero-downtime Auto-Failover mode (switches session when active quota <= 1.0%):
agy-token daemon --auto-switch

# Custom threshold and cooldown:
agy-token daemon --auto-switch --threshold 2.0 --cooldown 180 --interval 120
```

To enable auto-failover and the top-bar tray as a persistent background service:
```bash
systemctl --user enable --now antigravity-token-tracker.service
```

### 5. Native Linux Top-Bar System Tray Applet
Launch the top panel indicator directly in the terminal (foreground mode for interactive testing):
```bash
agy-token tray
```
Features:
- **Zero-Latency D-Bus Registration**: Connects to the desktop's StatusNotifierWatcher immediately on launch without network blocking.
- **High-Contrast Quota Gauge**: Sharp, auto-scaling color-coded progress ring (Green > 50%, Blue 25–50%, Yellow 10–25%, Red <= 10%) with crisp visibility on dark top panels.
- **1-Click Dropdown Menu**: View active session status, hot-swap accounts, toggle IP killswitch shield, or open the browser dashboard.
- **Singleton Process Lock**: Enforces a single running indicator instance via `~/.config/antigravity-token-tracker/tray.lock` to prevent duplicate tray icons and background resource leaks.

> [!NOTE]
> **Desktop Environment Support**:
> - **Ubuntu (GNOME)**: Supported out of the box via Ubuntu's preinstalled `ubuntu-appindicators` extension.
> - **Vanilla GNOME (Fedora / Arch / Debian)**: Requires the official [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) GNOME Shell extension.
> - **KDE Plasma / XFCE / Cinnamon / Sway / i3**: Natively supported via the standard FreeDesktop StatusNotifierItem (SNI) protocol.

### 6. Multi-Account Management
```bash
# Sync current active account from Antigravity IDE:
agy-token sync

# List all tracked accounts:
agy-token list

# Remove an account profile (by number, name, or email):
agy-token remove 1
agy-token remove other@gmail.com
```

### 7. Fast Account Switcher (Interactive & Typo-Proof)
```bash
# 1. Interactive Menu (Arrow keys ↑/↓ or number keys, Enter to confirm):
agy-token switch

# 2. Number shortcut (no typing email!):
agy-token switch 1
agy-token switch 2

# 3. Partial name / substring (typo-proof):
agy-token switch duzen
agy-token switch saber

# 4. Auto-switch to highest quota without prompting:
agy-token switch --auto

# 5. Hot-swap session state without restarting Antigravity:
agy-token switch 1 --no-restart
```

### 8. Scriptable Check
```bash
# Fast check (exit code 1 if weekly quota exhausted, 0 if healthy):
agy-token check

# JSON format:
agy-token check --json
```

### 9. Account Health & Token Doctor
Audit your Antigravity environment, system keyring, background daemon, and OAuth token validity across all accounts:
```bash
# Run full read-only diagnostic:
agy-token doctor

# Run diagnostics with safe auto-repairs (refreshes tokens, fixes permissions, restarts dead services):
agy-token doctor --fix

# Fast offline check without pinging Google endpoints:
agy-token doctor --no-network
```

### 10. Egress IP & Geolocation Audit
Verify your public egress IP address, country, ISP, and Antigravity compatibility:
```bash
# Check egress IP and region:
agy-token ip

# Bypass cache and force re-query:
agy-token ip --refresh

# Machine-readable JSON output:
agy-token ip --json
```

### 11. Antigravity Geo-Shield & Emergency IP Killswitch
Protect your Google accounts from regional geoblocking and API bans if your VPN disconnects:
```bash
# View shield status, policy, and compliance check:
agy-token shield status

# Enable or disable the killswitch shield:
agy-token shield on
agy-token shield off

# Switch between restricted countries and whitelist modes:
agy-token shield mode restricted
agy-token shield mode whitelist

# Configure allowed countries for whitelist mode:
agy-token shield allow US,DE,GB,NL,CA

# Test emergency termination manually:
agy-token shield trigger
```

---

## 🔧 System Tray & Service Troubleshooting

### 1. "Why didn't the tray show up after installing?"
- **Verify the service status**:
  ```bash
  systemctl --user status antigravity-token-tracker.service
  ```
  If it shows `inactive (dead)`, enable and start it:
  ```bash
  systemctl --user enable --now antigravity-token-tracker.service
  ```
- **Verify GNOME AppIndicator extension**:
  On GNOME Shell (Ubuntu, Fedora, Debian, Arch), verify that the StatusNotifier / AppIndicator extension is active:
  ```bash
  gnome-extensions list --enabled | grep appindicator
  ```
  If not enabled:
  ```bash
  gnome-extensions enable ubuntu-appindicators@ubuntu.com
  # Or on Fedora/Arch: gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com
  ```

### 2. "Why do I see 'Another tray indicator instance is already active (PID ...)'?"
The applet strictly enforces a single instance via an exclusive lockfile at `~/.config/antigravity-token-tracker/tray.lock` to avoid duplicate tray icons.
If an instance is already running (e.g. from systemd or autostart):
- Inspect running processes:
  ```bash
  pgrep -fl agy-token
  ```
- Restart the managed background service:
  ```bash
  systemctl --user restart antigravity-token-tracker.service
  ```
- Or kill any orphan tray processes:
  ```bash
  pkill -f "agy-token tray"
  ```

### 3. "How do I cleanly uninstall?"
To stop all services and remove the binary:
```bash
./uninstall.sh
```

---

## 🔄 1-Click Account Switching (Solution 2)

No need to open a browser, log out, or re-enter 2FA every time an account exhausts its tokens:

1. **Automatic Session Snapshots**: Whenever an account is active in Antigravity or Antigravity IDE, its authenticated cookies and tokens are automatically cached into `~/.config/antigravity-token-tracker/sessions/<email>/`.
2. **Instant Hot-Swap via CLI or Web Dashboard**:
   - In the terminal: run `agy-token switch` (it auto-selects the best account with available quota) or `agy-token switch <email>`.
   - In the Web UI (`http://localhost:8765`): click the **⚡ Switch to this Account** button on any inactive account card.
3. **Seamless Restart**: Antigravity is cleanly terminated and relaunched with the new session loaded in ~2 seconds.

---

## 📂 Configuration & History
- Account credentials: `~/.config/antigravity-token-tracker/accounts.json` (chmod 0600)
- Historical quota log: `~/.gemini/antigravity/token_history.jsonl`

---

## System Overview
Antigravity Token Tracker (`run_tracker` / `agy-token` + `tracker_lib`) is a standalone Rust service, CLI, D-Bus system tray applet, and embedded React SPA web dashboard that monitors Google Antigravity multi-account OAuth sessions, tracks weekly and 5-hour model quotas, estimates burn-rate velocity, rotates accounts across balancing strategies, and enforces outbound IP Geo-Shield killswitch protection.

## Local Interface & Invariants
- `install.sh`: Compiles `cargo build --release`, installs `target/release/run_tracker` to `$HOME/.local/bin/agy-token`, provisions `$HOME/.config/systemd/user/antigravity-token-tracker.service` (`tray --interval 60`) and `$HOME/.config/autostart/agy-token.desktop`, optionally updates shell `PATH` profiles, and smoke-tests `$TARGET_BIN --help`; exits with status 1 if `cargo` is missing or the binary smoke test fails `[install.sh]`
- `uninstall.sh`: Stops/disables `antigravity-token-tracker.service`, terminates running `agy-token` processes, and removes systemd unit, desktop autostart/application entries, CLI binaries/symlinks, and `tray.lock` while preserving `~/.config/antigravity-token-tracker` account data `[uninstall.sh]`
- Crate configuration targets Rust Edition `2024` (`rust-version = "1.85"`) exposing binary `run_tracker` (`src/main.rs`) and library `tracker_lib` (`src/lib.rs`) `[Cargo.toml]`

## Subdirectories & System Boundaries
- [src](./src/README.md): Rust backend core and CLI entrypoint (`run_tracker` / `tracker_lib`) coordinating command dispatch and 9 domain subsystems (`auth`, `balancer`, `cli`, `daemon`, `quota`, `shield`, `storage`, `tray`, `web`)
- [frontend](./frontend/README.md): Vite + Tailwind CSS + React SPA client source (`frontend/src`) built into `src/web/dist` and served by the embedded Axum web server

## Global Invariants
- Core domain types across `auth`, `balancer`, `shield`, `storage`, `quota`, and `daemon` statically satisfy `Send + Sync` at compile time `[src/lib.rs]`
- Account emails are normalized to trimmed lowercase on persistence, lookup, and switching; `accounts.json` and `shield.json` writes use `0o700` config directories, `0o600` `.tmp` files, atomic renames, and advisory `.accounts.lock` file locking for account registry operations `[src/storage/storage.rs:StorageManager::save_accounts, src/shield/shield.rs:save_shield_config]`
- Session switching (`switch_to_account`) snapshots the currently active account before restoring target desktop/IDE credentials and enforces `0o700` session directories `[src/balancer/switcher.rs:switch_to_account]`
- The system tray applet enforces single-instance execution via exclusive `flock` on `tray.lock` (unlocking on `Drop` without unlinking), and the embedded HTTP/SSE server binds strictly to `127.0.0.1:{port}` with localhost-only CORS `[src/tray/lock.rs:TrayLock::acquire, src/web/server.rs:start_web_server]`
- Geo-Shield evaluates egress IP geolocation against `ShieldMode::Restricted` (7 restricted ISO codes) or `ShieldMode::Whitelist` and terminates Antigravity processes (`SIGTERM`, fallback `SIGKILL`) on policy breach when enabled `[src/shield/shield.rs:evaluate_and_enforce_shield]`

## Global Side Effects
- Reads and mutates local Antigravity state (`~/.config/Antigravity`, IDE `state.vscdb`), Secret Service keyring items via D-Bus (`org.freedesktop.secrets`), and `accounts.json` / `balancer.json` / `shield.json` / `token_history.jsonl`
- Performs outbound HTTPS calls to Google OAuth2 (`oauth2.googleapis.com`, `https://www.googleapis.com/oauth2/v3/userinfo`), Google CloudCode PA (`daily-cloudcode-pa.googleapis.com`), and IP geolocation providers (`ipwho.is`, `ipapi.co`), plus local HTTPS RPC calls to `127.0.0.1` Language Server ports
- Spawns/terminates Antigravity OS processes, sends desktop notifications (`org.freedesktop.Notifications` / `notify-send`), registers a StatusNotifierItem on session D-Bus, and serves HTTP/SSE traffic on `127.0.0.1:8765`

## System Verification
- Run: `cargo test`
- Run: `cd frontend && npm run build`
