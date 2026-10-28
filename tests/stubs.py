"""Stub binaries for the shell scripts under test.

Each test gets a fresh HOME, XDG dirs and a PATH whose first entry holds
small bash stubs for everything the scripts call outside this repo:
the compositor (hyprctl), input injection (ydotool), the clipboard
(wl-copy, wl-paste), the engine (whisper-paste, curl, whisper-cli, sox,
arecord) and desktop noise (notify-send, paplay). Every stub appends its
arguments to <log>/<name>.log so a test can read what was called with what.
"""
from __future__ import annotations

import stat
import tempfile
import wave
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
BIN = REPO / "bin"

# A pid that never exists on Linux (pid_max is 4194304), so `kill -0`
# fails and the stop phase never signals a real process.
DEAD_PID = 4194300

STUBS = {
    "hyprctl": r'''n=$(cat "$STUB_LOG/hyprctl.calls" 2>/dev/null || echo 0); n=$((n+1)); echo "$n" > "$STUB_LOG/hyprctl.calls"
addr="${STUB_ADDRESS:-0x1}"; [ "$n" -ge "${STUB_FOCUS_MOVE_AT:-999}" ] && addr="${STUB_ADDRESS2:-0x2}"
printf '{"address":"%s","class":"%s","fullscreen":0}\n' "$addr" "${STUB_CLASS:-firefox}"''',
    "ydotool": r'''if [ "$1" = type ]; then cat >> "$STUB_LOG/typed.txt"; fi''',
    "wl-copy": r'''cat >> "$STUB_LOG/clipboard.txt"; printf '\n--\n' >> "$STUB_LOG/clipboard.txt"''',
    "wl-paste": r'''printf 'OLD'
''',
    "whisper-paste": r'''case "${VOICE_PASTE_PHASE:-}" in
  start) mkdir -p "$XDG_RUNTIME_DIR/ormus-voice"; : > "$XDG_RUNTIME_DIR/ormus-voice/recording.state" ;;
  stop)  rm -f "$XDG_RUNTIME_DIR/ormus-voice/recording.state"
         printf '%s' "${VOICE_PASTE_REWRITE_COMMAND-unset}" > "$STUB_LOG/rewrite_cmd.txt"
         printf '%s' "${STUB_TEXT:-}" ;;
esac''',
    "curl": r'''if [ -n "${STUB_CURL_EXIT:-}" ]; then exit "$STUB_CURL_EXIT"; fi
case "$*" in
  *api/version*)  printf '{"version":"0.0"}' ;;
  *api/generate*) printf '%s' "${STUB_OLLAMA_RESPONSE:-}" ;;
  *)              printf '%s' "${STUB_CURL_OUT:-}" ;;
esac''',
    "whisper-cli": r'''printf 'LOCAL'
''',
    "sox": r'''printf 'Samples read:  8000\nRMS     amplitude:     %s\n' "${STUB_RMS:-0.0500}" >&2''',
    "arecord": r'''for last; do :; done; python3 - "$last" <<'PY'
import sys, wave
with wave.open(sys.argv[1], "wb") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000); w.writeframes(b"\0" * 16000)
PY''',
    "notify-send": "",
    "paplay": "",
}


def make_env(tmp: Path, **extra: str) -> dict:
    """A clean environment: temp HOME and XDG dirs, stubs first on PATH."""
    stubs = tmp / "stubs"
    log = tmp / "log"
    for d in (stubs, log, tmp / "home", tmp / "config", tmp / "data", tmp / "run"):
        d.mkdir(parents=True, exist_ok=True)
    for name, body in STUBS.items():
        path = stubs / name
        path.write_text(
            '#!/usr/bin/env bash\nprintf \'%s\\n\' "$*" >> "$STUB_LOG/' + name + '.log"\n' + body + "\n"
        )
        path.chmod(path.stat().st_mode | stat.S_IEXEC)
    env = {
        "PATH": f"{stubs}:/usr/bin:/bin",
        "HOME": str(tmp / "home"),
        "XDG_CONFIG_HOME": str(tmp / "config"),
        "XDG_DATA_HOME": str(tmp / "data"),
        "XDG_RUNTIME_DIR": str(tmp / "run"),
        "STUB_LOG": str(log),
        "LANG": "C.UTF-8",
    }
    env.update(extra)
    return env


def log_lines(env: dict, name: str) -> list[str]:
    path = Path(env["STUB_LOG"]) / f"{name}.log"
    return path.read_text().splitlines() if path.exists() else []


def read_log_file(env: dict, name: str) -> str:
    path = Path(env["STUB_LOG"]) / name
    return path.read_text() if path.exists() else ""


def silent_wav(path: Path, seconds: float = 0.5) -> Path:
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(16000)
        w.writeframes(b"\0" * int(16000 * 2 * seconds))
    return path


def tmpdir() -> Path:
    return Path(tempfile.mkdtemp(prefix="voice-flow-test-"))
