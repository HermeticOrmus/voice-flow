"""whisper-paste and voice-train: the silence gate, the remote failover and the CPU-only fallback.

Run: python3 -m unittest discover -s tests -v
"""
from __future__ import annotations

import shutil
import subprocess
import unittest
from pathlib import Path

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
        r = subprocess.run([str(BIN / "voice-train"), "PyTorch"], env=self.env, capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("heard nothing", r.stdout)
        self.assertEqual(log_lines(self.env, "whisper-cli"), [])

    def test_local_fallback_is_cpu_only(self):
        self.env["STUB_CURL_EXIT"] = "7"
        subprocess.run([str(BIN / "voice-train"), "PyTorch"], env=self.env, capture_output=True, text=True)
        calls = log_lines(self.env, "whisper-cli")
        self.assertEqual(len(calls), 1)
        self.assertIn(" -ng ", " " + calls[0] + " ")


if __name__ == "__main__":
    unittest.main()


class Hosts(unittest.TestCase):
    """Speech goes to this machine's :8093 first unless voice-flow.conf names the hosts; the LLM
    steps go to this machine's Ollama unless it names others."""

    def setUp(self):
        self.tmp = tmpdir()
        self.env = make_env(self.tmp, VOICE_PASTE_QUIET="1", VOICE_TRAIN_TAKES="1")
        models = self.tmp / "data" / "ormus-voice" / "models"
        models.mkdir(parents=True)
        (models / "ggml-base.en.bin").write_bytes(b"model")
        run = self.tmp / "run" / "ormus-voice"
        run.mkdir(parents=True)
        self.wav = silent_wav(self.tmp / "take.wav")
        (run / "recording.state").write_text(f"{DEAD_PID}:{self.wav}")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_whisper_paste_asks_this_machine_first(self):
        self.env.update(VOICE_PASTE_PHASE="stop", STUB_CURL_OUT="Hi.")
        subprocess.run([str(BIN / "whisper-paste")], env=self.env, capture_output=True, text=True)
        self.assertIn("http://127.0.0.1:8093/inference", log_lines(self.env, "curl")[0])

    def test_voice_train_asks_this_machine_first(self):
        self.env["STUB_CURL_OUT"] = "Hi."
        subprocess.run([str(BIN / "voice-train"), "PyTorch"], env=self.env, capture_output=True, text=True)
        self.assertIn("http://127.0.0.1:8093/inference", log_lines(self.env, "curl")[0])

    def test_rewrite_runs_qwen_on_this_machine_by_default(self):
        self.env["STUB_OLLAMA_RESPONSE"] = '{"response":"Hello there."}'
        r = subprocess.run([str(BIN / "voice-rewrite-ollama")], input="um hello there",
                           env=self.env, capture_output=True, text=True)
        self.assertEqual(r.stdout.strip(), "Hello there.")
        log = "\n".join(log_lines(self.env, "curl"))
        self.assertTrue(log.startswith("-s --connect-timeout 0.4 -m 1 http://127.0.0.1:11434/api/version"), log[:120])
        self.assertIn('"model": "qwen3:32b"', log)
        self.assertIn('"think": false', log)

    def test_rewrite_passes_through_when_no_host_answers(self):
        self.env["STUB_CURL_EXIT"] = "7"
        r = subprocess.run([str(BIN / "voice-rewrite-ollama")], input="um hello there",
                           env=self.env, capture_output=True, text=True)
        self.assertEqual(r.stdout.strip(), "um hello there")

    def test_agent_asks_this_machine_by_default(self):
        self.env["STUB_OLLAMA_RESPONSE"] = '{"response":"42"}'
        r = subprocess.run([str(BIN / "voice-agent-ollama")], input="what is six times seven",
                           env=self.env, capture_output=True, text=True)
        self.assertEqual(r.stdout.strip(), "42")
        self.assertIn("http://127.0.0.1:11434/api/version", log_lines(self.env, "curl")[0])

    def conf(self, text: str) -> None:
        d = self.tmp / "config" / "ormus-voice"
        d.mkdir(parents=True, exist_ok=True)
        (d / "voice-flow.conf").write_text(text)

    def test_the_conf_names_the_hosts(self):
        """Each tool reads voice-flow.conf itself, run by voice-flow or on its own."""
        self.conf('VOICE_FLOW_METHOD=auto\nVOICE_PASTE_REMOTE_HOSTS="192.0.2.7:8093 127.0.0.1:8093"\n'
                  'OLLAMA_HOSTS="http://192.0.2.7:11434"\n')
        self.env.update(VOICE_PASTE_PHASE="stop", STUB_CURL_OUT="Hi.", STUB_OLLAMA_RESPONSE='{"response":"x"}')
        for tool, args, want in (("whisper-paste", [], "http://192.0.2.7:8093/inference"),
                                 ("voice-train", ["PyTorch"], "http://192.0.2.7:8093/inference"),
                                 ("voice-rewrite-ollama", [], "http://192.0.2.7:11434/api/version"),
                                 ("voice-agent-ollama", [], "http://192.0.2.7:11434/api/version")):
            (Path(self.env["STUB_LOG"]) / "curl.log").unlink(missing_ok=True)
            subprocess.run([str(BIN / tool), *args], input="hello", env=self.env, capture_output=True, text=True)
            self.assertIn(want, log_lines(self.env, "curl")[0], tool)

    def test_the_environment_wins_over_the_conf(self):
        self.conf('OLLAMA_HOSTS="http://192.0.2.7:11434"\n')
        self.env.update(OLLAMA_HOSTS="http://198.51.100.3:11434", STUB_OLLAMA_RESPONSE='{"response":"x"}')
        subprocess.run([str(BIN / "voice-rewrite-ollama")], input="hello", env=self.env, capture_output=True, text=True)
        self.assertIn("http://198.51.100.3:11434/api/version", log_lines(self.env, "curl")[0])


class Style(unittest.TestCase):
    """voice-rewrite-ollama puts the Express tone into its prompt."""

    def setUp(self):
        self.tmp = tmpdir()
        self.env = make_env(self.tmp, STUB_OLLAMA_RESPONSE='{"response":"Hello."}')

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def prompt_sent(self, express: str | None) -> str:
        if express is not None:
            self.env["VOICE_FLOW_EXPRESS"] = express
        subprocess.run([str(BIN / "voice-rewrite-ollama")], input="um hello",
                       env=self.env, capture_output=True, text=True)
        return "\n".join(log_lines(self.env, "curl"))

    def test_casual_style_line(self):
        self.assertIn("STYLE: Casual.", self.prompt_sent("casual"))

    def test_formal_style_line(self):
        self.assertIn("STYLE: Formal.", self.prompt_sent("formal"))

    def test_prompt_is_the_default_style(self):
        self.assertIn("STYLE: Prompt.", self.prompt_sent(None))

    def test_unknown_value_reads_as_prompt(self):
        self.assertIn("STYLE: Prompt.", self.prompt_sent("shouty"))

    def test_the_prompt_style_keeps_names_and_drops_the_trailing_thought(self):
        sent = self.prompt_sent("prompt")
        self.assertIn("Keep technical names verbatim", sent)
        self.assertIn("trails off", sent)
        self.assertIn("Never answer the prompt", sent)

    def test_a_long_rant_gets_a_longer_wait_and_a_wider_context(self):
        subprocess.run([str(BIN / "voice-rewrite-ollama")], input="word " * 600,  # 3000 chars
                       env=self.env, capture_output=True, text=True)
        sent = "\n".join(log_lines(self.env, "curl"))  # the body spans lines
        self.assertIn(" -m 45 ", sent)  # 15 s floor + 3000 chars / 100
        self.assertIn('"num_ctx": 16384', sent)


JUNK = ('{"text":"Vasime glaha, haiu, ro只是信 Мария gault, více, 보라, Tapia, köde, Glence. Thank you.",'
        '"segments":[{"text":"Vasime glaha, haiu, ro只是信 Мария gault, více, 보라, Tapia, köde, Glence.","avg_logprob":-5.92},'
        '{"text":" Thank you.","avg_logprob":-0.66}]}')
SPANISH = '{"text":"¿Qué es esto? ¿Qué ocurre?","segments":[{"text":"¿Qué es esto? ¿Qué ocurre?","avg_logprob":-0.98}]}'
NO_SEGMENTS = '{"text":"plain server answer"}'


class JunkGate(unittest.TestCase):
    """Whisper's own confidence decides: a transcript with a low log-prob is junk and is dropped."""

    def setUp(self):
        self.tmp = tmpdir()
        self.env = make_env(self.tmp, VOICE_PASTE_QUIET="1", VOICE_PASTE_PHASE="stop")
        models = self.tmp / "data" / "ormus-voice" / "models"
        models.mkdir(parents=True)
        (models / "ggml-base.en.bin").write_bytes(b"model")
        run = self.tmp / "run" / "ormus-voice"
        run.mkdir(parents=True)
        (run / "recording.state").write_text(f"{DEAD_PID}:{silent_wav(self.tmp / 'take.wav')}")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def stop(self, answer: str) -> subprocess.CompletedProcess:
        self.env["STUB_CURL_OUT"] = answer
        return subprocess.run([str(BIN / "whisper-paste")], env=self.env, capture_output=True, text=True)

    def test_the_2018_junk_is_dropped(self):
        r = self.stop(JUNK)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout, "")
        self.assertIn("junk", (self.tmp / "run" / "ormus-voice" / "last-drop").read_text())

    def test_spanish_speech_passes(self):
        self.assertEqual(self.stop(SPANISH).stdout.strip(), "¿Qué es esto? ¿Qué ocurre?")

    def test_a_server_without_segments_is_not_gated(self):
        self.assertEqual(self.stop(NO_SEGMENTS).stdout.strip(), "plain server answer")

    def test_a_text_only_server_still_works(self):
        self.assertEqual(self.stop("Hello there.").stdout.strip(), "Hello there.")

    def test_silence_names_its_reason(self):
        self.env["STUB_RMS"] = "0.0010"
        self.stop(SPANISH)
        self.assertIn("silent", (self.tmp / "run" / "ormus-voice" / "last-drop").read_text())


class TakeLength(unittest.TestCase):
    """A long take is kept whole: no two-minute cap on the recorder, and whisper gets time for the length of the take."""

    def setUp(self):
        self.tmp = tmpdir()
        self.env = make_env(self.tmp, VOICE_PASTE_QUIET="1")
        models = self.tmp / "data" / "ormus-voice" / "models"
        models.mkdir(parents=True)
        (models / "ggml-base.en.bin").write_bytes(b"model")
        (self.tmp / "run" / "ormus-voice").mkdir(parents=True)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_the_recorder_runs_an_hour_not_two_minutes(self):
        self.env["VOICE_PASTE_PHASE"] = "start"
        subprocess.run([str(BIN / "whisper-paste")], env=self.env, capture_output=True, text=True)
        calls = log_lines(self.env, "arecord")
        self.assertEqual(len(calls), 1, calls)
        self.assertIn(" -d 3600 ", " " + calls[0] + " ")

    def test_whisper_waits_a_tenth_of_the_take_over_the_floor(self):
        wav = silent_wav(self.tmp / "take.wav", seconds=300)  # a five-minute rant
        (self.tmp / "run" / "ormus-voice" / "recording.state").write_text(f"{DEAD_PID}:{wav}")
        self.env.update(VOICE_PASTE_PHASE="stop", STUB_CURL_OUT="Hello there.")
        r = subprocess.run([str(BIN / "whisper-paste")], env=self.env, capture_output=True, text=True)
        self.assertEqual(r.stdout.strip(), "Hello there.", r.stderr)
        calls = [c for c in log_lines(self.env, "curl") if "/inference" in c]
        self.assertEqual(len(calls), 1, calls)
        self.assertIn(" -m 36 ", " " + calls[0] + " ")  # 6 s floor + 300 s / 10
