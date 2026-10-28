"""voice-flow-web: a settings save keeps every conf key it does not own.

Run: python3 -m unittest discover -s tests -v
"""
from __future__ import annotations

import importlib.machinery
import importlib.util
import os
import shutil
import unittest

from stubs import BIN, tmpdir


def load_web(config_home: str):
    os.environ["XDG_CONFIG_HOME"] = config_home
    loader = importlib.machinery.SourceFileLoader("voice_flow_web", str(BIN / "voice-flow-web"))
    spec = importlib.util.spec_from_loader("voice_flow_web", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


class ConfWriter(unittest.TestCase):
    def setUp(self):
        self.tmp = tmpdir()
        self.conf_dir = self.tmp / "config" / "ormus-voice"
        self.conf_dir.mkdir(parents=True)
        self.conf = self.conf_dir / "voice-flow.conf"
        self.web = load_web(str(self.tmp / "config"))

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)
        os.environ.pop("XDG_CONFIG_HOME", None)

    def test_a_save_keeps_keys_it_does_not_own(self):
        self.conf.write_text(
            "# mine\nVOICE_FLOW_METHOD=auto\nVOICE_FLOW_EXPRESS=casual\nVOICE_IDEAS_PAUSE_MS=1500\n"
        )
        cfg = self.web.read_conf()
        cfg["VOICE_FLOW_REWRITE"] = "off"
        self.web.write_conf(cfg)
        text = self.conf.read_text()
        self.assertIn("VOICE_FLOW_EXPRESS=casual\n", text)
        self.assertIn("VOICE_IDEAS_PAUSE_MS=1500\n", text)
        self.assertIn("VOICE_FLOW_REWRITE=off\n", text)
        self.assertEqual(text.count("VOICE_FLOW_METHOD="), 1)

    def test_a_save_updates_a_key_in_place(self):
        self.conf.write_text("VOICE_FLOW_EXPRESS=casual\nVOICE_FLOW_PILL=1\n")
        cfg = self.web.read_conf()
        cfg["VOICE_FLOW_PILL"] = "0"
        self.web.write_conf(cfg)
        lines = self.conf.read_text().splitlines()
        self.assertEqual(lines.index("VOICE_FLOW_PILL=0"), 1)  # same place, new value

    def test_a_fresh_conf_is_written(self):
        cfg = self.web.read_conf()
        self.web.write_conf(cfg)
        self.assertIn("VOICE_FLOW_METHOD=auto\n", self.conf.read_text())


if __name__ == "__main__":
    unittest.main()
