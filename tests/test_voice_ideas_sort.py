"""voice-ideas-sort: bursts of utterances become notes with a target agent, once each.

A fake LLM stands in for Ollama; the targets are two fake board rows. The live judgment on
Ormus's real transcripts is tests/eval_sort.sh (needs sg).
"""
from __future__ import annotations

import importlib.machinery
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load_sorter():
    path = HERE.parent / "bin" / "voice-ideas-sort"
    loader = importlib.machinery.SourceFileLoader("voice_ideas_sort", str(path))
    spec = importlib.util.spec_from_loader("voice_ideas_sort", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


T0 = 1_700_000_000.0  # epoch of the first utterance


def utt(i: int, at: float, dur: float, text: str) -> dict:
    from datetime import datetime, timezone
    iso = lambda s: datetime.fromtimestamp(s, timezone.utc).isoformat(timespec="seconds")  # noqa: E731
    return {"id": f"u-{int(at * 1000)}", "t0": iso(at), "t1": iso(at + dur), "dur": dur, "text": text, "lp": -0.5, "backend": "x"}


UTTS = [
    utt(1, T0 + 0, 3, "the sun board should show my voice notes"),
    utt(2, T0 + 8, 2, "and send them to the session"),
    utt(3, T0 + 15, 4, "also the raven rate team switch is broken"),
    utt(4, T0 + 90, 2, "oh and call Laura"),           # a second burst, 71 s later
    utt(5, T0 + 95, 2, "about the Alqvimia catalog"),
]
TARGETS = [
    {"key": "sun|default|w1:p1", "host": "sun", "session": "default", "workspace": "DESK", "pane": "w1:p1", "agent": "claude",
     "name": "Sun board voice ideas", "status": "working"},
    {"key": "sg|raven|w2:p3", "host": "sg", "session": "raven", "workspace": "CUSTOMER PORTALS", "pane": "w2:p3", "agent": "claude",
     "name": "Raven Rate", "status": "idle"},
]


class FakeLLM:
    def __init__(self, answers):
        self.answers, self.calls = list(answers), []

    def __call__(self, system: str, user: str) -> str:
        self.calls.append((system, user))
        return self.answers.pop(0)


class Bursts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.vs = load_sorter()

    def test_gaps_split_bursts(self):
        b = self.vs.bursts(UTTS)
        self.assertEqual([[u["id"] for u in x] for x in b], [[u["id"] for u in UTTS[:3]], [u["id"] for u in UTTS[3:]]])

    def test_a_burst_closes_after_the_gap(self):
        b = self.vs.bursts(UTTS)
        self.assertTrue(self.vs.closed(b[0], now=T0 + 19 + 20))
        self.assertFalse(self.vs.closed(b[1], now=T0 + 97 + 5))
        self.assertTrue(self.vs.closed(b[1], now=T0 + 97 + 20))


class Targets(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.vs = load_sorter()

    def test_rows_come_from_the_board_docs(self):
        sun = {"host": "sun", "sessions": [{"name": "default", "workspaces": [{"label": "DESK", "agents": [
            {"pane": "w1:p1", "tab_label": "1 · Shell", "agent": "claude", "status": "working", "title": "Sun board voice ideas"}]}]}]}
        fleet = {"docs": {"sg": {"sessions": [{"name": "raven", "workspaces": [{"label": "CUSTOMER PORTALS", "agents": [
            {"pane": "w2:p3", "tab_label": "Raven Rate", "agent": "claude", "status": "idle", "title": "Claude Code"}]}]}]},
                          "corvin-server": {"sessions": [{"name": "x", "workspaces": [{"label": "Y", "agents": [
            {"pane": "w1:p1", "tab_label": "not reachable", "agent": "grok", "status": "idle", "title": "t"}]}]}]}}}
        rows = self.vs.targets_from(sun, fleet)
        self.assertEqual([r["key"] for r in rows], ["sun|default|w1:p1", "sg|raven|w2:p3"], "sg yes, other machines no")
        self.assertEqual(rows[0]["name"], "Sun board voice ideas", "the title names it")
        self.assertEqual(rows[1]["name"], "Raven Rate", "a generic title falls back to the tab label")
        self.assertEqual(rows[1]["workspace"], "CUSTOMER PORTALS")


class Sort(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.vs = load_sorter()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.utts, self.notes = root / "utterances.jsonl", root / "notes.jsonl"
        self.utts.write_text("".join(json.dumps(u) + "\n" for u in UTTS))

    def tearDown(self):
        self.tmp.cleanup()

    def rows(self):
        return [json.loads(l) for l in self.notes.read_text().splitlines()] if self.notes.exists() else []

    def test_a_closed_burst_becomes_notes_with_valid_targets_only(self):
        llm = FakeLLM([json.dumps({"notes": [
            {"text": "The Sun board should show my voice notes and send them to the session.", "target": "sun|default|w1:p1", "why": "board"},
            {"text": "The Raven Rate team switch is broken.", "target": "sg|raven|w9:p9", "why": "made up"},
            {"text": "A note with no home.", "target": None, "why": "none"}]})])
        n = self.vs.sort_once(self.utts, self.notes, llm, TARGETS, now=T0 + 19 + 25)
        self.assertEqual(n, 3)
        rows = self.rows()
        self.assertEqual([r["burst"] for r in rows], [UTTS[0]["id"]] * 3)
        self.assertEqual(rows[0]["target"]["key"], "sun|default|w1:p1")
        self.assertEqual(rows[0]["target"]["name"], "Sun board voice ideas")
        self.assertIsNone(rows[1]["target"], "a target not on the list is no target")
        self.assertIsNone(rows[2]["target"])
        self.assertEqual(rows[0]["utterances"], [u["id"] for u in UTTS[:3]])
        self.assertTrue(all(r["id"].startswith("n-") for r in rows))
        self.assertIn("the sun board should show my voice notes", llm.calls[0][1], "the burst text reaches the model")
        self.assertIn("sg|raven|w2:p3", llm.calls[0][1], "and the target keys")

    def test_facets_to_the_same_agent_join_into_one_note(self):
        llm = FakeLLM([json.dumps({"notes": [
            {"text": "speak into the mic and it hears everything", "target": "sun|default|w1:p1", "why": "a"},
            {"text": "then a dashboard I can copy from", "target": "sun|default|w1:p1", "why": "b"},
            {"text": "the raven rate team switch is broken", "target": "sg|raven|w2:p3", "why": "c"},
            {"text": "loose end one", "target": None, "why": "d"},
            {"text": "loose end two", "target": None, "why": "e"}]})])
        self.vs.sort_once(self.utts, self.notes, llm, TARGETS, now=T0 + 19 + 25)
        rows = self.rows()
        self.assertEqual([(r["target"] or {}).get("key") for r in rows], ["sun|default|w1:p1", "sg|raven|w2:p3", None, None])
        self.assertEqual(rows[0]["text"], "speak into the mic and it hears everything\nthen a dashboard I can copy from")
        self.assertEqual(rows[0]["why"], "a")

    def test_an_open_burst_waits_and_a_sorted_one_is_not_sorted_twice(self):
        llm = FakeLLM([json.dumps({"notes": [{"text": "x", "target": None, "why": ""}]})] * 3)
        self.vs.sort_once(self.utts, self.notes, llm, TARGETS, now=T0 + 19 + 25)
        self.assertEqual(len(llm.calls), 1, "burst 2 is still open")
        self.vs.sort_once(self.utts, self.notes, llm, TARGETS, now=T0 + 19 + 25)
        self.assertEqual(len(llm.calls), 1, "burst 1 is sorted already")
        self.vs.sort_once(self.utts, self.notes, llm, TARGETS, now=T0 + 97 + 25)
        self.assertEqual(len(llm.calls), 2, "burst 2 closed")
        self.assertEqual([r["burst"] for r in self.rows()], [UTTS[0]["id"], UTTS[3]["id"]])

    def test_bad_json_or_no_model_gives_one_raw_note(self):
        self.vs.sort_once(self.utts, self.notes, FakeLLM(["I cannot do that"]), TARGETS, now=T0 + 19 + 25)
        rows = self.rows()
        self.assertEqual(len(rows), 1)
        self.assertIsNone(rows[0]["target"])
        self.assertEqual(rows[0]["text"], " ".join(u["text"] for u in UTTS[:3]))
        self.assertEqual(rows[0]["why"], "unsorted: the sorter gave no JSON")

    def test_no_targets_still_makes_notes(self):
        llm = FakeLLM([json.dumps({"notes": [{"text": "t", "target": "sun|default|w1:p1", "why": ""}]})])
        self.vs.sort_once(self.utts, self.notes, llm, [], now=T0 + 19 + 25)
        self.assertIsNone(self.rows()[0]["target"], "no board, no target, but the note lands")


if __name__ == "__main__":
    unittest.main()
