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
After=network.target

[Service]
Type=simple
ExecStart=$TARGET_BIN tray --interval 60
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
EOF

# Always configure desktop autostart as a universal fallback
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

# Activate background service via systemd if available, else launch directly
TRAY_STARTED=false
if command -v systemctl &>/dev/null && systemctl --user status &>/dev/null; then
    systemctl --user daemon-reload 2>/dev/null || true
    if systemctl --user enable --now antigravity-token-tracker.service 2>/dev/null; then
        echo "✓ Enabled and started systemd user service: antigravity-token-tracker.service"
        TRAY_STARTED=true
    fi
fi

if [ "$TRAY_STARTED" = false ]; then
    # Launch in background for current session
    nohup "$TARGET_BIN" tray >/dev/null 2>&1 &
    echo "✓ Started background tray indicator in current session (PID $!)"
fi

# 5. Desktop Environment & AppIndicator extension check
if command -v gnome-shell &>/dev/null && command -v gnome-extensions &>/dev/null; then
    if ! gnome-extensions list --enabled 2>/dev/null | grep -qiE 'appindicator|tray'; then
        echo ""
        echo "⚠️  Note: GNOME Shell is running without an AppIndicator / StatusNotifier extension."
        echo "   GNOME requires an extension to display top-bar tray applets."
        echo "   To enable system tray icons, install and activate the extension:"
        echo "     • Ubuntu / Debian: sudo apt install gnome-shell-extension-appindicator"
        echo "     • Fedora:          sudo dnf install gnome-shell-extension-appindicator"
        echo "     • Arch Linux:      sudo pacman -S gnome-shell-extension-appindicator"
        echo "     Then run:          gnome-extensions enable ubuntu-appindicators@ubuntu.com"
        echo "                        (or: gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com)"
        echo ""
    fi
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
