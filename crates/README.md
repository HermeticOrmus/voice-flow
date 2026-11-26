# voice-flow in Rust: slice 1

Four crates and the scripts they replace. The scripts stay the live path until you switch.

| Crate | Binary | Replaces |
|---|---|---|
| `vf-core` | (library, no I/O) | the conf, substitutions, junk filters, Express prompts, history row, app profiles |
| `vf-engine` | `vf-whisper-paste`, `vf-rewrite-ollama`, `vf-substitute` | `bin/whisper-paste`, `bin/voice-rewrite-ollama`, `bin/voice-substitute` |
| `vf-inject` | (library) | the focus guard and injection half of `bin/voice-flow` |
| `vf` | `vf` | `bin/voice-flow` |

Same command lines, environment variables and files as the scripts, so the Python pill, the panel and the web page keep working. HTTP still goes through `curl` and audio through `arecord` and `sox`: that keeps the 98 black-box tests as the contract. Native PipeWire capture is spike S3.

## Prove it

```bash
./check.sh                      # the scripts against the stubs
./check-rust.sh                 # fmt, clippy, cargo test, then the same tests against the Rust
```

`VF_IMPL=rust` (read in `tests/stubs.py`) points `bin/voice-flow`, `bin/whisper-paste`, `bin/voice-rewrite-ollama` and `bin/voice-substitute` at the built binaries (`target/debug`, or `VF_TARGET_DIR`).

## Try it live (not done yet)

```bash
cargo build --release --workspace
ln -sf "$PWD/target/release/vf" ~/.local/bin/vf
ln -sf "$PWD/target/release/vf-whisper-paste" ~/.local/bin/whisper-paste
ln -sf "$PWD/target/release/vf-rewrite-ollama" ~/.local/bin/voice-rewrite-ollama
```

Then in `~/.config/hypr/bindings.lua` point the key at the new command (`vf` runs `whisper-paste` from PATH, as `voice-flow` does):

```lua
o.bind("SUPER + H", "Toggle dictation", "vf", { dont_inhibit = true, allow_input_capture = true })
```

Limits of slice 1 (read before you switch):

- No COSMIC focus fallback: the focus guard and the app profiles read `hyprctl` only (`ormus-active-window` is not called).
- The conf reader does no `$VAR` expansion and no command substitution: `KEY=value`, quotes, `export` and comments only. A conf that relies on the shell sourcing it differently reads differently.
- In `vf` the conf wins over the environment, as it does in `bin/voice-flow` (which sources the conf); the comment in that script says the opposite.
- The last line of `app-profiles.conf` is honoured without a trailing newline (the bash `read` loop drops it).
- HTTP is `curl`, capture is `arecord`, the gate is `sox`, encoding is `ffmpeg`; `voice-train`, `voice-agent-ollama` and `voice-rewrite-claude` are still the scripts.

Back to the scripts: `./install.sh` re-links `bin/*` over the three links above, and the bind goes back to `voice-flow`.
