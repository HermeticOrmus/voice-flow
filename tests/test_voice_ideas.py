"""voice-ideas: the listener's cut, filter, interlock, spool and log, on a synthetic fixture.

The fixture is three piper sentences on a bed of low white noise (tests/fixtures/three-sentences.wav,
planted times in three-sentences.json). The real-room precision was settled by the spike
(docs/spikes/vad-game-capture.md); this checks the mechanics the daemon runs.

Run: ./check.sh (uv supplies onnxruntime and numpy).
"""
from __future__ import annotations

import importlib.machinery
import importlib.util
import json
import os
import tempfile
import unittest
import wave
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
FIXTURE = HERE / "fixtures" / "three-sentences.wav"
PLANTED = json.loads((HERE / "fixtures" / "three-sentences.json").read_text())["sentences"]


def load_listener():
    path = HERE.parent / "bin" / "voice-ideas"
    loader = importlib.machinery.SourceFileLoader("voice_ideas", str(path))
    spec = importlib.util.spec_from_loader("voice_ideas", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


def read_fixture() -> np.ndarray:
    with wave.open(str(FIXTURE), "rb") as w:
        assert w.getframerate() == 16000 and w.getnchannels() == 1 and w.getsampwidth() == 2
        return np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16)


class Fake:
    """A transcriber that answers from a script, or fails like a server that is down."""

    def __init__(self, answers=None, down=False):
        self.answers, self.down, self.calls = list(answers or []), down, []

    def __call__(self, pcm):
        self.calls.append(len(pcm) / 16000)
        if self.down:
            return None
        return self.answers.pop(0) if self.answers else {"text": "ok", "lp": -0.5, "backend": "fake"}


class Cut(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.vi = load_listener()
        cls.pcm = read_fixture()

    def test_three_sentences_three_segments_at_the_planted_times(self):
        segs = list(self.vi.cut(self.pcm, self.vi.Vad(self.vi.ensure_model())))
        self.assertEqual(len(segs), len(PLANTED), [(round(a, 1), round(b, 1)) for a, b, _ in segs])
        for (a, b, _), p in zip(segs, PLANTED):
            self.assertAlmostEqual(a, p["t0"], delta=0.35, msg=f"start of {p['text']!r}")
            self.assertAlmostEqual(b, p["t1"], delta=0.45, msg=f"end of {p['text']!r}")

    def test_noise_alone_cuts_nothing(self):
        noise = (np.random.default_rng(1).standard_normal(16000 * 8) * 0.01 * 32768).astype(np.int16)
        self.assertEqual(list(self.vi.cut(noise, self.vi.Vad(self.vi.ensure_model()))), [])


class Keep(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.vi = load_listener()

    def test_fixed_list_and_repeats_and_low_logprob_are_dropped(self):
        keep = self.vi.keep
        self.assertFalse(keep("Thank you.", -0.6))
        self.assertFalse(keep("you", -0.9))
        self.assertFalse(keep("DONG, DONG, DONG, DONG, DONG!", -0.8))
        self.assertFalse(keep("Inhuman fan, Ezra Callais TT", -6.1))
        self.assertTrue(keep("What were we saying?", -0.62))
        self.assertTrue(keep("¿Qué es esto? ¿Qué ocurre?", -0.77))


class Process(unittest.TestCase):
    """The loop the daemon runs: chunks in, log lines out, with the dictation interlock and the spool."""

    @classmethod
    def setUpClass(cls):
        cls.vi = load_listener()
        cls.pcm = read_fixture()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.log, self.spool, self.flag = root / "utterances.jsonl", root / "spool", root / "recording.state"

    def tearDown(self):
        self.tmp.cleanup()

    def lines(self):
        return [json.loads(l) for l in self.log.read_text().splitlines()] if self.log.exists() else []

    def run_loop(self, transcriber, flag_during=None):
        vi = self.vi
        store = vi.Store(self.log, self.spool)
        flag_on = lambda t: flag_during is not None and flag_during[0] <= t <= flag_during[1]  # noqa: E731
        vi.process(vi.chunks_of(self.pcm), vi.Vad(vi.ensure_model()), transcriber, store,
                   dictation=lambda t: flag_on(t), clock=lambda t: 1_700_000_000.0 + t)
        return store

    def test_one_line_per_sentence_in_order(self):
        fake = Fake([{"text": p["text"], "lp": -0.4, "backend": "fake"} for p in PLANTED])
        self.run_loop(fake)
        rows = self.lines()
        self.assertEqual([r["text"] for r in rows], [p["text"] for p in PLANTED])
        for r in rows:
            self.assertEqual(set(r) >= {"id", "t0", "t1", "dur", "text", "lp", "backend"}, True, r)
            self.assertGreater(r["dur"], 0.5)
        self.assertEqual(len(fake.calls), len(PLANTED))

    def test_a_dictation_in_flight_drops_that_speech(self):
        mid = PLANTED[1]
        fake = Fake([{"text": p["text"], "lp": -0.4, "backend": "fake"} for p in (PLANTED[0], PLANTED[2])])
        self.run_loop(fake, flag_during=(mid["t0"] - 0.5, mid["t1"] + 0.5))
        self.assertEqual([r["text"] for r in self.lines()], [PLANTED[0]["text"], PLANTED[2]["text"]])

    def test_server_down_spools_then_drains(self):
        self.run_loop(Fake(down=True))
        self.assertEqual(self.lines(), [])
        self.assertEqual(len(list(self.spool.glob("*.wav"))), len(PLANTED))
        store = self.vi.Store(self.log, self.spool)
        store.drain(Fake([{"text": p["text"], "lp": -0.4, "backend": "fake"} for p in PLANTED]))
        self.assertEqual([r["text"] for r in self.lines()], [p["text"] for p in PLANTED])
        self.assertEqual(list(self.spool.glob("*.wav")), [])

    def test_junk_is_not_logged(self):
        self.run_loop(Fake([{"text": "Thank you.", "lp": -0.6, "backend": "fake"},
                            {"text": PLANTED[1]["text"], "lp": -0.4, "backend": "fake"},
                            {"text": "zzz", "lp": -5.0, "backend": "fake"}]))
        self.assertEqual([r["text"] for r in self.lines()], [PLANTED[1]["text"]])


if __name__ == "__main__":
    unittest.main()
