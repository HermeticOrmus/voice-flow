# Explainer: the mini sidecar

Subject: corvin-mini's role as Sun's dedicated extension — voice
inference, portal, transcription, LLM serving, sync, and loops.
System-mode explainer (state grounded 2026-07-13 against the live
machine). Companion quiz at the end; answers collapsed.

## Background

The problem space is a one-machine bottleneck. Sun (your daily driver)
has a 12 GB RTX 5070 Ti that four workloads were fighting over: the
scribe meeting stack (a resident whisper server plus batch
transcription jobs, ~11 GB together), ollama for local LLM calls, voice
dictation's whisper, and whatever you were actually doing (gaming, CUDA
dev). GPU memory is not shareable the way CPU time is — when it's full,
the loser gets evicted to CPU and runs 50-100x slower. That is exactly
what happened: the dictation rewrite model fell to CPU (47 s to say
"OK"), every dictation timed out its 8 s rewrite budget, and voice
latency rotted to 14-17 s without any single component "breaking."

corvin-mini is the answer to that shape of problem: a GMKtec EVO-X2
(Ryzen AI Max+ 395 "Strix Halo") with 128 GB of unified memory split
64 GB system / 64 GB GPU. Its Radeon 8060S is slower per-watt than
Sun's NVIDIA card, but it has five times the memory budget and it is
always on at HQ. The design conclusion: latency-sensitive work stays
where the user is; *memory-resident* work — models that must sit loaded
and warm — moves to the box with the memory. A sidecar, in the original
motorcycle sense: bolted alongside, carrying the weight, steered by the
main vehicle.

Three transport paths connect Sun to mini, tried in this order
everywhere: Thunderbolt point-to-point (`10.55.0.1 ↔ 10.55.0.2/30`,
0.3 ms), HQ LAN (`10.10.90.x`, 0.5 ms), Tailscale (`100.107.202.72`,
~12 ms, works from anywhere). Every client probes with sub-second
connect timeouts and falls back — first to the next path, then to
running the work locally on Sun. Nothing hard-depends on mini being
reachable.

## The essence

**One governing idea: models live resident on the box with the memory;
Sun sends work over the fastest reachable wire and keeps a local
fallback.** Everything else — the three whisper servers, the pinned
ollama models, the portal, the Syncthing folders, the mentor loop — is
that one idea applied to a different workload. The corollary that makes
it safe: mini is always *optional*. Unplug it and every consumer
degrades to the pre-sidecar behavior instead of failing.

Concrete before/after: a dictation used to mean *load whisper into
Sun's contended GPU, transcribe, cold-load a rewrite LLM, time out*.
Now it means *POST a 200 KB WAV over Thunderbolt to a model that never
unloads, get text in 0.4 s, POST it to a pinned 3b model, get cleaned
text in 0.3 s* — ~0.8 s end to end, and Sun's GPU did nothing.

## Guided tour (dependency order)

**The wire.** `thunderbolt0` on both machines, static /30, NetworkManager
profile `tb-sun` on mini (autoconnect, survives reboots). The host-probe
lists (`10.55.0.2 → 10.10.90.174 → 100.107.202.72`) appear in
`bin/whisper-paste`, `bin/voice-rewrite-ollama`, `bin/voice-agent-ollama`,
`bin/voice-flow-pill`, and scribe's `scribe-remote-transcribe` — same
pattern, five consumers.

**Three whisper servers, three latency classes.** All are whisper.cpp
built with Vulkan for the 8060S (`~/tools/whisper.cpp` on mini), each a
systemd unit holding one model resident:

- `:8090` `whisper-server.service` — medium.en + Silero VAD. The
  dictation *finals* server: accuracy tier, 0.4 s per utterance.
- `:8091` `whisper-server-partial.service` — base.en, no VAD. The
  *streaming* tier: 50-130 ms per call, hit every 1.2 s by the pill's
  sliding 15 s window while you speak. Kept separate from :8090 so a
  partial tick never queues behind a final.
- `:8092` `whisper-server-batch.service` — large-v3-turbo + `--convert`
  (ffmpeg, so it eats flac/m4a directly; `WorkingDirectory` must be
  writable or conversion 500s). The *throughput* tier: scribe meetings
  and dojo videos. A 19-minute talk transcribes in ~32 s.

Why three servers instead of one big model? Isolation of latency
classes — the same reason web apps separate interactive queries from
batch reports. A one-hour meeting on :8092 must not add even a
millisecond to a dictation on :8090.

**ollama on `:11434`** serves the rewrite (`llama3.2:3b`, pinned with
`keep_alive -1`, `num_ctx 4096` → 2.3 GB, 100 % GPU, ~0.3 s warm) and
agent mode (same model by default, `VOICE_AGENT_MODEL=ornith-9b` for
harder asks). Pinning is per-request: remote hosts get `-1`, Sun-local
fallback deliberately does not, to protect Sun's scarce VRAM. (A second
pinned model, qwen2.5:3b, is a leftover from prompt A/B testing —
harmless in 64 GB, unpin at will.)

**The portal.** `voice-flow-web.service` (user unit, aiadmin, linger)
serves the analytics dashboard on `127.0.0.1:7385`;
`voice-portal-tunnel.service` runs cloudflared, fronting it as
voice.ormus.solutions behind CF Access (your two emails). The data it
reads arrives by Syncthing, not by network calls to Sun — which is why
the portal survives Sun sleeping.

**Syncthing is the data plane.** Four two-way folders: `voice-data`
(`~/.local/share/ormus-voice`, minus models/), `voice-conf`
(`~/.config/ormus-voice`), `second-brain` (the vault — mini is its
always-on home), `loops-lib` (`~/loops/lib`, shared with Prometheus too
— one `common.sh`, three machines). Rule of thumb encoded here: *state
syncs, services don't*. Each machine runs its own processes against a
replicated filesystem; a portal edit on mini lands in Sun's files in
~15 s and vice versa.

**The loops beachhead.** Mini runs the `mentor` loop (cron 21:00
Panama — pinned with `TZ=` because mini's system clock is Chicago):
reads 24 h of Ormus-Claw via Periskope, writes a teaching digest +
quiz, sends it back marked `*mentor*` and filters that marker from its
own next read (self-ingestion guard). To exist at all it needed the
claude CLI (one self-contained binary) plus `~/.claude/.credentials.json`
copied over — the fleet auth-replication pattern.

Everything else on mini is one collective sentence: yt-dlp + ffmpeg for
media capture, the Ornith coding models that predate today, and the
Vulkan/glslc toolchain that built whisper.cpp.

## The quiz

1. You dictate at a coffee shop, away from HQ, with mini reachable only
   over Tailscale — then your phone hotspot drops entirely. What does a
   dictation do, step by step, in each of those two situations?
2. Why three separate whisper servers with three models, rather than one
   server with large-v3-turbo serving all three use cases? Name the
   principle and the concrete harm the single-server design would cause.
3. The rewrite call pins its model (`keep_alive -1`) only when the
   serving host is remote. What failure mode does the localhost
   exception prevent, and on which machine?
4. The portal on mini shows your dictation stats while Sun is asleep.
   Trace where those bytes came from and name the rule ("state syncs,
   services don't") in action. What would be wrong with the portal
   instead querying Sun's API directly?
5. Mini's system timezone is America/Chicago but mentor must fire at
   21:00 Panama. Where was that handled, and what silently goes wrong
   for *any future mini cron job* whose author forgets this?

<details><summary>Answers</summary>

1. Tailscale case: the host probe fails Thunderbolt and LAN within
   ~0.4 s each, reaches `100.107.202.72:8090`, transcribes remotely
   (slightly higher RTT, same flow). Hotspot-drop case: all three
   probes fail (~1.2 s total), whisper-paste falls back to local
   whisper-cli on Sun (base.en per conf), and the rewrite script's own
   probe falls through to Sun-local ollama or, failing that, passes the
   raw transcript through untouched. You still get text — slower and
   rougher, never nothing.
2. Isolation of latency classes. One server = one request queue: a
   one-hour meeting upload on the shared server would queue a live
   dictation (or a 1.2 s partial tick) behind minutes of batch decode.
   Separate resident models mean the interactive tier's worst case is
   unaffected by the throughput tier's load. (Also: partials want the
   *fastest* model, finals want the *accurate* one — one model can't be
   both.)
3. VRAM starvation on Sun. Pinning a model with `keep_alive -1` into
   Sun's 12 GB card would permanently occupy memory that scribe,
   games, and CUDA work need — recreating the exact contention the
   sidecar exists to end. Mini has ~64 GB of GPU budget, so pinning
   there is free.
4. A dictation on Sun appends to `history.jsonl`; Syncthing replicates
   the file to `/home/aiadmin/.local/share/ormus-voice` within seconds;
   the portal's stdlib server reads its *local* copy. Querying Sun's
   API instead would couple the portal's availability to Sun being
   awake — the exact dependency the migration to mini was meant to cut.
5. In the crontab line itself: `0 21 * * * TZ=America/Panama ...`.
   A future job without the `TZ=` prefix fires on Chicago time —
   silently one hour off (worse around DST transitions), the kind of
   drift you only notice weeks later when a digest starts arriving at
   the wrong hour.

</details>
