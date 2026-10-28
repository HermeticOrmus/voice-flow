#!/usr/bin/env python3
"""Probe: cut a raw s16le 16 kHz mono capture into speech segments with silero (ONNX) or webrtcvad,
transcribe each with whisper-server, label each as speech / empty / hallucination. Prints a table.
Throwaway: no error handling beyond the observation."""
import json, math, os, re, struct, subprocess, sys, tempfile, time, wave
import numpy as np

RAW, VAD, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
HOST = os.environ.get("WHISPER_HOST", "127.0.0.1:8093")
NO_WHISPER = os.environ.get("NO_WHISPER") == "1"
SR = 16000
THRESH, NEG, MIN_SPEECH_MS, PAD_MS, MAX_SPEECH_S = 0.5, 0.35, 250, 150, 30
MIN_SILENCE_MS = int(os.environ.get("MIN_SILENCE_MS", "800"))
VOCAB = " ".join(l.strip() for l in open(os.path.expanduser("~/.config/ormus-voice/vocabulary.txt"))
                 if l.strip() and not l.startswith("#"))
HALLU = {"", "you", "thank you", "thanks for watching", "bye", "okay", "thank you for watching", "music", "the", "subscribe"}

pcm = np.frombuffer(open(RAW, "rb").read(), dtype=np.int16)
dur = len(pcm) / SR
audio = pcm.astype(np.float32) / 32768.0

t_cpu0 = time.process_time(); t_wall0 = time.perf_counter()
if VAD == "silero":
    import onnxruntime as ort
    so = ort.SessionOptions(); so.intra_op_num_threads = int(os.environ.get("ORT_THREADS", "1")); so.inter_op_num_threads = 1
    sess = ort.InferenceSession("model/silero_vad.onnx", so, providers=["CPUExecutionProvider"])
    CHUNK, CTX = 512, 64
    state = np.zeros((2, 1, 128), dtype=np.float32); ctx = np.zeros(CTX, dtype=np.float32)
    probs = []
    for i in range(0, len(audio) - CHUNK + 1, CHUNK):
        x = np.concatenate([ctx, audio[i:i + CHUNK]])[None, :]
        out, state = sess.run(None, {"input": x, "state": state, "sr": np.array(SR, dtype=np.int64)})
        probs.append(float(out[0, 0])); ctx = audio[i + CHUNK - CTX:i + CHUNK]
    FRAME = CHUNK
else:
    import webrtcvad
    v = webrtcvad.Vad(3); FRAME = 480
    probs = [1.0 if v.is_speech(pcm[i:i + FRAME].tobytes(), SR) else 0.0 for i in range(0, len(pcm) - FRAME + 1, FRAME)]
vad_cpu = time.process_time() - t_cpu0; vad_wall = time.perf_counter() - t_wall0
frame_s = FRAME / SR

# segmenter (silero's get_speech_timestamps shape, simplified)
segs, start, last_speech, in_speech = [], None, None, False
for k, p in enumerate(probs):
    t = k * frame_s
    if p >= THRESH:
        if not in_speech: in_speech, start = True, t
        last_speech = t
        if t - start > MAX_SPEECH_S: segs.append((start, t)); start, in_speech = t, True
    elif in_speech and p < NEG and t - last_speech >= MIN_SILENCE_MS / 1000:
        segs.append((start, last_speech + frame_s)); in_speech = False
if in_speech: segs.append((start, (len(probs)) * frame_s))
segs = [(max(0, a - PAD_MS / 1000), min(dur, b + PAD_MS / 1000)) for a, b in segs if (b - a) * 1000 >= MIN_SPEECH_MS]

os.makedirs(OUT, exist_ok=True)
rows = []
for n, (a, b) in enumerate(segs, 1):
    clip = pcm[int(a * SR):int(b * SR)]
    rms = float(np.sqrt(np.mean(clip.astype(np.float64) ** 2)) / 32768)
    path = f"{OUT}/seg{n:03d}_{a:07.1f}-{b:07.1f}.wav"
    with wave.open(path, "wb") as w: w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR); w.writeframes(clip.tobytes())
    text, ms, label = "", 0, "skipped"
    if not NO_WHISPER:
        t0 = time.time()
        text = subprocess.run(["curl", "-sS", "-m", "30", "-F", f"file=@{path}", "-F", "response_format=text", "-F", f"prompt={VOCAB}",
                               f"http://{HOST}/inference"], capture_output=True, text=True).stdout.strip()
        ms = int((time.time() - t0) * 1000)
        low = re.sub(r"[.,!?]", "", text.lower()).strip()
        label = "empty" if low in HALLU else ("hallucination" if re.search(r"\b(\w+)\b([\s,-]+\1\b){3,}", low) else "speech")
    rows.append({"n": n, "t0": round(a, 1), "t1": round(b, 1), "dur": round(b - a, 1), "rms": round(rms, 4), "ms": ms, "label": label, "text": text})

summary = {"vad": VAD, "audio_s": round(dur, 1), "segments": len(segs), "speech_s": round(sum(b - a for a, b in segs), 1),
           "vad_cpu_s": round(vad_cpu, 3), "vad_cpu_fraction_of_core": round(vad_cpu / dur, 4), "vad_wall_s": round(vad_wall, 3),
           "labels": {k: sum(1 for r in rows if r["label"] == k) for k in ("speech", "empty", "hallucination", "skipped")},
           "whisper_ms_median": sorted(r["ms"] for r in rows)[len(rows) // 2] if rows else None}
json.dump({"summary": summary, "rows": rows}, open(f"{OUT}/result.json", "w"), indent=1, ensure_ascii=False)
print(json.dumps(summary))
print("| # | t0 | t1 | dur | rms | ms | label | text |"); print("|---|---|---|---|---|---|---|---|")
for r in rows: print(f"| {r['n']} | {r['t0']} | {r['t1']} | {r['dur']} | {r['rms']} | {r['ms']} | {r['label']} | {r['text'][:90]} |")
