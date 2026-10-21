import os
import re
import subprocess
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SEARCH = os.path.join(ROOT, "scripts", "search.py")


def run(*args):
    return subprocess.run([sys.executable, "-I", SEARCH, *args], capture_output=True, text=True)


def all_rule_ids():
    out = run("--list").stdout
    return [line.split()[0] for line in out.splitlines()]


class Search(unittest.TestCase):
    def test_id_lookup(self):
        r = run("--id", "A11Y-10")
        self.assertEqual(r.returncode, 0)
        self.assertIn("Visible, unobscured focus", r.stdout)

    def test_prefix_lookup(self):
        r = run("--id", "MOT")
        self.assertIn("MOT-07", r.stdout)

    def test_semantic_query_finds_rule(self):
        r = run("-n", "3", "confirmation dialog for delete undo")
        self.assertIn("INT-06", r.stdout)

    def test_unknown_id(self):
        self.assertEqual(run("--id", "NOPE-99").returncode, 1)

    def test_rule_ids_unique(self):
        ids = all_rule_ids()
        self.assertEqual(len(ids), len(set(ids)))
        self.assertGreater(len(ids), 80)


class Integrity(unittest.TestCase):
    """Every rule id cited anywhere must exist, so cross-references cannot rot."""

    def test_cross_references_resolve(self):
        known = set(all_rule_ids())
        pat = re.compile(r"\b(INT|PSY|VIS|MOT|A11Y|FBK|SYS|BAD|CSS|PAT)-(\d+)\b")
        missing = []
        for base, _, names in os.walk(ROOT):
            if ".git" in base.split(os.sep):
                continue
            for n in names:
                if not n.endswith((".md", ".json", ".txt")):
                    continue
                p = os.path.join(base, n)
                with open(p, encoding="utf-8") as f:
                    text = f.read()
                for m in pat.finditer(text):
                    rid = m.group(0)
                    if rid not in known:
                        missing.append((os.path.relpath(p, ROOT), rid))
        self.assertEqual(missing, [])


if __name__ == "__main__":
    unittest.main()
