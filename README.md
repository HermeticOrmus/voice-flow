# voice-flow

Wispr Flow clone for Pop!_OS / COSMIC (Wayland). System-wide voice dictation:
hold-free toggle, speak into **any** focused app, GPU whisper.cpp, optional
local LLM cleanup, a focus-proof feedback pill, and a native control panel.

## Components (`~/.local/bin/`)

| Binary | What it is |
|--------|-----------|
| `voice-flow` | Orchestrator. Toggle record → transcribe (via `whisper-paste`) → inject with ydotool. Bound to **Super+V**. |
| `whisper-paste` | The engine: arecord → silence gate → whisper.cpp (GPU) → hallucination filter → optional LLM rewrite. |
| `voice-flow-pill` | GTK4 layer-shell waveform pill (focus-proof feedback). |
| `voice-flow-dashboard` | GTK4 control panel: live status, today's stats, method/model/rewrite/pill toggles, last transcript. |
| `ormus-active-window` | Rust helper — prints the COSMIC focused window via `cosmic-toplevel-info-v1`. Powers the focus interlock. |
| `voice-rewrite-ollama` / `voice-rewrite-claude` | Optional Wispr-style AI cleanup commands. |

## Focus interlock (the window-switch fix)

`ydotool` injects into whatever window has focus at inject time. If you switch
windows during the ~1–3s transcription, text would land in the wrong window.
`voice-flow` captures the focused window (sorted set of activated toplevel ids,
via `ormus-active-window --list`) before transcription and re-checks after; if
focus moved, it **diverts the transcript to the clipboard** and notifies you to
paste, instead of typing into the wrong window. Degrades safely: if the helper
is absent, it behaves as before (always inject).

## Config

`~/.config/ormus-voice/voice-flow.conf` (shell `KEY=value`, sourced each run,
written by the dashboard): `VOICE_FLOW_METHOD`, `VOICE_PASTE_MODEL`,
`VOICE_FLOW_REWRITE`, `VOICE_FLOW_PILL`.

## History / stats

`~/.local/share/ormus-voice/history.jsonl` — one privacy-respecting line per
dictation (counts only, never transcript text): `ts, words, chars, model,
method, rewrite, transcribe_ms, diverted`. The dashboard reads it for "today".

## Install

```sh
./install.sh            # symlinks bin/* into ~/.local/bin, builds active-window
```

Bind **Super+V** to `voice-flow` in COSMIC Settings → Keyboard → Custom Shortcuts.
