"""voice-flow-pill: its colors come from the desktop theme, with the built-in palette as the fallback.

The pill prints its resolved roles with --print-theme and exits before GTK loads.
Run: python3 -m unittest discover -s tests -v
"""
from __future__ import annotations

import shutil
import subprocess
import unittest

from stubs import BIN, make_env, tmpdir

ORMUS_SUN = '''# Ormus Aurum
mode = "dark"
accent    = "#FFD700"
background         = "#090A16"
lighter_background = "#141418"
foreground        = "#F5E6C8"
dark_foreground   = "#8A7434"
light_foreground  = "#ECDBA6"
red     = "#CC6633"
green   = "#5A9A7A"
blue    = "#6488B4"
bright_green   = "#7ABB9A"
bright_blue    = "#8FACD6"
hyprland_active_border = "rgba(FFD700ff) rgba(F5E6C8dd) 45deg"
'''


def roles(env: dict) -> dict:
    r = subprocess.run([str(BIN / "voice-flow-pill"), "--print-theme"], env=env, capture_output=True, text=True, timeout=20)
    assert r.returncode == 0, r.stderr[-400:]
    return dict(line.split("=", 1) for line in r.stdout.splitlines() if "=" in line)


class PillTheme(unittest.TestCase):
    def setUp(self):
        self.tmp = tmpdir()
        self.env = make_env(self.tmp, XDG_STATE_HOME=str(self.tmp / "state"))
        self.theme_dir = self.tmp / "state" / "omarchy" / "current" / "theme"

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_the_desktop_theme_is_read(self):
        self.theme_dir.mkdir(parents=True)
        (self.theme_dir / "colors.toml").write_text(ORMUS_SUN)
        t = roles(self.env)
        self.assertEqual(t["ground"], "#090A16")
        self.assertEqual(t["accent"], "#FFD700")
        self.assertEqual(t["ok"], "#7ABB9A")   # bright_green over green
        self.assertEqual(t["warn"], "#CC6633")
        self.assertEqual(t["agent"], "#8FACD6")

    def test_no_theme_file_means_the_built_in_palette(self):
        t = roles(self.env)
        self.assertEqual(t["ground"], "#0B0B0D")
        self.assertEqual(t["accent"], "#E0B258")

    def test_a_malformed_line_is_skipped(self):
        self.theme_dir.mkdir(parents=True)
        (self.theme_dir / "colors.toml").write_text('accent = "gold"\nbackground = "#090A16"\n')
        t = roles(self.env)
        self.assertEqual(t["ground"], "#090A16")
        self.assertEqual(t["accent"], "#E0B258")


if __name__ == "__main__":
    unittest.main()
