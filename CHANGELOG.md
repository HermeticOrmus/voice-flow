# Changelog

## Unreleased

- A failing local `whisper-cli` ends the take as an empty transcript: the pill names the reason and the temp WAV is removed. It used to end `whisper-paste` with no text and no reason.
- `VOICE_PASTE_UPLOAD=opus` sends the take to the whisper server as Opus 32 kbit/s, for a slow uplink. WAV stays the default; a failed encode sends the WAV; the local `whisper-cli` reads the WAV.

## 1.0.0: public release

- The repo is public: whisper and Ollama hosts come from `voice-flow.conf` (this machine by default), and the tree names no private host, client or person (`tests/test_public.py` checks every file).
- MIT license, README for a fresh install, issue forms for bugs and feedback.
