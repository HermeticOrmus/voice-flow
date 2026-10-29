<p align="center">
  <img src="https://ormus.solutions/mascot/pixellab_liquid_to_swan.gif" alt="voice-flow" width="128" style="image-rendering: pixelated;" />
</p>

<h1 align="center">voice-flow</h1>

<p align="center">
  <em>Voice dictation for Linux Wayland — speak into any window, a local whisper does the rest</em>
</p>

<p align="center">
  <a href="https://github.com/HermeticOrmus/voice-flow/stargazers"><img src="https://img.shields.io/github/stars/HermeticOrmus/voice-flow?style=flat-square&color=aa8142" alt="Stars" /></a>
  <a href="https://github.com/HermeticOrmus/voice-flow/blob/master/LICENSE"><img src="https://img.shields.io/github/license/HermeticOrmus/voice-flow?style=flat-square&color=aa8142" alt="License" /></a>
  <a href="https://github.com/HermeticOrmus/voice-flow/commits"><img src="https://img.shields.io/github/last-commit/HermeticOrmus/voice-flow?style=flat-square&color=aa8142" alt="Last Commit" /></a>
  <img src="https://img.shields.io/badge/Wayland-aa8142?style=flat-square&logo=wayland&logoColor=white" alt="Wayland" />
  <img src="https://img.shields.io/badge/whisper.cpp-aa8142?style=flat-square" alt="whisper.cpp" />
</p>

---

> Press a key, speak, press it again: the words land in the window you were in.

voice-flow is system-wide dictation for Linux on Wayland (Hyprland/Omarchy, COSMIC). Your voice goes to a whisper server or to `whisper-cli` on your own machine, never to a cloud service. An optional local LLM (Ollama) cleans up what you said: it keeps every request and technical name, drops the thinking aloud, and never answers or obeys what you dictate.

## What you get

- **Dictation into any window.** Short text is typed, long text is pasted. Terminals get the terminal paste key.
- **A pill on screen** that shows the level of your voice, the words as they arrive (live partials), and what landed.
- **A focus guard.** If you switch windows while it transcribes, the text goes to the clipboard instead of the wrong window.
- **Express styles.** `prompt` (the default: the prompt you meant for a coding agent), `formal`, `casual`, `verbatim`.
- **Training on your words.** A vocabulary that whisper hears better, learned substitutions, and corrections that teach the cleanup.
- **A control panel** (GTK4, Rust) and a local analytics page.
- **voice-ideas** (optional): a listener for idle thoughts while you game or walk around. It cuts speech at pauses and logs one line per thing you said.

## Quick start

### Requirements

| Need | Why | Arch / Omarchy package |
|---|---|---|
| `arecord` | records the microphone | `alsa-utils` |
| `whisper-cli` and a model | local transcription | `whisper.cpp` (AUR) |
| `ydotool` with `ydotoold` running | types and pastes into the window | `ydotool` |
| `wl-clipboard`, `notify-send`, `jq` | clipboard, notes, history | `wl-clipboard`, `libnotify`, `jq` |
| Python 3 with GTK4 and layer-shell | the pill | `python-gobject`, `gtk4-layer-shell` |
| Rust (optional) | the panel | `rust` |
| Ollama (optional) | the cleanup and agent mode | `ollama` |
| `ffmpeg` (optional) | mic tuning before whisper | `ffmpeg` |
| `uv` (optional) | voice-ideas and the tests | `uv` |

### Install

```bash
git clone https://github.com/HermeticOrmus/voice-flow.git
cd voice-flow
./install.sh        # links bin/* into ~/.local/bin, builds the panel when cargo is there
```

The repo is what runs: `install.sh` links the scripts, so `git pull` updates them.

Get a whisper model (the default is `medium.en`; `base.en` is smaller and faster):

```bash
mkdir -p ~/.local/share/ormus-voice/models
curl -L -o ~/.local/share/ormus-voice/models/ggml-medium.en.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin
```

Bind a key to `voice-flow`. Hyprland with a Lua config (Omarchy), in `~/.config/hypr/bindings.lua`:

```lua
o.bind("SUPER + H", "Toggle dictation", "voice-flow", { dont_inhibit = true, allow_input_capture = true })
```

COSMIC: Settings, Keyboard, Custom Shortcuts, command `voice-flow`.

Press the key, speak, press it again.

### Faster: a resident whisper server

`whisper-cli` loads the model on every take. A resident server answers in about a second. whisper.cpp's own server speaks the same API voice-flow uses:

```bash
whisper-server -m ~/.local/share/ormus-voice/models/ggml-medium.en.bin --host 127.0.0.1 --port 8093
```

voice-flow tries `127.0.0.1:8093` first and falls back to `whisper-cli` when nothing answers. Run the server on a GPU machine on your LAN and name it in the config (below). Keep whisper off an integrated GPU that drives your displays: inference there can hang the desktop.

## Config

`~/.config/ormus-voice/voice-flow.conf`: shell `KEY=value` lines, read on every take. The panel and the analytics page write the keys they own and keep every other line. A variable set in the environment wins.

| Key | Default | Meaning |
|---|---|---|
| `VOICE_PASTE_REMOTE_HOSTS` | `127.0.0.1:8093` | whisper servers, in order, first that answers wins |
| `VOICE_HOST_NAMES` | none | names to show for them: `"192.0.2.7:8093=GPU box;127.0.0.1:8093=laptop"` |
| `OLLAMA_HOSTS` | `http://127.0.0.1:11434` | Ollama servers for the cleanup, agent mode and voice-ideas |
| `VOICE_PASTE_MODEL` | `medium.en` | the model `whisper-cli` uses when no server answers |
| `VOICE_FLOW_METHOD` | `auto` | `auto`, `type` or `paste` |
| `VOICE_FLOW_EXPRESS` | `prompt` | `prompt`, `formal`, `casual`, `verbatim` |
| `VOICE_FLOW_REWRITE` | `on` | the LLM cleanup on or off |
| `VOICE_REWRITE_MODEL` | `qwen3:32b` | the cleanup model on Ollama |
| `VOICE_FLOW_PILL` | `1` | the pill on screen |
| `VOICE_FLOW_PARTIALS` | `1` | live words in the pill |
| `VOICE_FLOW_LOG_TEXT` | `0` | `1` keeps transcript text in the local history |

Example for a GPU server on your LAN:

```bash
VOICE_PASTE_REMOTE_HOSTS="127.0.0.1:8093 192.0.2.7:8093"
OLLAMA_HOSTS="http://192.0.2.7:11434"
VOICE_HOST_NAMES="192.0.2.7:8093=GPU box"
```

### Per-app profiles

`~/.config/ormus-voice/app-profiles.conf` maps window classes to `verbatim` (raw transcript, no LLM) or `clean`. Unlisted apps get `clean`. Find a class with `hyprctl activewindow -j | jq -r .class`.

## Components

| Command | What it does |
|---|---|
| `voice-flow` | The key's command: start, then stop, transcribe and insert. `voice-flow agent` treats the dictation as a question and inserts the answer. |
| `whisper-paste` | The engine: record, silence gate, mic tuning, whisper (server first, then `whisper-cli`), hallucination filter, optional cleanup. |
| `vibecoding-record` | Mic tuning: high-pass, noise reduction, compression. Skipped when absent. |
| `voice-flow-pill` | The on-screen pill (GTK4 layer shell). |
| `voice-flow-panel` | The control window: state, today's numbers, Express, switches, dictionary, last paste. |
| `voice-flow-web` | A local analytics page on `127.0.0.1:7385`. Stdlib only, no outside requests. |
| `voice-train`, `voice-substitute` | Train whisper on your words; apply learned fixes. |
| `voice-rewrite-ollama`, `voice-rewrite-claude`, `voice-agent-ollama` | The cleanup and agent backends. |
| `voice-ideas`, `voice-ideas-sort` | The idle-thought listener and its sorter (below). |

## Training it on your words

1. **Vocabulary** (`~/.config/ormus-voice/vocabulary.txt`): terms whisper is told to expect, so it hears them.
2. **Substitutions** (`~/.config/ormus-voice/substitutions.tsv`): heard to meant, applied before any LLM, in every mode.
3. **Corrections**: the journal `~/.local/share/ormus-voice/corrections.jsonl` (written by `voice-train`) feeds its latest pairs to the cleanup as examples.

```bash
voice-train PyTorch Hyprland kubectl   # say each term; learn what whisper gets wrong
voice-train status
voice-train distill                    # corrections journal to rules
```

## voice-ideas

`voice-ideas` listens on the default microphone, cuts speech at a 1.5 s pause (silero VAD), sends each utterance to your whisper server and appends one JSON line to `~/.local/share/ormus-voice/ideas/utterances.jsonl`. It never types into a window, keeps no audio, and drops what you say while a dictation records. `install.sh` installs the user unit and does not enable it:

```bash
systemctl --user start voice-ideas    # listen
systemctl --user stop voice-ideas
```

After a pause, `voice-ideas-sort` splits a burst into separate notes with an Ollama model (or Claude with `VOICE_IDEAS_SORTER=claude` and an API key). Its targets come from a local agent board; without one, every note still lands, with no target.

## Privacy

- Audio never leaves the machines you name in the config.
- The history (`~/.local/share/ormus-voice/history.jsonl`) holds counts only: words, characters, timing, method. Transcript text is kept only when you set `VOICE_FLOW_LOG_TEXT=1`.
- The analytics page listens on localhost and makes no outside requests.

## Compatibility

- Hyprland (Omarchy) and COSMIC on Wayland; other wlroots compositors should work with `ydotool`.
- The focus guard reads the focused window on COSMIC today; on Hyprland it is off until it reads `hyprctl`.
- Used daily on Arch (Omarchy, Hyprland); earlier on Pop!_OS (COSMIC).

## Tests

```bash
./check.sh                                           # Python tests (uv supplies the packages)
(cd panel && cargo test --release && cargo clippy --release)
```

## Feedback

Tell us what worked and what got in your way: [open a feedback issue](https://github.com/HermeticOrmus/voice-flow/issues/new?template=feedback.yml). Questions and setups are welcome in [Discussions](https://github.com/HermeticOrmus/voice-flow/discussions).

## Contributing

Small pull requests, one change each, with a test that fails before the change. Run `./check.sh` first. Commits follow `type(scope): description`.

## License

MIT © 2026 [Diego Bodart](https://github.com/HermeticOrmus) — see [LICENSE](LICENSE). Built under the [Gold Hat principle](GOLD_HAT.md).
