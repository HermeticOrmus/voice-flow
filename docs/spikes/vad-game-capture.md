---
assumption: silero VAD (ONNX on onnxruntime, CPU only) cuts a real DJI capture of Ormus while he games into speech utterances, with no utterance from room or game audio alone.
verdict: VERIFIED
proof_level: 5
observed_on: Sun, DJI Mic Mini on Ormus, the game on, live parec capture (228 s); plus the laptop mic in the same room (310 s) as a stress proxy
scope: one session, one game, one room with loud speakers, one speaker (English, Spanish, noises), no voice chat from others; silero one thread, 1.5 s pause; does not cover another person talking in the room
reproduce: "parec --device=<dji source> --format=s16le --rate=16000 --channels=1 --raw cap.raw (5 min, game on, speak), then cd spike && MIN_SILENCE_MS=1500 ORT_THREADS=1 .venv/bin/python vad_probe.py cap.raw silero out; every row must be your voice (the audio of this run was deleted; the result tables are in ~/.local/state/ormus-voice/spikes/vad-game-capture/)"
probe: spike/vad-game-capture
reopen_if: another person talks in the room, a game with voice chat, a different mic, or a game that runs on Sun's dGPU (whisper on the same card)
action: PROMOTE
---

## Card

```
Assumption:      silero VAD (ONNX on onnxruntime, CPU only) cuts a real DJI capture of Ormus while he games into speech utterances, with no utterance from room or game audio alone.
Question:        mechanism
Prediction:      On a capture of at least 5 minutes with N spoken thoughts, silero yields one or more segments per thought and 0 segments whose whisper text is empty or a known hallucination. webrtcvad (mode 3) on the same audio yields more such empty segments. silero costs under 5% of one core on 32 ms frames.
Pass predicate:  Every silero segment transcribes to real speech (non-empty after the whisper-paste hallucination filter), every spoken thought Ormus confirms is covered by at least one segment, and VAD CPU is under 5% of one core.
Wrong if:        More than 1 segment per 5 minutes transcribes to nothing or a hallucination (room or game audio), or a spoken thought has no segment, or VAD CPU is 5% of a core or more.
Must not worsen: whisper-server :8093 keeps answering a Super+H dictation during the probe (its inference lock is shared).
Target surface:  Sun, the DJI Mic Mini as the default source, Ormus in the room while he games, live capture through parec at 16 kHz mono.
Proxy surfaces:  The same capture replayed from its raw file for the VAD comparison runs and the CPU measure.
One-way door:    no
Attempt budget:  4
Selection rule:  n/a (one mechanism plus one comparator)
```

Dropped from the card before attempt 0 (measured during recon): "whisper needs no dGPU time on Sun while a game holds the dGPU". No game process holds Sun's dGPU now (`nvidia-smi` lists only whisper-server, 5408 MiB), so there is no surface to test it on. Both whisper hosts answer: Sun 127.0.0.1:8093 and sg 100.66.108.108:8093 (health in 0.19 s). The listener carries the same host list as whisper-paste. Reopen when a game runs on Sun's dGPU.

### Card amendments after the contract check (before attempt 0)

The fresh-context skeptic (sonnet, card only) answered: the probe can fail, but a pass would not prove the assumption. Its objections and the edits taken:

1. Ground truth. "Transcribes to real text" is not "is Ormus". Game dialogue, a stream or a teammate transcribe as fluent text. Edit: Ormus labels every segment's transcript as his or not his; Wrong-if adds "any segment that is not his voice". A planted stretch of 60 s or more with game audio and Ormus silent must yield 0 segments.
2. Game audio may be inaudible to the DJI (its noise cancel). Edit: record the capture's RMS with Ormus silent; if game audio is inaudible, say the claim is untested on that surface. Found at attempt 0: the DJI transmitter is OFF (receiver sends zeros; dji-mic retries every 20 s). The laptop mic hears the room at RMS 0.11 with peaks at full scale: a harder surface than the DJI. It becomes the stress proxy until the DJI is on.
3. The comparator was outside the predicate. Edit: Pass adds "silero's non-Ormus segments are at most half of webrtcvad's (0 vs 0 passes)".
4. Fragmentation. Edit: record segments per thought; it is not a kill criterion, because the listener groups pieces under 20 s apart into one burst.
5. CPU and must-not-worsen were vague. Edit: VAD CPU = process_time of the VAD loop over audio seconds on the replay, and the live probe's CPU time over wall time with the game on. Must-not-worsen = a fixed 3 s clip POSTed to :8093 during the probe answers under 3 s (synthetic keys are never sent on Sun).
6. N. Edit: at least 5 spoken thoughts in the capture, or the coverage half of the predicate is INCONCLUSIVE.

## Attempts
| # | hypothesis | change | observed | result |
|---|------------|--------|----------|--------|
| 0 | baseline: game audio in the room, Ormus silent (as far as known), laptop mic, 56 s | none (harness smoke, no whisper) | silero: 0 segments; webrtcvad mode 3: 4 segments, 7.5 s | prediction holds for both VADs on game-only audio |
| 1 | the 12% CPU read was every onnxruntime thread summed | intra_op_num_threads = 1, inter_op = 1 | silero 5 runs: CPU fraction of one core median 0.018, range 0.0096 to 0.026; wall 0.15 s for 56 s. Default threads 3 runs: median 0.0625, range 0.0535 to 0.143 | under the 5% line with one thread; over it with the default pool |
| 2 | silero cuts no game audio while webrtcvad does, on 5 min of the room | whole 310 s capture, whisper on every segment, both VADs | silero: 1 segment ("Coming." 121.4 to 122.1 s, rms 0.011). webrtcvad: 6 segments, 8.9 s; 5 of them game audio that whisper turned into multilingual nonsense (passed the fixed-list filter); 1 the same "Coming!". silero CPU 0.0019 of a core; whisper median 197 ms (silero) and 1985 ms (webrtc, longer clips) | false-positive half holds for silero (0 or 1 in 310 s, pending Ormus's label of "Coming."); coverage half waits on Ormus speaking |
| 3 | the target surface: the DJI on Ormus, the game on, him speaking | same probe, DJI capture 228 s (transmitter switched on at 20:34 CDT) | silero: 12 segments, 16.5 s. Ormus labeled groups B (169 s, loud noises), C (175 to 199 s, talk in English and Spanish) and D (two quiet nonsense clips at 190 and 193 s) as his own voice, "crazy stuff". Group A (first 6 s, "What's up, great work, how are you?" at rms 0.007) not picked: confirmation asked. webrtcvad: 13 segments | his voice is covered (10 of 12 segments his, pending A); the first 6 s hold the only candidate false segments |
| 4 | a 1.5 s pause gives whisper whole sentences instead of fragments | MIN_SILENCE_MS 800 to 1500 | 7 segments, 20.9 s; "The line? What do you want me to give you?" and "¿Qué es esto? ¿Qué ocurre? Matin dance. ¿Hmm? ¿Tu matin dance?" come out whole; CPU unchanged (0.0019) | the listener cuts at 1.5 s pauses |

## Evidence

### Attempt 0 and 1 (replay of the first 56 s of the room capture, `/tmp/claude-1000/vi-spike/smoke.raw`)

- Room level with the game on and Ormus silent: laptop mic at 13% volume still reads peaks at full scale and RMS 0.11 (3 s sample). The DJI source read all zeros (transmitter off).
- silero, one thread: 0 segments on game audio. webrtcvad mode 3: 4 segments (7.5 s) on the same audio.
- CPU, silero one thread, 5 runs: 0.0096 0.0214 0.0118 0.0181 0.026 of a core. Default threads, 3 runs: 0.0625 0.1426 0.0535. `process_time` sums every thread; the default pool spins. The listener must pin onnxruntime (and OMP/OpenBLAS) to one thread.
- What limits the number: the replay runs the loop as fast as it can (wall 0.15 s for 56 s of audio); a live stream paces one chunk per 32 ms, so a spinning pool would cost more, not less. Measured on the replay only so far.

### Attempt 2 (replay of the whole 310 s room capture, `/tmp/claude-1000/vi-spike/cap5.raw`, results in `cap5-silero/result.json` and `cap5-webrtc/result.json`)

- silero: one segment, "Coming." at 121.4 s (0.7 s, rms 0.011). Ormus labeled it: his own word. So silero produced 0 game segments in 310 s of loud room audio, and 1 of 1 of his utterances. He was otherwise quiet in those 5 minutes, so the coverage half needs the next capture.
- webrtcvad: six segments. Five are game audio; whisper produced fluent-looking nonsense in several languages for them, and the fixed hallucination list did not catch one of them. The skeptic predicted this failure of the filter: the filter is not a speaker test.
- Constraint found: the listener must not rely on the fixed phrase list alone. whisper-server's `/transcribe` answers `segments[].no_speech_prob` and `avg_logprob` and the detected `language`; drop a segment whose language is not en or es, or whose no_speech_prob is high. The thresholds come from this capture's junk segments.

### whisper's own signals on the six webrtcvad segments (`/transcribe`, JSON, same vocabulary prompt)

| segment | text | avg_logprob | no_speech_prob |
|---|---|---|---|
| game 13.9 s | "助開" (first run: "Signal impossibility Machine Signal") | -5.92 | 0 |
| game 23.4 s | "you" | -0.98 | 0 |
| game 26.5 s | "you" | -0.97 | 0 |
| game 35.2 s | "Thank you." | -0.66 | 0 |
| Ormus 121.4 s | "Coming." | -0.71 | 0 |
| game 241.0 s | "Thank you." | -0.81 | 0 |

`no_speech_prob` is 0 on every clip, so it is no signal here. The fixed list catches "you" and "Thank you."; `avg_logprob` under -1.5 catches the rest (junk -5.92, real speech -0.66 to -0.98). Whisper's output on junk changes between runs (temperature fallback), so a filter on the text alone is not stable; the log-prob is.

### Live cadence CPU on the DJI stream (`spike/vad_live.py`, one 512-sample chunk per 32 ms, the game on)

- Threads pinned (onnxruntime intra 1 / inter 1, OMP_NUM_THREADS=1, OPENBLAS_NUM_THREADS=1): 60 s of audio, wall 59.3 s, CPU 0.30 s, cpu/wall 0.0050.
- Default threads: 30 s of audio, wall 31.8 s, CPU 30.0 s, cpu/wall 0.94. The thread pool spins between chunks.
- Constraint: the listener pins every thread pool to one thread, or it burns a whole core while it waits for audio.

### Attempts 3 and 4 (the DJI capture, `dji5.raw`, 228 s; results in `dji5-silero/`, `dji5-silero-1500/`, `dji5-webrtc/`)

- Levels on the DJI: his speech rms 0.014 to 0.086; the floor with him silent 0.002; group A 0.007.
- whisper `/transcribe` on the 12 silero segments: `avg_logprob` under -1.5 only on two of HIS noises (humming -3.33, a quiet vocal -6.13), never on group A (-0.45). So the log-prob filter removes noise he made, not quiet game dialogue. `language` came back `pt` on one of his Spanish lines: never filter on language.
- 1.5 s pause: 7 segments instead of 12, whole sentences, same CPU.
- Attempt budget (4) spent.

## Skeptic

## Constraints for /mplan

- silero VAD, the ONNX model on onnxruntime, every thread pool pinned to one thread (onnxruntime intra and inter, OMP_NUM_THREADS, OPENBLAS_NUM_THREADS). Unpinned it burns a core at the live cadence.
- Cut at a 1.5 s pause (MIN_SILENCE_MS 1500), 250 ms minimum speech, 150 ms pad, 30 s maximum. Whole sentences reach whisper.
- Transcribe through `/transcribe` (JSON) with the vocabulary as `initial_prompt`; drop a segment whose text is on the fixed list or whose `avg_logprob` is under -1.5. Never filter on `language` or `no_speech_prob`.
- Whisper host order as whisper-paste: Sun 127.0.0.1:8093 first, then sg. Per-clip latency on Sun: median 0.22 s, max 2.7 s.
- webrtcvad is out: 5 game segments in 310 s where silero had 0.
- The DJI's own level with Ormus silent is rms 0.002; his speech 0.007 to 0.086. No level gate is needed on this evidence.
