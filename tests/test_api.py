"""voice-flow-api: the Super+H pipeline over HTTP.

The real server, the real whisper-paste --file, voice-substitute and voice-rewrite-ollama run;
only the two network engines are fakes (a whisper server and an Ollama) on loopback ports.

Run: python3 -m unittest discover -s tests -v
"""
from __future__ import annotations

import http.server
import json
import math
import os
import shutil
import socket
import struct
import subprocess
import sys
import threading
import time
import unittest
import urllib.error
import urllib.request
import wave
from pathlib import Path

from stubs import BIN, tmpdir


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def tone(path: Path, seconds: float = 0.6) -> Path:
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(16000)
        w.writeframes(b"".join(struct.pack("<h", int(8000 * math.sin(i / 8))) for i in range(int(16000 * seconds))))
    return path


class Fake(http.server.ThreadingHTTPServer):
    """A whisper server (POST /inference) and an Ollama (GET /api/version, POST /api/generate)."""

    def __init__(self, whisper_reply: dict, ollama_reply: str | None = "Cleaned text."):
        self.whisper_reply, self.ollama_reply = whisper_reply, ollama_reply
        self.generate_calls = 0
        self.delay = 0.0
        outer = self

        class H(http.server.BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def reply(self, body: bytes, code=200):
                self.send_response(code)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def do_GET(self):
                self.reply(b'{"version":"0"}')

            def do_POST(self):
                n = int(self.headers.get("Content-Length") or 0)
                self.rfile.read(n)
                if self.path.startswith("/inference"):
                    time.sleep(outer.delay)
                    return self.reply(json.dumps(outer.whisper_reply).encode())
                outer.generate_calls += 1
                if outer.ollama_reply is None:
                    return self.reply(b"", 500)
                self.reply(json.dumps({"response": outer.ollama_reply}).encode())

        super().__init__(("127.0.0.1", 0), H)
        threading.Thread(target=self.serve_forever, daemon=True).start()

    @property
    def addr(self) -> str:
        return f"127.0.0.1:{self.server_address[1]}"


SPEECH = {"text": " Hello there frend.", "segments": [{"text": " Hello there frend.", "avg_logprob": -0.4}]}
JUNK = {"text": " Sous-titres par la communaute.", "segments": [{"text": " Sous-titres par la communaute.", "avg_logprob": -5.9}]}


class Api(unittest.TestCase):
    def setUp(self):
        self.tmp = tmpdir()
        self.fakes: list[Fake] = []
        self.proc = None

    def tearDown(self):
        if self.proc:
            self.proc.terminate()
            self.proc.wait(5)
        for f in self.fakes:
            f.shutdown()
        shutil.rmtree(self.tmp, ignore_errors=True)

    def start(self, whisper=SPEECH, ollama="Cleaned text.", engine_up=True, **extra):
        fake = Fake(whisper, ollama)
        self.fakes.append(fake)
        self.fake = fake
        port = free_port()
        self.port = port
        home = self.tmp / "home"
        (self.tmp / "config" / "ormus-voice").mkdir(parents=True)
        (self.tmp / "config" / "ormus-voice" / "substitutions.tsv").write_text("frend\tfriend\n")
        home.mkdir()
        env = {
            "PATH": f"{BIN}:/usr/bin:/bin", "HOME": str(home), "LANG": "C.UTF-8",
            "XDG_CONFIG_HOME": str(self.tmp / "config"), "XDG_DATA_HOME": str(self.tmp / "data"),
            "XDG_RUNTIME_DIR": str(self.tmp / "run"),
            "VOICE_PASTE_REMOTE_HOSTS": fake.addr if engine_up else f"127.0.0.1:{free_port()}",
            "OLLAMA_HOST": f"http://{fake.addr}", "VOICE_FLOW_API_PORT": str(port),
            "VOICE_FLOW_CONF": str(self.tmp / "none.conf"),
        }
        env.update(extra)
        self.env = env
        self.proc = subprocess.Popen([sys.executable, str(BIN / "voice-flow-api")], env=env,
                                     stderr=subprocess.DEVNULL)
        for _ in range(50):
            try:
                socket.create_connection(("127.0.0.1", port), 0.2).close()
                return
            except OSError:
                time.sleep(0.1)
        self.fail("server did not start")

    def call(self, method, path, body=None, ctype=None, headers=None):
        req = urllib.request.Request(f"http://127.0.0.1:{self.port}{path}", data=body, method=method)
        if ctype:
            req.add_header("Content-Type", ctype)
        for k, v in (headers or {}).items():
            req.add_header(k, v)
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return r.status, json.loads(r.read())
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read())

    def wav(self) -> bytes:
        return tone(self.tmp / "t.wav").read_bytes()

    def history(self) -> list[dict]:
        p = self.tmp / "data" / "ormus-voice" / "history.jsonl"
        return [json.loads(l) for l in p.read_text().splitlines()] if p.exists() else []

    def test_health_names_the_first_reachable_engine(self):
        self.start()
        code, body = self.call("GET", "/v1/health")
        self.assertEqual((code, body), (200, {"status": "ok", "engine": self.fake.addr}))

    def test_health_without_an_engine_is_no_engine(self):
        self.start(engine_up=False)
        code, body = self.call("GET", "/v1/health")
        self.assertEqual((code, body["error"]), (503, "no_engine"))

    def test_dictate_runs_substitution_then_rewrite(self):
        self.start()
        code, body = self.call("POST", "/v1/dictate", self.wav(), "audio/wav")
        self.assertEqual(code, 200, body)
        self.assertEqual(body["text"], "Cleaned text.")
        self.assertEqual(body["raw"], "Hello there frend.")
        self.assertEqual((body["style"], body["engine"]), ("prompt", self.fake.addr))
        self.assertEqual(set(body["ms"]), {"transcribe", "rewrite"})
        self.assertNotIn("dropped", body)
        self.assertEqual(self.fake.generate_calls, 1)

    def test_substitution_reaches_the_verbatim_text(self):
        self.start()
        code, body = self.call("POST", "/v1/dictate?style=verbatim", self.wav(), "audio/wav")
        self.assertEqual((code, body["text"], body["style"]), (200, "Hello there friend.", "verbatim"))
        self.assertEqual(self.fake.generate_calls, 0)

    def test_the_conf_style_is_the_default(self):
        self.start(VOICE_FLOW_EXPRESS="verbatim")
        _, body = self.call("POST", "/v1/dictate", self.wav(), "audio/wav")
        self.assertEqual((body["style"], self.fake.generate_calls), ("verbatim", 0))

    def test_junk_take_is_dropped_with_200(self):
        self.start(whisper=JUNK)
        code, body = self.call("POST", "/v1/dictate", self.wav(), "audio/wav")
        self.assertEqual((code, body["text"], body["dropped"]), (200, "", "junk"))

    def test_failed_rewrite_keeps_the_words(self):
        self.start(ollama=None)
        code, body = self.call("POST", "/v1/dictate", self.wav(), "audio/wav")
        self.assertEqual(code, 200, body)
        self.assertEqual((body["text"], body["rewrite"]), ("Hello there friend.", "failed"))

    def test_ogg_and_mp4_are_decoded(self):
        self.start()
        src = tone(self.tmp / "s.wav")
        for ext, ctype, codec in (("ogg", "audio/ogg", "libopus"), ("m4a", "audio/mp4", "aac")):
            out = self.tmp / f"s.{ext}"
            subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", str(src), "-c:a", codec, str(out)], check=True)
            code, body = self.call("POST", "/v1/dictate?style=verbatim", out.read_bytes(), ctype)
            self.assertEqual((code, body.get("raw")), (200, "Hello there frend."), ctype)

    def test_errors_have_the_json_shape(self):
        self.start(VOICE_FLOW_API_MAX_BYTES="1000")
        self.assertEqual(self.call("POST", "/v1/dictate", b"not audio at all", "audio/wav")[1]["error"], "bad_audio")
        self.assertEqual(self.call("POST", "/v1/dictate", b"x" * 2000, "audio/wav")[0:2][1]["error"], "too_large")
        self.assertEqual(self.call("POST", "/v1/dictate", b"x", "text/plain")[0], 415)
        code, body = self.call("POST", "/v1/dictate?style=loud", b"x", "audio/wav")
        self.assertEqual((code, body["error"]), (400, "bad_request"))

    def test_no_engine_on_dictate(self):
        self.start(engine_up=False)
        code, body = self.call("POST", "/v1/dictate", self.wav(), "audio/wav")
        self.assertEqual((code, body["error"]), (503, "no_engine"))

    def test_history_is_counts_only_with_method_api(self):
        self.start()
        self.call("POST", "/v1/dictate", self.wav(), "audio/wav")
        (line,) = self.history()
        self.assertEqual((line["method"], line["words"], line["chars"]), ("api", 2, 13))
        self.assertNotIn("text", line)
        self.assertNotIn("Cleaned", json.dumps(line))

    def test_history_carries_text_only_when_asked(self):
        self.start(VOICE_FLOW_LOG_TEXT="1")
        self.call("POST", "/v1/dictate", self.wav(), "audio/wav")
        self.assertEqual(self.history()[0]["text"], "Cleaned text.")

    def test_super_h_files_are_untouched(self):
        self.start()
        self.call("POST", "/v1/dictate", self.wav(), "audio/wav")
        ov = self.tmp / "data" / "ormus-voice"
        self.assertFalse((ov / "last-paste.txt").exists())
        self.assertFalse((ov / "backend.state").exists())
        self.assertNotIn(b"Cleaned", b"".join(p.read_bytes() for p in ov.rglob("*") if p.is_file()))

    def test_a_playlist_is_not_followed(self):
        self.start()
        evil = b"#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXTINF:1,\nfile:///etc/hostname\n"
        code, body = self.call("POST", "/v1/dictate", evil, "audio/wav")
        self.assertEqual((code, body["error"]), (400, "bad_audio"))

    def test_busy_when_the_parallel_limit_is_reached(self):
        self.start(VOICE_FLOW_API_PARALLEL="1")
        self.fake.delay = 1.5
        first = threading.Thread(target=lambda: self.call("POST", "/v1/dictate?style=verbatim", self.wav(), "audio/wav"))
        first.start()
        time.sleep(0.6)
        code, body = self.call("POST", "/v1/dictate?style=verbatim", self.wav(), "audio/wav")
        first.join()
        self.assertEqual((code, body["error"]), (503, "busy"))

    def test_a_bad_max_bytes_stops_the_server_at_start(self):
        env = dict(os.environ, VOICE_FLOW_API_MAX_BYTES="lots", VOICE_FLOW_API_PORT=str(free_port()),
                   VOICE_FLOW_CONF=str(self.tmp / "none.conf"))
        r = subprocess.run([sys.executable, str(BIN / "voice-flow-api")], env=env, capture_output=True, timeout=10)
        self.assertEqual(r.returncode, 2)

    def test_super_h_runtime_dir_is_untouched(self):
        self.start()
        self.call("POST", "/v1/dictate", self.wav(), "audio/wav")
        self.assertFalse((self.tmp / "run").exists())


if __name__ == "__main__":
    unittest.main()
