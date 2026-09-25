#!/usr/bin/env bash
set -e

echo "Uninstalling Antigravity Token Tracker..."

# Stop and disable systemd service if running
if command -v systemctl &>/dev/null; then
    systemctl --user stop antigravity-token-tracker.service 2>/dev/null || true
    systemctl --user disable antigravity-token-tracker.service 2>/dev/null || true
fi

# Remove service file
rm -f "$HOME/.config/systemd/user/antigravity-token-tracker.service"

# Remove desktop entry
rm -f "$HOME/.local/share/applications/antigravity-token-tracker-tray.desktop"

# Remove CLI symlink
rm -f "$HOME/.local/bin/agy-token"

echo "✓ Removed CLI command and service."
echo "Note: Account configurations in ~/.config/antigravity-token-tracker were kept."
echo "To also remove saved accounts, run: rm -rf ~/.config/antigravity-token-tracker"
