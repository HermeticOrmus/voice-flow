"""The tree is public: it names no private host, client or person.

Every tracked file but README.md, the audio, the lock files and this test is read as text. A
private IPv4 address (10/8, 172.16/12, 192.168/16, 100.64/10) or one of WORDS (whole word, any
case) fails the test with file:line. "Sun" is too common a word to ban here; check it by hand.
Example addresses come from the documentation ranges (192.0.2.x, 198.51.100.x).
"""
from __future__ import annotations

import re
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SKIP = {"README.md", "tests/test_public.py"}
SKIP_SUFFIXES = (".wav", ".lock")
WORDS = ("sg", "mars", "moon", "corvin", "corvin-mini", "corvin-server", "tailscale", "tailnet", "raven",
         "alqvimia", "biogenesis", "laura", "voice.ormus.solutions", "ormus-claw")
OCTET = r"(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)"
PRIVATE_IP = (rf"(?<![\d.])(?:10(?:\.{OCTET}){{3}}|172\.(?:1[6-9]|2\d|3[01])(?:\.{OCTET}){{2}}"
              rf"|192\.168(?:\.{OCTET}){{2}}|100\.(?:6[4-9]|[7-9]\d|1[01]\d|12[0-7])(?:\.{OCTET}){{2}})(?!\.?\d)")
BANNED = re.compile(r"\b(?:" + "|".join(re.escape(w) for w in WORDS) + r")\b|" + PRIVATE_IP, re.IGNORECASE)

# Lines whose banned word is a value the code compares, kept as is: renaming it changes behavior.
# Each entry is (file, the whole line stripped); an edit to the line makes it a hit again.
KEPT = {
    # the board's fleet.json key for the machine whose agents the sorter can target
    ("bin/voice-ideas-sort", 'TARGET_MACHINES = ("sg",)   # the fleet docs the board can act on (its FLEET_MACHINES)'),
    # the eval finds one of the author's own dictations in his history by its words
}


def tracked() -> list[str] | None:
    r = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True)
    return r.stdout.splitlines() if r.returncode == 0 else None


def hits(files: list[str]) -> list[str]:
    out = []
    for name in files:
        path = ROOT / name
        if name in SKIP or name.endswith(SKIP_SUFFIXES) or not path.is_file():
            continue
        for n, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
            m = BANNED.search(line)
            if m and (name, line.strip()) not in KEPT:
                out.append(f"{name}:{n}: {m.group(0)}")
    return out


class Public(unittest.TestCase):
    def test_the_pattern_catches_what_it_should(self):
        for text in ("on sg", "the Raven portal", "call Laura", "10.10.90.174", "172.20.0.1", "192.168.1.180:22",
                     "100.107.202.72", "https://voice.ormus.solutions/", "Ormus-Claw", "corvin-server"):
            self.assertRegex(text, BANNED, text)
        for text in ("a msg", "192.0.2.7:8093", "198.51.100.4", "127.0.0.1:8093", "100.1.2.3", "1.10.0.0.1",
                     "ormus-voice", "Sunday", "the sun board"):
            self.assertNotRegex(text, BANNED, text)

    def test_the_tree_names_no_private_host_client_or_person(self):
        files = tracked()
        if files is None:
            self.skipTest("not a git checkout")
        found = hits(files)
        if found:
            self.fail(f"{len(found)} private names in the tree:\n" + "\n".join(found))


if __name__ == "__main__":
    unittest.main()
