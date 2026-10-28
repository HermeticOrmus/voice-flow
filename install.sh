#!/usr/bin/env bash
# Install voice-flow: symlink the scripts into ~/.local/bin, build the
# Rust focus helper, and register the dashboard launcher.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HOME/.local/bin"
mkdir -p "$BIN" "$HOME/.local/share/applications"

for f in voice-flow voice-flow-pill voice-flow-dashboard voice-flow-web vibecoding-record voice-agent-ollama voice-train voice-substitute; do
    ln -sf "$HERE/bin/$f" "$BIN/$f"
    chmod +x "$HERE/bin/$f"
    echo "linked $f"
done

# voice-ideas (the listener): its own link and its user unit, installed but never enabled.
# Start it with `systemctl --user start voice-ideas` or from the Sun board.
ln -sf "$HERE/bin/voice-ideas" "$BIN/voice-ideas"
chmod +x "$HERE/bin/voice-ideas"
mkdir -p "$HOME/.config/systemd/user"
cp -n "$HERE/systemd/voice-ideas.service" "$HOME/.config/systemd/user/"
systemctl --user daemon-reload 2>/dev/null || true
echo "installed voice-ideas (unit present, not enabled)"

# Default per-app profiles — only if the user hasn't customized one.
CONF_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/ormus-voice"
mkdir -p "$CONF_DIR"
if [ ! -f "$CONF_DIR/app-profiles.conf" ]; then
    cp "$HERE/config/app-profiles.conf" "$CONF_DIR/app-profiles.conf"
    echo "installed default app-profiles.conf"
fi

if command -v cargo >/dev/null 2>&1; then
    echo "building ormus-active-window…"
    ( cd "$HERE/active-window" && cargo build --release -q )
    cp "$HERE/active-window/target/release/ormus-active-window" "$BIN/ormus-active-window"
    echo "installed ormus-active-window"
else
    echo "WARN: cargo not found — focus interlock disabled (voice-flow still works)"
fi

cp "$HERE/voice-flow-dashboard.desktop" "$HOME/.local/share/applications/"
echo "done. Bind Super+V to voice-flow in COSMIC keyboard shortcuts."
