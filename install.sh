#!/usr/bin/env bash
set -e

# ⚡ Antigravity Token Tracker Installer (Rust Native)
echo "====================================================="
echo "  ⚡ Installing Antigravity Token & Quota Tracker"
echo "====================================================="

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="$HOME/.local/bin"
TARGET_BIN="$BIN_DIR/agy-token"

# 1. Verify Cargo / Rust
if ! command -v cargo &>/dev/null; then
    echo "❌ Error: Rust & Cargo are required to build the release binary."
    echo "   Please install Rust (curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh) and retry."
    exit 1
fi

echo "✓ Found Cargo $(cargo --version)"

# 2. Build optimized release binary
echo "🔨 Compiling standalone release binary..."
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

# 3. Copy binary to ~/.local/bin
mkdir -p "$BIN_DIR"
cp -f "$SCRIPT_DIR/target/release/run_tracker" "$TARGET_BIN"
chmod +x "$TARGET_BIN"
echo "✓ Installed standalone executable: $TARGET_BIN"

# 4. Service and Background Autostart Configuration
mkdir -p "$HOME/.config/systemd/user"
cat << EOF > "$HOME/.config/systemd/user/antigravity-token-tracker.service"
[Unit]
Description=Antigravity Account Token & Quota Lifecycle Tray Daemon (Rust Native)
After=network.target graphical-session.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=$TARGET_BIN tray --interval 60
Restart=on-failure
RestartSec=15
Environment=DISPLAY=:0
Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/%U/bus
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
EOF

# Determine optimal background runner (systemd user service vs desktop autostart)
if command -v systemctl &>/dev/null && systemctl --user status &>/dev/null; then
    systemctl --user daemon-reload 2>/dev/null || true
    systemctl --user enable --now antigravity-token-tracker.service 2>/dev/null || true
    # Remove redundant autostart entry to prevent duplicate singleton lock conflicts
    rm -f "$HOME/.config/autostart/agy-token.desktop"
    echo "✓ Enabled and started systemd user service: antigravity-token-tracker.service"
else
    # Fallback for non-systemd environments: configure desktop autostart
    mkdir -p "$HOME/.config/autostart"
    cat << EOF > "$HOME/.config/autostart/agy-token.desktop"
[Desktop Entry]
Type=Application
Name=Antigravity Token Tracker
Comment=Top-Bar Tray Indicator and Quota Monitor
Exec=$TARGET_BIN tray
Icon=utilities-system-monitor
Terminal=false
Categories=Utility;System;
X-GNOME-Autostart-enabled=true
EOF
    chmod 644 "$HOME/.config/autostart/agy-token.desktop"
    echo "✓ Configured desktop autostart: $HOME/.config/autostart/agy-token.desktop"
    # Launch immediately in background for current session
    nohup "$TARGET_BIN" tray >/dev/null 2>&1 &
    echo "✓ Started background tray indicator in current session (PID $!)"
fi

# 6. PATH verification
if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
    echo ""
    echo "⚠️  Note: $BIN_DIR is not currently in your \$PATH."
    echo "   To use 'agy-token' directly from anywhere, add this to your ~/.bashrc or ~/.zshrc:"
    echo "   export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
fi

echo ""
echo "✅ Installation complete!"
echo "Run 'agy-token status' or 'agy-token ip' to verify."
