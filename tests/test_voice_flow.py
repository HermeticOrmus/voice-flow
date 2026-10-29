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

    def test_terminal_pastes_with_ctrl_shift_v_and_rewrites(self):
        self.env["STUB_CLASS"] = "foot"
        r = dictate(self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn(CTRL_SHIFT_V, log_lines(self.env, "ydotool"))
        self.assertEqual(read_log_file(self.env, "rewrite_cmd.txt"), "voice-rewrite-ollama")

    def test_workboard_window_is_a_terminal_and_rewrites(self):
        self.env["STUB_CLASS"] = "org.ormus.workboard.ormus"
        dictate(self.env)
        self.assertIn(CTRL_SHIFT_V, log_lines(self.env, "ydotool"))
        self.assertEqual(read_log_file(self.env, "rewrite_cmd.txt"), "voice-rewrite-ollama")

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


class Express(unittest.TestCase):
    """Express is the tone of the cleanup: formal, casual, or verbatim (no rewrite)."""

    def setUp(self):
        self.tmp = tmpdir()
        self.env = make_env(self.tmp, VOICE_FLOW_PILL="0", STUB_TEXT=LONG, STUB_CLASS="firefox")
        conf = self.tmp / "config" / "ormus-voice"
        conf.mkdir(parents=True)
        shutil.copy(REPO / "config" / "app-profiles.conf", conf / "app-profiles.conf")
        self.conf = conf / "voice-flow.conf"

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_verbatim_skips_the_rewrite(self):
        self.conf.write_text("VOICE_FLOW_EXPRESS=verbatim\n")
        dictate(self.env)
        self.assertEqual(read_log_file(self.env, "rewrite_cmd.txt"), "")

    def test_casual_keeps_the_rewrite(self):
        self.conf.write_text("VOICE_FLOW_EXPRESS=casual\n")
        dictate(self.env)
        self.assertEqual(read_log_file(self.env, "rewrite_cmd.txt"), "voice-rewrite-ollama")

    def test_unknown_value_reads_as_prompt_and_rewrites(self):
        self.conf.write_text("VOICE_FLOW_EXPRESS=shouty\n")
        r = dictate(self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(read_log_file(self.env, "rewrite_cmd.txt"), "voice-rewrite-ollama")

    def test_a_listed_app_stays_verbatim_whatever_the_express(self):
        self.conf.write_text("VOICE_FLOW_EXPRESS=formal\n")
        (self.conf.parent / "app-profiles.conf").write_text("foot  verbatim\n")
        self.env["STUB_CLASS"] = "foot"
        dictate(self.env)
        self.assertEqual(read_log_file(self.env, "rewrite_cmd.txt"), "")


class FocusGuard(unittest.TestCase):
    """A window switch during transcription sends the text to the clipboard, never to the new window."""

    def setUp(self):
        self.tmp = tmpdir()
        self.env = make_env(self.tmp, VOICE_FLOW_PILL="0", STUB_TEXT=LONG, STUB_CLASS="firefox")
        conf = self.tmp / "config" / "ormus-voice"
        conf.mkdir(parents=True)
        shutil.copy(REPO / "config" / "app-profiles.conf", conf / "app-profiles.conf")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_focus_moved_diverts_to_the_clipboard(self):
        # Stop-phase hyprctl calls: 1 profile, 2 focus before, 3 focus after.
        self.env["STUB_FOCUS_MOVE_AT"] = "3"
        dictate(self.env)
        self.assertEqual([line for line in log_lines(self.env, "ydotool") if line.startswith("key")], [])
        self.assertEqual(read_log_file(self.env, "clipboard.txt").split("\n--\n")[0], LONG)
        row = (self.tmp / "data" / "ormus-voice" / "history.jsonl").read_text()
        self.assertIn('"method":"clipboard-divert"', row)
        self.assertIn('"diverted":1', row)

    def test_focus_steady_injects(self):
        dictate(self.env)
        self.assertIn(CTRL_V, log_lines(self.env, "ydotool"))
        self.assertIn('"diverted":0', (self.tmp / "data" / "ormus-voice" / "history.jsonl").read_text())


class Forecast(unittest.TestCase):
    """The pill says what will happen to the words: verbatim or the Express style on start, the paste key on a divert."""

    def setUp(self):
        self.tmp = tmpdir()
        self.env = make_env(self.tmp, VOICE_FLOW_PILL="1", STUB_TEXT=LONG)
        conf = self.tmp / "config" / "ormus-voice"
        conf.mkdir(parents=True)
        shutil.copy(REPO / "config" / "app-profiles.conf", conf / "app-profiles.conf")
        self.conf = conf / "voice-flow.conf"
        self.pill_state = self.tmp / "run" / "ormus-voice" / "pill.state"

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def start(self):
        r = subprocess.run([str(BIN / "voice-flow")], env=self.env, capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_a_terminal_forecasts_the_prompt_style(self):
        self.env["STUB_CLASS"] = "foot"
        self.start()
        self.assertEqual(self.pill_state.read_text(), "listening:prompt")

    def test_a_listed_app_forecasts_verbatim(self):
        (self.conf.parent / "app-profiles.conf").write_text("foot  verbatim\n")
        self.env["STUB_CLASS"] = "foot"
        self.start()
        self.assertEqual(self.pill_state.read_text(), "listening:verbatim")

    def test_a_browser_forecasts_the_prompt_style(self):
        self.env["STUB_CLASS"] = "firefox"
        self.start()
        self.assertEqual(self.pill_state.read_text(), "listening:prompt")

    def test_a_chosen_express_is_the_forecast(self):
        self.env["STUB_CLASS"] = "firefox"
        self.conf.write_text("VOICE_FLOW_EXPRESS=formal\n")
        self.start()
        self.assertEqual(self.pill_state.read_text(), "listening:formal")

    def test_express_verbatim_forecasts_verbatim(self):
        self.env["STUB_CLASS"] = "firefox"
        self.conf.write_text("VOICE_FLOW_EXPRESS=verbatim\n")
        self.start()
        self.assertEqual(self.pill_state.read_text(), "listening:verbatim")

    def test_rewrite_off_forecasts_verbatim(self):
        self.env["STUB_CLASS"] = "firefox"
        self.conf.write_text("VOICE_FLOW_REWRITE=off\n")
        self.start()
        self.assertEqual(self.pill_state.read_text(), "listening:verbatim")

    def test_a_divert_in_a_terminal_names_ctrl_shift_v(self):
        # hyprctl calls with the pill on: 1 forecast, 2 profile, 3 focus before, 4 focus after.
        self.env.update(STUB_CLASS="foot", STUB_FOCUS_MOVE_AT="4")
        dictate(self.env)
        self.assertTrue(self.pill_state.read_text().startswith("copied:Ctrl+Shift+V "), self.pill_state.read_text())
        self.assertIn("Ctrl+Shift+V", "\n".join(log_lines(self.env, "notify-send")))

    def test_a_divert_elsewhere_names_ctrl_v(self):
        self.env.update(STUB_CLASS="firefox", STUB_FOCUS_MOVE_AT="4")
        dictate(self.env)
        self.assertTrue(self.pill_state.read_text().startswith("copied:Ctrl+V "), self.pill_state.read_text())
