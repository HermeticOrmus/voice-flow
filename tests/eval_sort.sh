#!/usr/bin/env bash
# eval_sort.sh: the sorter's judgment on two of Ormus's real dictations (from his history.jsonl),
# against the live board targets and the live model. Expectations written before the first run:
#
#   burst 1 = the 20:03 ramble that asked for this system ("speak into the mic ... sunboard ...
#             copy and paste"). PASS when at least one note targets the agent named
#             "Voice idea capture dashboard" (this session on Sun) and the note keeps his words:
#             it contains "mic", "sun board" or "sunboard", and "copy".
#   burst 2 = the 19:xx note "the sun board right now tells me that nothing is running ... stuff
#             running on multiple of the SG sessions". PASS when its note targets an agent whose
#             name matches /voice idea capture|sun board/i, or has no target with a why that
#             mentions the board.
#
# Needs sg (Ollama) and the Sun board on 127.0.0.1:7741. Writes nothing to the real notes file.
set -euo pipefail
cd "$(dirname "$0")/.."
H=~/.local/share/ormus-voice/history.jsonl
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
python3 - "$H" "$T" <<'PY'
import json, sys
from datetime import datetime, timezone
H, T = sys.argv[1:]
rows = [json.loads(l) for l in open(H) if l.strip()]
pick = {}
for r in rows:
    t = r.get("text") or ""
    if "speak into the mic" in t and "sunboard" in t.lower(): pick["b1"] = r
    if "nothing is running" in t and "SG sessions" in t: pick["b2"] = r
assert len(pick) == 2, f"found {list(pick)} in history"
iso = lambda s: datetime.fromtimestamp(s, timezone.utc).astimezone().isoformat(timespec="seconds")
out = []
for i, k in enumerate(("b2", "b1")):          # b2 first: it was said first (19:xx), b1 at 20:03
    base = 1_700_000_000 + i * 600
    out.append({"id": f"u-{base * 1000}", "t0": iso(base), "t1": iso(base + 60), "dur": 60.0, "text": pick[k]["text"], "lp": -0.5, "backend": "eval"})
open(f"{T}/utterances.jsonl", "w").write("".join(json.dumps(u) + "\n" for u in out))
print("bursts:", [u["id"] for u in out])
PY
uv run --quiet --python 3.12 python - "$T" <<'PY'
import importlib.machinery, importlib.util, json, re, sys, time
T = sys.argv[1]
l = importlib.machinery.SourceFileLoader("vs", "bin/voice-ideas-sort"); s = importlib.util.spec_from_loader("vs", l); m = importlib.util.module_from_spec(s); l.exec_module(m)
from pathlib import Path
targets = m.fetch_targets()
t0 = time.time()
n = m.sort_once(Path(f"{T}/utterances.jsonl"), Path(f"{T}/notes.jsonl"), m.llm_ollama, targets, now=time.time())
print(f"{n} notes in {time.time() - t0:.1f} s, {len(targets)} targets")
notes = [json.loads(x) for x in open(f"{T}/notes.jsonl")]
by = {}
for x in notes: by.setdefault(x["burst"], []).append(x)
b2, b1 = sorted(by)   # u-1700000000000 (b2), u-1700000600000 (b1)
for b in (b2, b1):
    print(f"--- burst {b} ---")
    for x in by[b]: print(f"  -> {(x['target'] or {}).get('name') or 'no target'} [{x['why']}]\n     {x['text']}")
ok1 = any((x["target"] or {}).get("name") == "Voice idea capture dashboard" and "mic" in x["text"].lower()
          and ("sun board" in x["text"].lower() or "sunboard" in x["text"].lower()) and "copy" in x["text"].lower() for x in by[b1])
ok2 = any(re.search(r"voice idea capture|sun board", (x["target"] or {}).get("name") or "", re.I)
          or (x["target"] is None and "board" in x["why"].lower()) for x in by[b2])
print(f"burst 1 (the 20:03 ramble): {'PASS' if ok1 else 'FAIL'}")
print(f"burst 2 (the board says nothing runs): {'PASS' if ok2 else 'FAIL'}")
sys.exit(0 if ok1 and ok2 else 1)
PY
