#!/usr/bin/env bash
# Install voice-flow: symlink the scripts into ~/.local/bin, build the
# Rust focus helper, and register the dashboard launcher.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$HOME/.local/bin"
mkdir -p "$BIN" "$HOME/.local/share/applications"

for f in voice-flow voice-flow-pill voice-flow-dashboard vibecoding-record; do
    ln -sf "$HERE/bin/$f" "$BIN/$f"
    chmod +x "$HERE/bin/$f"
    echo "linked $f"
done

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
