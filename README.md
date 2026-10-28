# voice-flow

Wispr Flow clone for Pop!_OS / COSMIC (Wayland). System-wide voice dictation:
hold-free toggle, speak into **any** focused app, GPU whisper.cpp, optional
local LLM cleanup, a focus-proof feedback pill, and a native control panel.

## Components (`~/.local/bin/`)

| Binary | What it is |
|--------|-----------|
| `voice-flow` | Orchestrator. Toggle record → transcribe (via `whisper-paste`) → inject with ydotool. Bound to **Super+V**. `voice-flow agent` (**Super+Shift+V**) treats the dictation as a command — the LLM's answer is what gets injected. |
| `whisper-paste` | The engine: arecord (48 kHz) → silence gate → mic tuning (`vibecoding-record`) → whisper (remote-first, see below) → hallucination filter → optional LLM rewrite. |
| `vibecoding-record` | Mic tuning stage: high-pass, noise reduction, compression, presence EQ → clean 16 kHz mono WAV for whisper.cpp. If absent, `whisper-paste` skips tuning and uses the raw capture. |
| `voice-flow-pill` | GTK4 layer-shell waveform pill (focus-proof feedback). |
| `voice-flow-dashboard` | GTK4 control panel: live status, today's stats, engine card, toggles, last transcript. Quick controls. |
| `voice-flow-web` | Rich analytics dashboard: charts, streaks, time-saved, training data management (vocabulary + substitutions CRUD), engine status, settings, opt-in transcript history. Stdlib server, localhost only, zero external requests. Runs on Sun (`http://127.0.0.1:7385`, GTK Analytics button) AND on corvin-mini, where it backs the public portal. |

## The portal — voice.ormus.solutions

Served **from corvin-mini** (always-on at HQ) behind a Cloudflare
Tunnel (`voice-portal`) + CF Access (Ormus emails only, 30-day
sessions). The voice data lives on Sun and syncs to mini via Syncthing
(folders `voice-data` = `~/.local/share/ormus-voice` minus `models/`,
`voice-conf` = `~/.config/ormus-voice`) — two-way, so vocabulary and
settings edited in the portal flow back to Sun where dictation reads
them. Mini-side units: user systemd `voice-flow-web.service` +
`voice-portal-tunnel.service` (aiadmin, linger on). Sun keeps its own
local `voice-flow-web.service` for the GTK Analytics button; Sun's
tunnel unit is disabled. `backend.state` lives in the data dir (not
runtime) precisely so the portal's engine card syncs.
| `ormus-active-window` | Rust helper — prints the COSMIC focused window via `cosmic-toplevel-info-v1`. Powers the focus interlock and per-app profiles. |
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

When 20 s pass after the last utterance, the listener runs `voice-ideas-sort`: the burst (utterances
under 20 s apart) goes to a model that splits it into separate ideas, cleans the words without a
paraphrase, and picks for each the one agent it belongs to from the rows the Sun board shows (Sun's
herdr and sg's workboards), or none. One line per note in `ideas/notes.jsonl` (`id`, `burst`,
`utterances`, `text`, `target` or null, `why`). No model, no board, bad JSON: the burst still lands
as one raw note. `voice-ideas-sort --dry` prints the exchange and writes nothing.

| Conf key | Default | Meaning |
|---|---|---|
| `VOICE_IDEAS_SORTER` | `ollama` | `ollama` (sg, JSON mode, no thinking) or `claude` |
| `VOICE_IDEAS_MODEL` | `qwen3.6:35b-a3b-q4_K_M` | the Ollama model (resident on sg) |
| `OLLAMA_HOSTS` | sg on the HQ LAN, then Tailscale | base URLs, first that answers wins |
| `VOICE_IDEAS_CLAUDE_MODEL` | `claude-haiku-4-5` | with `ANTHROPIC_API_KEY`, `ANTHROPIC_API_KEY_FILE` or `~/.credentials/anthropic/ormus` |

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
against a second, fast whisper-server on mini (`:8091`, base.en,
50–130 ms per call) and shows the words landing in real time. Feedback
only — the injected text still comes from medium.en on stop. Sliding
15 s window, non-speech annotations stripped, disabled automatically
when no partial host answers. Toggle: `VOICE_FLOW_PARTIALS` (conf or
web settings).

## Pill states

Gold = dictation, silver = agent mode. While listening the pill shows
an elapsed timer (copper as the 120 s cap nears); after injection it
shows a one-line preview of exactly what landed; on a focus divert it
shows the text plus "copied — Ctrl+V".

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
