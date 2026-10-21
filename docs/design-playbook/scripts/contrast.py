#!/usr/bin/env python3
"""WCAG 2.x contrast checker. Standard library only.

  contrast.py FG BG                     ratio and AA/AAA verdicts for two hex colors
  contrast.py --tokens FILE [FILE...]   check every contrastPairs entry in DTCG token files

A DTCG token file declares the pairs it promises to keep, at the root:

  "$extensions": {"dev.playbook": {"contrastPairs": [
      ["color.text.default", "color.surface.default", 4.5, "body text"]
  ]}}

Colors are tokens whose $value is a hex string or an object with a "hex" key.
Aliases ("{color.x}") are resolved. Exit code 1 if any pair fails.

WCAG 2 ratios are compared unrounded: 4.499 fails 4.5. APCA is not implemented
here; use the apca-w3 package when a product opts into it.
"""
import json
import re
import sys

HEX = re.compile(r"^#?([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$")


def parse_hex(value):
    m = HEX.match(value.strip())
    if not m:
        raise ValueError(f"not a hex color: {value!r}")
    h = m.group(1)
    if len(h) == 3:
        h = "".join(c * 2 for c in h)
    r, g, b = (int(h[i:i + 2], 16) for i in (0, 2, 4))
    a = int(h[6:8], 16) / 255 if len(h) == 8 else 1.0
    return r, g, b, a


def _lin(c):
    c /= 255
    return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4


def luminance(rgb):
    r, g, b = rgb[:3]
    return 0.2126 * _lin(r) + 0.7152 * _lin(g) + 0.0722 * _lin(b)


def composite(fg, bg):
    """Flatten a translucent foreground over an opaque background."""
    a = fg[3]
    return tuple(round(fg[i] * a + bg[i] * (1 - a)) for i in range(3)) + (1.0,)


def ratio(fg_hex, bg_hex):
    bg = parse_hex(bg_hex)
    if bg[3] < 1:
        raise ValueError("background must be opaque")
    fg = composite(parse_hex(fg_hex), bg)
    l1, l2 = luminance(fg), luminance(bg)
    hi, lo = max(l1, l2), min(l1, l2)
    return (hi + 0.05) / (lo + 0.05)


def verdicts(r):
    return {
        "AA normal (4.5)": r >= 4.5,
        "AA large / UI (3)": r >= 3.0,
        "AAA normal (7)": r >= 7.0,
        "AAA large (4.5)": r >= 4.5,
    }


def flatten(node, path=()):
    """Yield (dotted_path, token) for every object that has $value."""
    if isinstance(node, dict):
        if "$value" in node:
            yield ".".join(path), node
            return
        for k, v in node.items():
            if not k.startswith("$"):
                yield from flatten(v, path + (k,))


def resolve_hex(tokens, name, seen=()):
    if name in seen:
        raise ValueError(f"alias cycle at {name}")
    if name not in tokens:
        raise KeyError(f"unknown token {name}")
    value = tokens[name]["$value"]
    if isinstance(value, dict):
        value = value.get("hex")
        if not value:
            raise ValueError(f"{name}: color object has no hex")
    if isinstance(value, str) and value.startswith("{") and value.endswith("}"):
        return resolve_hex(tokens, value[1:-1], seen + (name,))
    return value


def check_tokens(path):
    with open(path, encoding="utf-8") as f:
        doc = json.load(f)
    tokens = dict(flatten(doc))
    pairs = (doc.get("$extensions", {}).get("dev.playbook", {}).get("contrastPairs", []))
    if not pairs:
        print(f"{path}: no contrastPairs declared")
        return 0, 0
    failed = 0
    for pair in pairs:
        fg, bg, need = pair[0], pair[1], float(pair[2])
        note = pair[3] if len(pair) > 3 else ""
        try:
            r = ratio(resolve_hex(tokens, fg), resolve_hex(tokens, bg))
        except (KeyError, ValueError) as e:
            print(f"ERROR {path}: {fg} on {bg}: {e}")
            failed += 1
            continue
        ok = r >= need
        failed += not ok
        print(f"{'PASS' if ok else 'FAIL'} {r:6.2f} (need {need:g})  {fg} on {bg}  {note}")
    return len(pairs), failed


def main(argv):
    if len(argv) >= 3 and argv[1] == "--tokens":
        total = bad = 0
        for p in argv[2:]:
            n, f = check_tokens(p)
            total += n
            bad += f
        print(f"{total - bad}/{total} pairs pass")
        return 1 if bad else 0
    if len(argv) == 3:
        r = ratio(argv[1], argv[2])
        print(f"{r:.2f}:1")
        for k, v in verdicts(r).items():
            print(f"  {'pass' if v else 'FAIL'}  {k}")
        return 0 if r >= 4.5 else 1
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
