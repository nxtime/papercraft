import json
import os
import subprocess
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
import contrast  # noqa: E402


class Ratio(unittest.TestCase):
    def test_black_white(self):
        self.assertAlmostEqual(contrast.ratio("#000", "#fff"), 21.0, places=2)

    def test_known_gray_boundary(self):
        # #767676 on white is the classic minimum AA gray (4.54); #777 just fails.
        self.assertGreaterEqual(contrast.ratio("#767676", "#ffffff"), 4.5)
        self.assertLess(contrast.ratio("#777777", "#ffffff"), 4.5)

    def test_symmetry(self):
        self.assertAlmostEqual(contrast.ratio("#123456", "#fedcba"), contrast.ratio("#fedcba", "#123456"))

    def test_translucent_foreground_is_flattened(self):
        # 50% black on white flattens to mid gray, far below black's 21:1.
        r = contrast.ratio("#00000080", "#ffffff")
        self.assertLess(r, 8)
        self.assertGreater(r, 3)

    def test_rejects_bad_input(self):
        with self.assertRaises(ValueError):
            contrast.ratio("blue", "#fff")
        with self.assertRaises(ValueError):
            contrast.ratio("#000", "#ffffff80")


class Tokens(unittest.TestCase):
    def run_cli(self, *args):
        return subprocess.run([sys.executable, "-I", os.path.join(ROOT, "scripts", "contrast.py"), *args],
                              capture_output=True, text=True)

    def test_shipped_themes_pass(self):
        r = self.run_cli("--tokens", os.path.join(ROOT, "templates", "tokens.light.json"),
                         os.path.join(ROOT, "templates", "tokens.dark.json"))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def test_failing_pair_is_reported(self):
        import tempfile
        doc = {"color": {"$type": "color",
                         "a": {"$value": "#777777"}, "b": {"$value": "#ffffff"}},
               "$extensions": {"dev.playbook": {"contrastPairs": [["color.a", "color.b", 4.5]]}}}
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump(doc, f)
        try:
            r = self.run_cli("--tokens", f.name)
        finally:
            os.unlink(f.name)
        self.assertEqual(r.returncode, 1)
        self.assertIn("FAIL", r.stdout)


if __name__ == "__main__":
    unittest.main()
