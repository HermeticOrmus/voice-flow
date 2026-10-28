#!/usr/bin/env bash
# Install voice-flow: symlink the scripts into ~/.local/bin, build the
# Rust helpers (the panel; the COSMIC focus reader), and register the
# dashboard launcher. Re-run after a pull: the links follow the repo.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HOME/.local/bin"
mkdir -p "$BIN" "$HOME/.local/share/applications"

for f in voice-flow whisper-paste voice-flow-pill voice-flow-dashboard voice-flow-web vibecoding-record \
         voice-rewrite-ollama voice-rewrite-claude voice-agent-ollama voice-train voice-substitute; do
    ln -sf "$HERE/bin/$f" "$BIN/$f"
    chmod +x "$HERE/bin/$f"
    echo "linked $f"
done

# Default per-app profiles — only if the user hasn't customized one.
CONF_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/ormus-voice"
mkdir -p "$CONF_DIR"
if [ ! -f "$CONF_DIR/app-profiles.conf" ]; then
    cp "$HERE/config/app-profiles.conf" "$CONF_DIR/app-profiles.conf"
    echo "installed default app-profiles.conf"
fi

if command -v cargo >/dev/null 2>&1; then
    echo "building voice-flow-panel…"
    ( cd "$HERE/panel" && cargo build --release -q )
    ln -sf "$HERE/panel/target/release/voice-flow-panel" "$BIN/voice-flow-panel"
    echo "linked voice-flow-panel"
    # COSMIC only: reads cosmic-toplevel-info-v1. On Hyprland it cannot connect
    # and voice-flow falls back to hyprctl, so a failed build is not fatal.
    echo "building ormus-active-window…"
    if ( cd "$HERE/active-window" && cargo build --release -q ); then
        cp "$HERE/active-window/target/release/ormus-active-window" "$BIN/ormus-active-window"
        echo "installed ormus-active-window"
    else
        echo "WARN: ormus-active-window did not build (COSMIC-only helper; fine on Hyprland)"
    fi
else
    echo "WARN: cargo not found — voice-flow-panel and ormus-active-window skipped"
fi

cp "$HERE/voice-flow-dashboard.desktop" "$HOME/.local/share/applications/"
echo "done. Bind a key to voice-flow: Hyprland (Omarchy) bindings.lua"
echo '  o.bind("SUPER + H", "Toggle dictation", "voice-flow", { dont_inhibit = true, allow_input_capture = true })'
echo "COSMIC: Settings → Keyboard → Custom Shortcuts."
