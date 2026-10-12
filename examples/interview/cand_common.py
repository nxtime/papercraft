"""Shared helpers for the Candidate page scripts (candidate.py runs cand_components, cand_access, cand_session, cand_challenge)."""
import json, sys

sys.path.insert(0, "/Volumes/Storage/Projects/papercraft/examples/interview")
from ds_common import *  # noqa: F401,F403  (pc helpers, T, ic, PROJECT)

OUT = "/private/tmp/claude-501/-Volumes-Storage-Projects-photocraft/11f4040a-b4f4-4b9e-965a-bde08b400450/scratchpad/"
SIDEBAR_W, MAIN_W = 248, 1192

# Colors that exist in the candidate app code but have no token in tokens.css (IDE dark theme, login split, system check).
CAND_VARS = {
    # IDE dark theme (apps/web/src/components/ide/*)
    "color-ide-panel": "#252526", "color-ide-deep": "#1a1a1a", "color-ide-console": "#181818", "color-ide-input": "#161616",
    "color-ide-line": "#2a2a2a", "color-ide-active": "#2a2d2e", "color-ide-rail": "#1f1f1f",
    "color-ide-text": "#cccccc", "color-ide-text-muted": "#888888", "color-ide-text-faint": "#666666", "color-ide-text-dim": "#bbbbbb",
    "color-ide-pass": "#3fb950", "color-ide-fail": "#f85149", "color-ide-warn": "#d29922",
    "color-ide-pass-bg": "#1a3a1a", "color-ide-warn-bg": "#2a2a14", "color-ide-fail-bg": "#3a1a1a", "color-ide-teal-bg": "#1a3540", "color-ide-blue-bg": "#1a2a3a",
    "color-ide-submit-bg": "#1c2a40", "color-ide-submit-fg": "#5a80a8",
    "color-syntax-comment": "#6a9955", "color-syntax-string": "#ce9178", "color-syntax-keyword": "#569cd6", "color-syntax-type": "#4ec9b0",
    "color-syntax-number": "#b5cea8", "color-syntax-fn": "#dcdcaa", "color-syntax-gutter": "#5a5a5a",
    "color-wb-bg": "#13131f", "color-wb-toolbar": "#1c1c2e", "color-wb-border": "#252540", "color-wb-button": "#252540", "color-wb-text": "#9999bb",
    "color-diagram-bg": "#0d1117", "color-ide-css": "#c586c0", "color-ide-pass-border": "#3fb95035", "color-ide-chip": "#252525", "color-ide-sep": "#333333",
    "color-wb-white": "#e0e0e0", "color-wb-blue": "#6ab0ff", "color-wb-purple": "#c792ea", "color-wb-orange": "#ff9f43", "color-ide-fail-border": "#5a2020",
    # login split (globals.css .login-*)
    "color-login-bg": "#0f1117", "color-login-live-bg": "#16a34a2e", "color-login-live-fg": "#4ade80",
    # system check page (globals.css .syscheck-*)
    "color-syscheck-bg": "#f0f0f0", "color-syscheck-ready": "#16a34a", "color-syscheck-ready-bg": "#16a34a1a", "color-syscheck-fail": "#dc2626",
    "color-syscheck-camera": "#111111", "color-syscheck-mic": "#22c55e",
}


def fg(c):
    return f"var(--color-{c})"


def setup_cand():
    fid = next(i["id"] for i in call("list_resources")["items"] if i["name"] == PROJECT)
    ps = pages(fid)
    call("set_variables", {"fileId": fid, "variables": CAND_VARS})
    return fid, ps["Candidate"], ps["Design System"]


def I(comp, name=None, style=None, **ov):
    """Instance html: I("Button / Primary", Label="Save", style="width:100%")."""
    a = f' layer-name="{name}"' if name else ""
    s = f' style="{style}"' if style else ""
    o = json.dumps(ov, ensure_ascii=False).replace("'", "&#39;")
    return f"<x-paper-instance component=\"{comp}\"{a}{s} overrides='{o}'/>"


def write(P, parent, html):
    r = P.c("write_html", targetNodeId=parent, mode="insert-children", html=html)
    for w in r.get("warnings", []):
        print("  warn:", w)
    if r.get("ignoredStyles"):
        print("  ignored:", sorted(set(map(str, r["ignoredStyles"])))[:8])
    return r.get("createdNodeIds", [])


def top_children(P):
    return P.c("get_children", nodeId="root")["children"]


def delete_named(P, pred):
    ids = [c["id"] for c in top_children(P) if pred(c["name"])]
    if ids:
        P.c("delete_nodes", nodeIds=ids)
    return len(ids)


def place(P, node, x, y):
    P.c("update_styles", updates=[{"nodeIds": [node], "styles": {"left": x, "top": y}}])


def make_section(P, name, title, screens, x, y):
    """screens: [(caption, route, screen html)] -> one `Section / ...` frame holding captioned screens in a row."""
    blocks = "".join(col(f"Block / {c}", T("Caption", f"{c}  ·  {r}", 14, fg("text-secondary"), 600) + html, gap="8px") for c, r, html in screens)
    sec = col(name, T("Section Title", title, 24, weight=700) + row("Screens", blocks, gap="var(--spacing-xl)", align_items="flex-start"),
              gap="var(--spacing-xl)", padding="var(--spacing-xl)", background="transparent")
    nid = write(P, "root", sec)[0]
    place(P, nid, x, y)
    return nid


def screen(name, body, w=1440, h=900, **st):
    return row(name, body, width=f"{w}px", height=f"{h}px", background="var(--color-bg-layout)", overflow="hidden", align_items="stretch", **st)


def shell(name, active, title, subtitle, content, sidebar_name=None):
    """Candidate shell screen: sidebar instance + header instance + padded content."""
    sb = I(f"Candidate Sidebar / {active} Active", "Sidebar", style="height:900px") if active else I("Candidate Sidebar", "Sidebar")
    main = col("Main", I("Candidate Shell Header", "Header", style="height:134px", Title=title, Subtitle=subtitle) +
               col("Content", content, padding="var(--page-padding)", gap="var(--spacing-lg)", flex_grow="1", overflow="hidden"),
               width=f"{MAIN_W}px", background="var(--color-bg-layout)")
    return screen(name, sb + main)


def shot(P, node, name, scale=1):
    P.screenshot(OUT + name + ".png", node, scale)
    return OUT + name + ".png"
