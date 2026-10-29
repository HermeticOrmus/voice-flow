"""Where speech and the LLM steps go: this machine unless voice-flow.conf names the hosts.

Every part reads the same keys, the environment first: VOICE_PASTE_REMOTE_HOSTS (whisper servers, in
order), OLLAMA_HOSTS, VOICE_HOST_NAMES ("host=name;host=name", the names a person reads).
"""
from __future__ import annotations

import importlib.machinery
import importlib.util
import os
import shutil
import unittest
from pathlib import Path

from stubs import tmpdir

BIN = Path(__file__).resolve().parent.parent / "bin"
KEYS = ("VOICE_PASTE_REMOTE_HOSTS", "OLLAMA_HOSTS", "VOICE_HOST_NAMES", "VOICE_IDEAS_HOSTS", "OLLAMA_HOST")


def load(name: str):
    loader = importlib.machinery.SourceFileLoader(name.replace("-", "_"), str(BIN / name))
    mod = importlib.util.module_from_spec(importlib.util.spec_from_loader(loader.name, loader))
    loader.exec_module(mod)
    return mod


class Hosts(unittest.TestCase):
    def setUp(self):
        self.tmp = tmpdir()
        self.saved = {k: os.environ.pop(k) for k in KEYS if k in os.environ}
        self.saved_home = os.environ.get("XDG_CONFIG_HOME")
        os.environ["XDG_CONFIG_HOME"] = str(self.tmp / "config")
        self.dir = self.tmp / "config" / "ormus-voice"
        self.dir.mkdir(parents=True)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)
        os.environ.update(self.saved)
        if self.saved_home is None:
            os.environ.pop("XDG_CONFIG_HOME", None)
        else:
            os.environ["XDG_CONFIG_HOME"] = self.saved_home

    def conf(self, text: str) -> None:
        (self.dir / "voice-flow.conf").write_text(text)

    def test_with_no_conf_everything_stays_on_this_machine(self):
        self.assertEqual(load("voice-ideas").whisper_hosts(), ["127.0.0.1:8093"])
        self.assertEqual(load("voice-ideas-sort").ollama_hosts(), ["http://127.0.0.1:11434"])
        self.assertEqual(load("voice-flow-web").hosts(), {"127.0.0.1:8093": "this machine"})

    def test_the_conf_names_the_hosts_and_their_names(self):
        self.conf('VOICE_PASTE_REMOTE_HOSTS="127.0.0.1:8093 192.0.2.7:8093"\n'
                  'OLLAMA_HOSTS="http://192.0.2.7:11434"\n'
                  'VOICE_HOST_NAMES="192.0.2.7:8093=the server;127.0.0.1:8093=desk"\n')
        self.assertEqual(load("voice-ideas").whisper_hosts(), ["127.0.0.1:8093", "192.0.2.7:8093"])
        self.assertEqual(load("voice-ideas-sort").ollama_hosts(), ["http://192.0.2.7:11434"])
        self.assertEqual(load("voice-flow-web").hosts(),
                         {"127.0.0.1:8093": "desk", "192.0.2.7:8093": "the server"})

    def test_an_unnamed_host_reads_as_its_address(self):
        self.conf('VOICE_PASTE_REMOTE_HOSTS="198.51.100.4:8093"\n')
        self.assertEqual(load("voice-flow-web").hosts(), {"198.51.100.4:8093": "198.51.100.4:8093"})

    def test_the_listener_may_keep_its_own_list(self):
        self.conf('VOICE_PASTE_REMOTE_HOSTS="192.0.2.7:8093"\nVOICE_IDEAS_HOSTS="198.51.100.4:8093"\n')
        self.assertEqual(load("voice-ideas").whisper_hosts(), ["198.51.100.4:8093"])

    def test_the_environment_wins(self):
        self.conf('OLLAMA_HOSTS="http://192.0.2.7:11434"\n')
        os.environ["OLLAMA_HOSTS"] = "http://198.51.100.3:11434"
        self.assertEqual(load("voice-ideas-sort").ollama_hosts(), ["http://198.51.100.3:11434"])
        os.environ["OLLAMA_HOST"] = "http://198.51.100.9:11434"   # one host named outright beats a list
        self.assertEqual(load("voice-ideas-sort").ollama_hosts(), ["http://198.51.100.9:11434"])


if __name__ == "__main__":
    unittest.main()
