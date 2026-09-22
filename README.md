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
- **Python**: 3.8 or higher
- **Antigravity**: Antigravity Desktop App and/or Antigravity IDE installed

### 1. One-Line Automated Install (Recommended)
Clone the repository and run the installer:
```bash
git clone https://github.com/GhaderiSaber/antigravity-token-tracker.git
cd antigravity-token-tracker
./install.sh
```
The installer will:
1. Verify Python 3 installation.
2. Install the lightweight `rich` dependency via `pip`.
3. Create the `agy-token` global executable command in `~/.local/bin/`.
4. Generate an automated systemd user service configured for your user.
5. Perform initial discovery and sync of your active Antigravity session.

> [!TIP]
> Ensure `~/.local/bin` is in your `PATH` (typically standard on modern Linux). If `agy-token` is not found, add `export PATH="$HOME/.local/bin:$PATH"` to your `~/.bashrc` or `~/.zshrc`.

### 2. Manual Installation (Alternative)
If you prefer not to use the installer script:
```bash
# 1. Install dependencies
pip install -r requirements.txt

# 2. Link executable to your PATH
mkdir -p ~/.local/bin
ln -sf "$(pwd)/run_tracker.py" ~/.local/bin/agy-token
chmod +x run_tracker.py

# 3. Sync initial account
agy-token sync
```

Or install as an editable Python package:
```bash
pip install -e .
```

### 3. Background Notifications (Systemd Service)
To receive native desktop notifications (`notify-send`) when tokens exhaust or when the weekly quota refreshes to 100%:
```bash
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
Launch the top panel indicator directly:
```bash
agy-token tray
```
Features:
- Live dynamic icon with color-coded quota gauge.
- Click to view full account breakdown, switch accounts in 1-click, and access the web dashboard.
- Starts automatically on desktop login via `~/.config/autostart/`.

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
