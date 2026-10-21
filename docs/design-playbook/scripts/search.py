#!/usr/bin/env python3
"""Search the playbook. Standard library only, no index to maintain.

  search.py QUERY...            ranked sections matching the query
  search.py --id A11Y-10        print one rule by id (also accepts a prefix: A11Y)
  search.py --list              list all rule ids and titles
  search.py -n 8 QUERY...       number of results (default 5)

A "section" is a Markdown heading plus its body. Rule ids come from headings of
the form "### A11Y-10 Title". Ranking is BM25 over section text with a boost for
title/id hits, which is enough for a corpus this size.
"""
import math
import os
import re
import sys
from collections import Counter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIRS = ["principles", "platforms", "checklists", "templates", "sources", "skills"]
TOP = ["README.md", "AGENTS.md"]
HEADING = re.compile(r"^(#{1,6})\s+(.*\S)\s*$")
RULE_ID = re.compile(r"^([A-Z0-9]+-\d+)\b")
TOKEN = re.compile(r"[a-z0-9][a-z0-9\-_.]*[a-z0-9]|[a-z0-9]")


def stem(t):
    """Crude plural/gerund folding so 'transitions' matches 'transition'."""
    if len(t) > 5 and t.endswith("ing"):
        return t[:-3]
    if len(t) > 3 and t.endswith("s") and not t.endswith("ss"):
        return t[:-1]
    return t


def tokenize(text):
    return [stem(t) for t in TOKEN.findall(text.lower())]


def files():
    for name in TOP:
        p = os.path.join(ROOT, name)
        if os.path.exists(p):
            yield p
    for d in DIRS:
        for base, _, names in os.walk(os.path.join(ROOT, d)):
            for n in sorted(names):
                if n.endswith(".md"):
                    yield os.path.join(base, n)


def sections():
    out = []
    for path in files():
        rel = os.path.relpath(path, ROOT)
        in_fence = False
        cur = None
        with open(path, encoding="utf-8") as f:
            for i, line in enumerate(f, 1):
                if line.lstrip().startswith("```"):
                    in_fence = not in_fence
                m = None if in_fence else HEADING.match(line)
                if m:
                    if cur:
                        out.append(cur)
                    title = m.group(2)
                    idm = RULE_ID.match(title)
                    cur = {"path": rel, "line": i, "title": title,
                           "id": idm.group(1) if idm else None, "body": []}
                elif cur is not None:
                    cur["body"].append(line.rstrip("\n"))
            if cur:
                out.append(cur)
                cur = None
    for s in out:
        s["text"] = "\n".join(s["body"]).strip()
    return out


def rank(query, secs, k):
    q = tokenize(query)
    if not q:
        return []
    docs = [tokenize(s["title"] + " " + (s["id"] or "") + " " + s["text"]) for s in secs]
    n = len(docs)
    avg = sum(len(d) for d in docs) / max(n, 1)
    df = Counter(t for d in docs for t in set(d))
    scored = []
    for s, d in zip(secs, docs):
        tf = Counter(d)
        score = 0.0
        for t in set(q):
            if t not in tf:
                continue
            idf = math.log(1 + (n - df[t] + 0.5) / (df[t] + 0.5))
            score += idf * tf[t] * 2.2 / (tf[t] + 1.2 * (0.25 + 0.75 * len(d) / avg))
        head = set(tokenize(s["title"] + " " + (s["id"] or "")))
        score += 1.5 * len(head & set(q))
        if score > 0:
            scored.append((score, s))
    scored.sort(key=lambda x: -x[0])
    return scored[:k]


def show(s, full):
    print(f"{s['path']}:{s['line']}  {s['title']}")
    text = s["text"]
    if not full and len(text) > 420:
        text = text[:420].rsplit(" ", 1)[0] + " ..."
    for line in text.splitlines():
        print("    " + line)
    print()


def main(argv):
    args = argv[1:]
    k = 5
    if "-n" in args:
        i = args.index("-n")
        k = int(args[i + 1])
        del args[i:i + 2]
    secs = sections()
    if args[:1] == ["--list"]:
        for s in secs:
            if s["id"]:
                print(f"{s['id']:10} {s['title'][len(s['id']):].strip()}  ({s['path']}:{s['line']})")
        return 0
    if args[:1] == ["--id"] and len(args) == 2:
        want = args[1].upper()
        hits = [s for s in secs if s["id"] and (s["id"] == want or s["id"].startswith(want + "-"))]
        if not hits:
            print(f"no rule {want}", file=sys.stderr)
            return 1
        for s in hits:
            show(s, True)
        return 0
    if not args:
        print(__doc__)
        return 2
    results = rank(" ".join(args), secs, k)
    if not results:
        print("no match; widen the query, or follow sources/SOURCES.md to research", file=sys.stderr)
        return 1
    for _, s in results:
        show(s, False)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
