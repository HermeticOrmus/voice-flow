"""Probe: silero on a growing raw capture at the audio's own pace (one 512-sample chunk per 32 ms), CPU time over wall time."""
import os, sys, time
import numpy as np, onnxruntime as ort
RAW, SECS = sys.argv[1], float(sys.argv[2])
so = ort.SessionOptions(); so.intra_op_num_threads = 1; so.inter_op_num_threads = 1
sess = ort.InferenceSession("model/silero_vad.onnx", so, providers=["CPUExecutionProvider"])
state = np.zeros((2, 1, 128), dtype=np.float32); ctx = np.zeros(64, dtype=np.float32)
f = open(RAW, "rb"); f.seek(0, 2)
t_wall0, t_cpu0, chunks, speech = time.perf_counter(), time.process_time(), 0, 0
while chunks * 512 / 16000 < SECS:
    b = f.read(1024)
    if len(b) < 1024:
        f.seek(-len(b), 1) if b else None; time.sleep(0.01); continue
    x = np.frombuffer(b, dtype=np.int16).astype(np.float32) / 32768.0
    out, state = sess.run(None, {"input": np.concatenate([ctx, x])[None, :], "state": state, "sr": np.array(16000, dtype=np.int64)})
    ctx = x[-64:]; chunks += 1; speech += out[0, 0] >= 0.5
wall, cpu = time.perf_counter() - t_wall0, time.process_time() - t_cpu0
print(f"live: audio {chunks*512/16000:.1f} s, wall {wall:.1f} s, cpu {cpu:.2f} s, cpu/wall {cpu/wall:.4f}, speech chunks {speech}")
