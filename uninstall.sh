#!/usr/bin/env bash
set -e

echo "Uninstalling Antigravity Token Tracker..."

# Stop and disable systemd service if running
if command -v systemctl &>/dev/null; then
    systemctl --user stop antigravity-token-tracker.service 2>/dev/null || true
    systemctl --user disable antigravity-token-tracker.service 2>/dev/null || true
fi

# Terminate any remaining background processes
pkill -f "agy-token" 2>/dev/null || true

# Remove service file
rm -f "$HOME/.config/systemd/user/antigravity-token-tracker.service"
if command -v systemctl &>/dev/null; then
    systemctl --user daemon-reload 2>/dev/null || true
fi

# Remove desktop autostart entries
rm -f "$HOME/.config/autostart/agy-token.desktop"
rm -f "$HOME/.local/share/applications/antigravity-token-tracker-tray.desktop"

# Remove CLI binaries and symlinks
rm -f "$HOME/.local/bin/agy-token"
rm -f "/usr/local/bin/agy-token" 2>/dev/null || true
rm -f "$HOME/bin/agy-token" 2>/dev/null || true
rm -f "$HOME/.config/antigravity-token-tracker/tray.lock" 2>/dev/null || true

echo "✓ Removed CLI command and service."
echo "Note: Account configurations in ~/.config/antigravity-token-tracker were kept."
echo "To also remove saved accounts, run: rm -rf ~/.config/antigravity-token-tracker"
