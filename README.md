# voice-flow

Wispr Flow clone for Pop!_OS / COSMIC (Wayland). System-wide voice dictation:
hold-free toggle, speak into **any** focused app, GPU whisper.cpp, optional
local LLM cleanup, a focus-proof feedback pill, and a native control panel.

## Components (`~/.local/bin/`)

| Binary | What it is |
|--------|-----------|
| `voice-flow` | Orchestrator. Toggle record → transcribe (via `whisper-paste`) → inject with ydotool. Bound to **Super+V**. |
| `whisper-paste` | The engine: arecord (48 kHz) → silence gate → mic tuning (`vibecoding-record`) → whisper (remote-first, see below) → hallucination filter → optional LLM rewrite. |
| `vibecoding-record` | Mic tuning stage: high-pass, noise reduction, compression, presence EQ → clean 16 kHz mono WAV for whisper.cpp. If absent, `whisper-paste` skips tuning and uses the raw capture. |
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

## corvin-mini offload (remote-first inference)

Sun's 12 GB 5070 Ti is contended (scribe's resident whisper-server +
batch transcription jobs), which starved dictation: whisper-cli
cold-started per call and the ollama rewrite fell to CPU (47 s → 8 s
timeout → raw paste). Both inference stages now run on corvin-mini
(Radeon 8060S, 64 GB VRAM budget) with models resident:

- **Transcribe**: whisper.cpp `whisper-server` systemd service on
  `mini:8090`, medium.en pinned. ~0.4 s for 11 s of audio, measured
  from Sun over the Thunderbolt link.
- **Rewrite**: mini's ollama (`llama3.2:3b`, `keep_alive -1`,
  `num_ctx 4096`, 100 % GPU). ~0.3 s warm.

Both scripts probe hosts in order — Thunderbolt `10.55.0.2` → HQ LAN
`10.10.90.174` → Tailscale `100.107.202.72` — and fall back to local
inference (whisper-cli) / raw passthrough (rewrite) when nothing
answers. A fully-dead remote costs ~0.4 s of probing. Override with
`VOICE_PASTE_REMOTE=off`, `VOICE_PASTE_REMOTE_HOSTS`, `OLLAMA_HOSTS`.

Mini side lives at `corvin-mini:~/tools/whisper.cpp` (Vulkan build;
`glslc` copied from Sun, `spirv-headers` from apt) with models in
`~/.local/share/whisper-models/` and the unit at
`/etc/systemd/system/whisper-server.service`. The TB peer IP is the
NetworkManager profile `tb-sun` (`10.55.0.2/30`, autoconnect).

Note: the dashboard's model toggle governs the **local fallback**
only — the server's resident model (medium.en) wins on the remote
path.

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
