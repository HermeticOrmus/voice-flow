# voice-flow

Wispr Flow clone for Linux Wayland (Omarchy/Hyprland on Sun; COSMIC before).
System-wide voice dictation: hold-free toggle, speak into **any** focused
app, a resident GPU whisper server, optional local LLM cleanup with a
chosen tone, a focus-proof feedback pill, and native control windows.

Install: `./install.sh` links `bin/*` into `~/.local/bin` (the repo is what
runs), builds the Rust helpers, and prints the key bind. On Sun the bind is
**Super+H** (`~/.config/hypr/bindings.lua`); Super+V stays universal paste.

## Components (`~/.local/bin/`)

| Binary | What it is |
|--------|-----------|
| `voice-flow` | Orchestrator. Toggle record → transcribe (via `whisper-paste`) → inject with ydotool (raw keycodes; terminals and `org.ormus.workboard.*` windows get Ctrl+Shift+V). Bound to **Super+H**. `voice-flow agent` treats the dictation as a command — the LLM's answer is what gets injected (no key on Hyprland yet). |
| `whisper-paste` | The engine: arecord (48 kHz) → silence gate → mic tuning (`vibecoding-record`) → whisper (remote-first, see below) → hallucination filter → optional LLM rewrite. |
| `vibecoding-record` | Mic tuning stage: high-pass, noise reduction, compression, presence EQ → clean 16 kHz mono WAV for whisper.cpp. If absent, `whisper-paste` skips tuning and uses the raw capture. |
| `voice-flow-pill` | GTK4 layer-shell waveform pill (focus-proof feedback). |
| `voice-flow-panel` | Rust GTK4 window: Express (formal, casual, verbatim), dictionary, substitutions, last paste. Writes `VOICE_FLOW_EXPRESS` in place and keeps every other conf key. |
| `voice-flow-dashboard` | Python GTK4 control panel: live status, today's stats, engine card, toggles, last transcript. Its controls move into `voice-flow-panel`. |
| `voice-flow-web` | Rich analytics dashboard: charts, streaks, time-saved, training data management (vocabulary + substitutions CRUD), engine status, settings, opt-in transcript history. Stdlib server, localhost only, zero external requests. Runs on Sun (`http://127.0.0.1:7385`, GTK Analytics button) AND on Mars (corvin-mini), where it backs the portal. |

## The portal — voice.ormus.solutions

Served **from Mars** (corvin-mini, always-on at HQ) behind a Cloudflare
Tunnel (`voice-portal`) + CF Access (Ormus emails only, 30-day
sessions). The voice data lives on Sun and syncs to Mars via Syncthing
(folders `voice-data` = `~/.local/share/ormus-voice` minus `models/`,
`voice-conf` = `~/.config/ormus-voice`) — two-way, so vocabulary and
settings edited in the portal flow back to Sun where dictation reads
them. Mini-side units: user systemd `voice-flow-web.service` +
`voice-portal-tunnel.service` (aiadmin, linger on). Sun keeps its own
local `voice-flow-web.service` for the GTK Analytics button; Sun's
tunnel unit is disabled. `backend.state` lives in the data dir (not
runtime) precisely so the portal's engine card syncs.
| `ormus-active-window` | Rust helper — prints the COSMIC focused window via `cosmic-toplevel-info-v1`. On Hyprland it cannot connect; per-app profiles read `hyprctl activewindow` instead and the focus interlock fails open. |
| `voice-rewrite-ollama` / `voice-rewrite-claude` | Optional Wispr-style AI cleanup commands. |
| `voice-agent-ollama` | Agent mode: the transcript is an instruction, stdout is the answer. `VOICE_AGENT_MODEL` picks the model (default llama3.2:3b on mini; try `ornith-9b` for harder asks). |

## voice-ideas: the listener for idle thoughts

Ormus, while he games: "I can just speak into the mic, I can turn some process on, and that hears
everything that I'm saying ... talking a little bit, and then stopping, and then starting."

`voice-ideas` listens on the default source, cuts speech at a 1.5 s pause (silero VAD, ONNX on
onnxruntime, one thread), sends each utterance to whisper-server, and appends one JSON line per
utterance to `~/.local/share/ormus-voice/ideas/utterances.jsonl`. It never writes into a window
(that is `voice-flow`), it keeps no audio (a clip lives in memory until whisper answers, or in
`ideas/spool/` while no server answers), and it drops whatever is spoken while a Super+H dictation
records. The user unit `voice-ideas.service` is installed by `install.sh` and never enabled: start
it with `systemctl --user start voice-ideas` (or from the Sun board), stop it the same way.

| Conf key (env, then `voice-flow.conf`) | Default | Meaning |
|---|---|---|
| `VOICE_IDEAS_SOURCE` | the default source | PipeWire source name (`pactl list sources short`) |
| `VOICE_IDEAS_HOSTS` | Sun :8093, then sg | whisper-server hosts, first that answers wins |
| `VOICE_IDEAS_PAUSE_MS` | 1500 | the pause that ends an utterance |

`voice-ideas --replay f.raw` runs the same loop over a raw s16le 16 kHz mono file; `--drain`
transcribes the spool. The mechanism was settled by the spike in `docs/spikes/vad-game-capture.md`
(branch `spike/vad-game-capture`): silero cut 0 game segments in 310 s of a loud room and 12 of 12
of his utterances on the DJI; the thread pools must be pinned or they spin a whole core; whisper's
`avg_logprob` under -1.5 marks junk. `./check.sh` runs the tests (uv supplies the packages).

## Per-app profiles

`~/.config/ormus-voice/app-profiles.conf` maps focused `app_id` globs to
a profile: `verbatim` (raw transcript, no LLM — terminals, where a
rewrite mangles paths and flags) or `clean` (full cleanup). Unlisted
apps get `clean`. Find ids with `ormus-active-window --list`.

## Rewrite hardening

The cleanup prompt treats the speaker as never talking to the model:
dictated questions get written down, not answered; dictated
instructions get transcribed, not executed. Self-corrections ("wait
no, scratch that") collapse to the corrected version; spoken
punctuation/numbers/dates convert to written forms. The script also
strips echoed transcript tags, aborts over-editing (output length far
from input length → raw paste), and feeds the last few `voice-correct`
journal entries as few-shot examples so recurring mishearings fix
themselves.

## Training the stack on your terms

Three layers, compounding:

1. **Vocabulary biasing** (`~/.config/ormus-voice/vocabulary.txt`) —
   terms are fed to whisper as its decode prompt, so it *hears* them
   more readily. First line of defense.
2. **Learned substitutions** (`~/.config/ormus-voice/substitutions.tsv`)
   — deterministic heard→canonical fixes applied by `voice-substitute`
   after transcription, before any LLM. Works in every mode, including
   verbatim/terminal where the rewrite is skipped.
3. **Rewrite few-shot** — the corrections journal's latest pairs ride
   into the cleanup prompt as examples.

`voice-train` builds layers 1+2 from your actual voice: for each term
it records you saying it, transcribes RAW (no biasing — captures the
true mishearing), and learns the pair when whisper gets it wrong.

```bash
voice-train Embry0 Alqvimia McLeod   # enroll specific terms
voice-train                          # re-run everything in vocabulary.txt
voice-train status                   # what's learned
voice-train distill                  # corrections journal → rules
```

Day-to-day: when a paste comes out wrong, `voice-correct "what I
meant"` logs the pair; `voice-train distill` turns the journal into
rules.

## Live partials (streaming)

While you speak, the pill re-transcribes its own mic tap every ~1.2 s
against the same whisper server as the final pass and shows the words
landing in real time. Feedback only — the injected text still comes from
the stop-time pass. Sliding 15 s window, non-speech annotations stripped,
disabled automatically when no host answers. Toggle: `VOICE_FLOW_PARTIALS`
(conf or web settings).

## Pill states

Gold = dictation, silver = agent mode. While listening the bars follow the
mic and turn copper in the last 15 s before the 120 s cap; after injection
the pill shows a one-line preview of exactly what landed; on a focus divert
it shows the text plus "copied — Ctrl+V".

## Focus interlock (the window-switch fix)

`ydotool` injects into whatever window has focus at inject time. If you switch
windows during the ~1–3s transcription, text would land in the wrong window.
`voice-flow` captures the focused window before transcription and re-checks
after; if focus moved, it **diverts the transcript to the clipboard** and
notifies you to paste, instead of typing into the wrong window. Degrades
safely: when no focus reader answers, it behaves as before (always inject).
Today the reader is `ormus-active-window --list` (COSMIC); on Hyprland it
cannot connect, so the guard is off there until it reads `hyprctl`.

## Engine (remote-first inference)

Speech goes to a resident whisper server, never to a cold `whisper-cli`
and never to Sun's iGPU (it drives the displays; inference there hangs the
desktop). Order, first that answers wins:

- `127.0.0.1:8093` — Sun, faster-whisper large-v3-turbo on the RTX 5070 Ti
  (`~/tools/whisper-server`, user unit `whisper-server.service`).
- `10.10.90.145:8093` (HQ LAN), then `100.66.108.108:8093` (Tailscale) — sg.
- `whisper-cli` on the CPU as the last resort (`VOICE_PASTE_GPU=0`).

Rewrite and agent mode probe sg's Ollama on `:11434` in that same order;
the rewrite model is `qwen3:32b` (think off, resident, 15 s timeout).
Raw passthrough when no host answers. Override with `VOICE_PASTE_REMOTE=off`,
`VOICE_PASTE_REMOTE_HOSTS`, `OLLAMA_HOSTS`, `VOICE_REWRITE_MODEL`.

The dashboard's model toggle governs the **local fallback** only.

## Config

`~/.config/ormus-voice/voice-flow.conf` (shell `KEY=value`, sourced each run;
written by the panel, the dashboard and the web settings): `VOICE_FLOW_METHOD`,
`VOICE_PASTE_MODEL` (local fallback), `VOICE_FLOW_REWRITE`, `VOICE_FLOW_EXPRESS`
(formal, casual, verbatim), `VOICE_FLOW_PILL`, `VOICE_FLOW_PARTIALS`,
`VOICE_FLOW_LOG_TEXT` (1 keeps transcript text in the history log).

## History / stats

`~/.local/share/ormus-voice/history.jsonl` — one privacy-respecting line per
dictation (counts only, never transcript text): `ts, words, chars, model,
method, rewrite, transcribe_ms, diverted`. The dashboard reads it for "today".

## Install

```sh
./install.sh            # links bin/* into ~/.local/bin, builds panel + active-window
```

Omarchy/Hyprland: in `~/.config/hypr/bindings.lua`

```lua
o.bind("SUPER + H", "Toggle dictation", "voice-flow", { dont_inhibit = true, allow_input_capture = true })
```

COSMIC: bind the key to `voice-flow` in Settings → Keyboard → Custom Shortcuts.

## Tests

```sh
python3 -m unittest discover -s tests -v   # stubbed compositor, injector, clipboard, engine
cd panel && cargo test --release           # the panel's store
```
