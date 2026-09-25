#!/usr/bin/env bash
set -e

# ⚡ Antigravity Token Tracker Installer
echo "====================================================="
echo "  ⚡ Installing Antigravity Token & Quota Tracker"
echo "====================================================="

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$HOME/.local/bin"
TARGET_BIN="$BIN_DIR/agy-token"

# 1. Verify Python 3
if ! command -v python3 &>/dev/null; then
    echo "❌ Error: Python 3 is required but not installed."
    echo "   Please install python3 (e.g. sudo apt install python3) and retry."
    exit 1
fi

PY_VER=$(python3 -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')")
echo "✓ Found Python $PY_VER"

# 2. Install Python Dependencies
echo "📦 Checking required dependencies (rich)..."
if python3 -c "import rich" &>/dev/null; then
    echo "✓ Required dependencies already satisfied (rich found)."
elif [ -n "$VIRTUAL_ENV" ]; then
    echo "📦 Installing dependencies into active virtual environment..."
    python3 -m pip install -r "$SCRIPT_DIR/requirements.txt" --quiet
    echo "✓ Installed required dependencies into virtual environment."
elif python3 -m pip install --user -r "$SCRIPT_DIR/requirements.txt" --quiet 2>/dev/null; then
    echo "✓ Installed required dependencies."
elif python3 -m pip install --user --break-system-packages -r "$SCRIPT_DIR/requirements.txt" --quiet 2>/dev/null; then
    echo "✓ Installed required dependencies (--break-system-packages)."
else
    echo "⚠️  Could not automatically install dependencies due to system package management restrictions (PEP 668)."
    echo "   Please install 'rich' via your package manager:"
    echo "     sudo apt install python3-rich"
    echo "   or run:"
    echo "     pip install --user --break-system-packages -r requirements.txt"
    exit 1
fi

# 3. Create CLI symlink in ~/.local/bin
mkdir -p "$BIN_DIR"
chmod +x "$SCRIPT_DIR/run_tracker.py"

ln -sf "$SCRIPT_DIR/run_tracker.py" "$TARGET_BIN"
echo "✓ Created executable symlink: $TARGET_BIN"

# 4. PATH verification
if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
    echo ""
    echo "⚠️  Note: $BIN_DIR is not currently in your \$PATH."
    echo "   To use 'agy-token' directly from anywhere, add this to your ~/.bashrc or ~/.zshrc:"
    echo "   export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
fi

# 5. Optional Systemd Background Daemon setup
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"
mkdir -p "$SYSTEMD_USER_DIR"
USER_SERVICE_FILE="$SYSTEMD_USER_DIR/antigravity-token-tracker.service"

CURRENT_UID=$(id -u)
cat << EOF > "$USER_SERVICE_FILE"
[Unit]
Description=Antigravity Account Token & Quota Lifecycle Daemon
After=network.target

[Service]
Type=simple
# Zero-downtime auto-failover, top-bar indicator, and desktop alerts
ExecStart=$TARGET_BIN daemon --interval 300 --auto-switch --tray
Restart=on-failure
RestartSec=30
Environment=DISPLAY=:0
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$CURRENT_UID/bus
Environment=PYTHONUNBUFFERED=1

[Install]
WantedBy=default.target
EOF

echo "✓ Created systemd service template: $USER_SERVICE_FILE"

# 5b. Autostart & Tray Deduplication
# Since the systemd service runs the unified daemon + tray indicator with single-instance locking,
# clean up any legacy standalone autostart desktop entry so GNOME doesn't spawn redundant processes.
if [ -f "$HOME/.config/autostart/antigravity-token-tracker-tray.desktop" ]; then
    rm -f "$HOME/.config/autostart/antigravity-token-tracker-tray.desktop"
    echo "✓ Cleaned up redundant autostart entry ~/.config/autostart/antigravity-token-tracker-tray.desktop (systemd daemon handles tray)"
fi

# 5c. Desktop Application Menu Entry
DESKTOP_APPS_DIR="$HOME/.local/share/applications"
mkdir -p "$DESKTOP_APPS_DIR"
cp "$SCRIPT_DIR/antigravity-token-tracker-tray.desktop" "$DESKTOP_APPS_DIR/antigravity-token-tracker-tray.desktop"
echo "✓ Installed desktop application entry: $DESKTOP_APPS_DIR/antigravity-token-tracker-tray.desktop"

# Reload systemd daemon if available
if command -v systemctl &>/dev/null; then
    systemctl --user daemon-reload 2>/dev/null || true
fi

# 6. Initial Account Discovery
echo ""
echo "🔍 Discovering active Antigravity sessions..."
"$TARGET_BIN" sync || true

echo ""
echo "====================================================="
echo "  🎉 Installation Complete!"
echo "====================================================="
echo ""
echo "Quick Commands:"
echo "  • agy-token status       View current token usage and countdowns"
echo "  • agy-token switch       Interactively choose and switch accounts"
echo "  • agy-token tray         Launch top-bar system tray indicator"
echo "  • agy-token web          Launch the browser dashboard (http://localhost:8765)"
echo "  • agy-token watch        Live terminal monitor"
echo ""
echo "Optional - Enable automatic background notifications & top-bar tray:"
echo "  systemctl --user daemon-reload"
echo "  systemctl --user enable --now antigravity-token-tracker.service"
echo ""
