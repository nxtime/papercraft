"""Create the `Interview` project: pages, color variables and design tokens.

The values are the single source of truth from the interview repo's packages/ui/src/tokens.css
(names kept identical, without the leading `--`). Run once; the other scripts in this folder build
the design system page and the app screens on top of it.
"""
import os, re, sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "..", "craft-skills", "skills", "papercraft", "scripts"))
from pc import *  # noqa: E402

TOKENS_CSS = os.environ.get("TOKENS_CSS", "/Volumes/Storage/Projects/interview/packages/ui/src/tokens.css")


def parse_tokens(path):
    colors, others = {}, {}
    for name, value in re.findall(r"--([\w-]+):\s*([^;]+);", open(path).read()):
        v = value.strip()
        if name.startswith("color-"):
            colors[name] = v
        else:
            others[name] = v
    return colors, others


def main():
    colors, others = parse_tokens(TOKENS_CSS)
    fid, info = open_project("Interview")
    existing = {p["name"]: p["id"] for p in info["pages"]}
    first = info["pages"][0]["id"]
    wanted = ["Design System", "Admin", "Candidate"]
    if "Page 1" in existing and "Design System" not in existing:
        call("rename_pages", {"fileId": fid, "updates": [{"pageId": first, "name": "Design System"}]})
    have = {p["name"] for p in call("get_basic_info", {"fileId": fid})["pages"]}
    for w in wanted:
        if w not in have:
            pid = call("create_page", {"fileId": fid, "name": w})["pageId"]
    call("set_variables", {"fileId": fid, "variables": colors})
    call("set_tokens", {"fileId": fid, "tokens": others})
    print("project", fid, "colors", len(colors), "tokens", len(others))


if __name__ == "__main__":
    main()
