"""voice-flow: where the words go and how, decided by the focused window.

Runs bin/voice-flow start then stop with stubbed hyprctl, ydotool, the
clipboard and the engine. Run: python3 -m unittest discover -s tests -v
"""
from __future__ import annotations

import shutil
import subprocess
import unittest

from stubs import BIN, REPO, log_lines, make_env, read_log_file, tmpdir

CTRL_V = "key 29:1 47:1 47:0 29:0"
CTRL_SHIFT_V = "key 29:1 42:1 47:1 47:0 42:0 29:0"
LONG = "These words are long enough that voice-flow pastes them instead of typing them one by one."


def dictate(env: dict) -> subprocess.CompletedProcess:
    start = subprocess.run([str(BIN / "voice-flow")], env=env, capture_output=True, text=True)
    assert start.returncode == 0, start.stderr
    return subprocess.run([str(BIN / "voice-flow")], env=env, capture_output=True, text=True)


class Inject(unittest.TestCase):
    def setUp(self):
        self.tmp = tmpdir()
        self.env = make_env(self.tmp, VOICE_FLOW_PILL="0", STUB_TEXT=LONG)
        conf = self.tmp / "config" / "ormus-voice"
        conf.mkdir(parents=True)
        shutil.copy(REPO / "config" / "app-profiles.conf", conf / "app-profiles.conf")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_terminal_pastes_with_ctrl_shift_v_and_no_rewrite(self):
        self.env["STUB_CLASS"] = "foot"
        r = dictate(self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn(CTRL_SHIFT_V, log_lines(self.env, "ydotool"))
        self.assertEqual(read_log_file(self.env, "rewrite_cmd.txt"), "")

    def test_workboard_window_is_a_terminal(self):
        self.env["STUB_CLASS"] = "org.ormus.workboard.ormus"
        dictate(self.env)
        self.assertIn(CTRL_SHIFT_V, log_lines(self.env, "ydotool"))
        self.assertEqual(read_log_file(self.env, "rewrite_cmd.txt"), "")

    def test_browser_pastes_with_ctrl_v_and_rewrites(self):
        self.env["STUB_CLASS"] = "firefox"
        dictate(self.env)
        self.assertIn(CTRL_V, log_lines(self.env, "ydotool"))
        self.assertEqual(read_log_file(self.env, "rewrite_cmd.txt"), "voice-rewrite-ollama")
        clipboard = read_log_file(self.env, "clipboard.txt").split("\n--\n")
        self.assertEqual(clipboard[0], LONG)
        self.assertEqual(clipboard[1], "OLD")  # the old clipboard comes back

    def test_line_breaks_are_typed_as_spaces(self):
        self.env["STUB_CLASS"] = "firefox"
        self.env["STUB_TEXT"] = "first line\nsecond line"
        dictate(self.env)
        self.assertEqual(read_log_file(self.env, "typed.txt"), "first line second line")

    def test_history_row_counts_the_dictation(self):
        self.env["STUB_CLASS"] = "firefox"
        dictate(self.env)
        rows = (self.tmp / "data" / "ormus-voice" / "history.jsonl").read_text().splitlines()
        self.assertEqual(len(rows), 1)
        self.assertIn('"method":"paste"', rows[0])
        self.assertNotIn('"text"', rows[0])  # counts only unless VOICE_FLOW_LOG_TEXT=1


if __name__ == "__main__":
    unittest.main()
