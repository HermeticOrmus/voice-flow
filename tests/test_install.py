"""install.sh on a fresh account: it links the scripts and finishes even when the Rust parts cannot
build (a cargo with no toolchain set, as on a fresh account with rustup)."""
from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class Install(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        stubs = self.tmp / "stubs"
        stubs.mkdir()
        cargo = stubs / "cargo"
        cargo.write_text("#!/bin/sh\necho 'error: no default toolchain' >&2\nexit 1\n")
        cargo.chmod(0o755)
        self.env = {"HOME": str(self.tmp / "home"), "PATH": f"{stubs}:/usr/bin:/bin",
                    "XDG_RUNTIME_DIR": str(self.tmp / "run")}

    def test_a_panel_that_does_not_build_is_a_warning_not_the_end(self):
        r = subprocess.run(["bash", str(ROOT / "install.sh")], env=self.env, capture_output=True, text=True,
                           timeout=60)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("voice-flow-panel did not build", r.stdout)
        self.assertIn("Bind a key to voice-flow", r.stdout)
        self.assertTrue((self.tmp / "home/.local/bin/voice-flow").is_symlink())
        self.assertTrue((self.tmp / "home/.local/share/applications/voice-flow-dashboard.desktop").exists())


if __name__ == "__main__":
    unittest.main()
