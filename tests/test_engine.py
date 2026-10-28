"""whisper-paste and voice-train: the silence gate, the remote failover and the CPU-only fallback.

Run: python3 -m unittest discover -s tests -v
"""
from __future__ import annotations

import shutil
import subprocess
import unittest

from stubs import BIN, DEAD_PID, log_lines, make_env, silent_wav, tmpdir


class WhisperPaste(unittest.TestCase):
    def setUp(self):
        self.tmp = tmpdir()
        self.env = make_env(self.tmp, VOICE_PASTE_QUIET="1")
        models = self.tmp / "data" / "ormus-voice" / "models"
        models.mkdir(parents=True)
        (models / "ggml-base.en.bin").write_bytes(b"model")
        run = self.tmp / "run" / "ormus-voice"
        run.mkdir(parents=True)
        self.wav = silent_wav(self.tmp / "take.wav")
        (run / "recording.state").write_text(f"{DEAD_PID}:{self.wav}")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def stop(self) -> subprocess.CompletedProcess:
        self.env["VOICE_PASTE_PHASE"] = "stop"
        return subprocess.run([str(BIN / "whisper-paste")], env=self.env, capture_output=True, text=True)

    def test_silence_gate_drops_a_quiet_take(self):
        self.env["STUB_RMS"] = "0.0010"
        r = self.stop()
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout, "")
        self.assertEqual(log_lines(self.env, "curl"), [])

    def test_remote_text_passes_through(self):
        self.env["STUB_CURL_OUT"] = "  Hello there.  "
        r = self.stop()
        self.assertEqual(r.stdout.strip(), "Hello there.")
        self.assertEqual(log_lines(self.env, "whisper-cli"), [])

    def test_empty_remote_answer_is_not_a_fallback(self):
        self.env["STUB_CURL_OUT"] = ""
        r = self.stop()
        self.assertEqual(r.stdout, "")
        self.assertEqual(log_lines(self.env, "whisper-cli"), [])

    def test_local_fallback_is_cpu_only(self):
        self.env["STUB_CURL_EXIT"] = "7"
        r = self.stop()
        self.assertEqual(r.stdout.strip(), "LOCAL")
        calls = log_lines(self.env, "whisper-cli")
        self.assertEqual(len(calls), 1)
        self.assertIn(" -ng ", " " + calls[0] + " ")
        self.assertNotIn("-dev", calls[0])


class VoiceTrain(unittest.TestCase):
    def setUp(self):
        self.tmp = tmpdir()
        self.env = make_env(self.tmp, VOICE_TRAIN_TAKES="1")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_an_answering_host_that_hears_nothing_is_final(self):
        self.env["STUB_CURL_OUT"] = ""
        r = subprocess.run([str(BIN / "voice-train"), "Embry0"], env=self.env, capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("heard nothing", r.stdout)
        self.assertEqual(log_lines(self.env, "whisper-cli"), [])

    def test_local_fallback_is_cpu_only(self):
        self.env["STUB_CURL_EXIT"] = "7"
        subprocess.run([str(BIN / "voice-train"), "Embry0"], env=self.env, capture_output=True, text=True)
        calls = log_lines(self.env, "whisper-cli")
        self.assertEqual(len(calls), 1)
        self.assertIn(" -ng ", " " + calls[0] + " ")


if __name__ == "__main__":
    unittest.main()
