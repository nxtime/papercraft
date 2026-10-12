"""Shared helpers for the Design System page scripts (ds_foundations.py, ds_components.py, design_system.py)."""
import sys

sys.path.insert(0, "/Volumes/Storage/Projects/craft-skills/skills/papercraft/scripts")
from pc import *  # noqa: F401,F403,E402

PROJECT = "Interview"

# Extra color variables (no token in tokens.css: values read from the component code / AntD default theme).
EXTRA_VARS = {
    # app sidebar (globals.css .app-sidebar*)
    "color-sidebar-bg": "#11161d",
    "color-sidebar-border": "#ffffff12",
    "color-sidebar-text": "#ffffffa6",
    "color-sidebar-text-muted": "#ffffff73",
    "color-sidebar-text-faint": "#ffffff59",
    "color-sidebar-hover": "#ffffff0f",
    "color-sidebar-badge": "#ffffff40",
    "color-sidebar-tag-bg": "#1677ff40",
    # Tag tones (Tag.tsx)
    "color-tag-blue-fg": "#0958d9", "color-tag-blue-bg": "#e6f4ff", "color-tag-blue-bd": "#91caff",
    "color-tag-green-fg": "#389e0d", "color-tag-green-bg": "#f6ffed", "color-tag-green-bd": "#b7eb8f",
    "color-tag-gold-fg": "#ad6800", "color-tag-gold-bg": "#fffbe6", "color-tag-gold-bd": "#ffe58f",
    "color-tag-red-fg": "#cf1322", "color-tag-red-bg": "#fff1f0", "color-tag-red-bd": "#ffccc7",
    "color-tag-purple-fg": "#531dab", "color-tag-purple-bg": "#f9f0ff", "color-tag-purple-bd": "#d3adf7",
    "color-tag-cyan-fg": "#08979c", "color-tag-cyan-bg": "#e6fffb", "color-tag-cyan-bd": "#87e8de",
    # misc component colors
    "color-button-dark": "#1f1f1f",
    "color-button-primary-hover": "#4096ff",
    "color-card-hover-border": "#bfdbff",
    "color-star": "#faad14",
    "color-score-warn": "#faad14",
    "color-score-poor": "#ff4d4f",
    "color-text-placeholder": "#00000040",
    "color-tooltip-bg": "#000000d9",
    "color-switch-off": "#00000040",
    "color-focus-ring": "#1677ff1a",
    # Avatar palette (Avatar.tsx PALETTE; blue/amber/green reuse brand/warning/success)
    "color-avatar-purple": "#722ed1", "color-avatar-teal": "#13a8a8", "color-avatar-pink": "#eb2f96",
    "color-avatar-orange": "#fa541c", "color-avatar-indigo": "#2f54eb",
}

SHADOW_BTN = "0 1px 2px rgba(5,90,235,.25)"
SHADOW_FOCUS = "0 0 0 2px var(--color-focus-ring)"


def camel(k):
    k = k.replace("_", "-")
    p = k.split("-")
    return p[0] + "".join(x.title() for x in p[1:])


def setup():
    """Returns (file id, Design System Page) and defines the extra variables."""
    fid = next(i["id"] for i in call("list_resources")["items"] if i["name"] == PROJECT)
    P = pages(fid)["Design System"]
    call("set_variables", {"fileId": fid, "variables": EXTRA_VARS})
    return fid, P


def clear(P):
    ids = [c["id"] for c in P.c("get_children", nodeId="root")["children"]]
    if ids:
        P.c("delete_nodes", nodeIds=ids)


def lines(h):  # shorthand for line-height in px
    return f"{h}px"


def T(name, t, size=14, color="var(--color-text-primary)", weight=400, lh=None, **st):
    """Text layer with px font size and a line-height (default ~1.5 rounded to 2px)."""
    lh = lh or {10.5: 14, 11: 16, 12: 16, 13: 18, 14: 22, 15: 20, 16: 24, 18: 26, 20: 28, 24: 32, 30: 36}.get(size, round(size * 1.4))
    return text(name, t, size=f"{size}px", color=color, weight=weight, line_height=f"{lh}px", **st)


def ic(name, size=16, color="var(--color-text-secondary)", layer=None):
    return f'<x-paper-icon name="phosphor:{name}" size="{size}" color="{color}" layer-name="{layer or "Icon / " + name}"/>'


def place_components(P, specs, x0, y0, max_w, groups):
    """Create main components from [(name, html)] (html = ONE top-level element each) and lay them out in rows.

    groups: ordered {group title: [component names]}; each group gets a title text and wrapped rows.
    """
    ids = {}
    for i in range(0, len(specs), 12):
        chunk = specs[i : i + 12]
        r = P.c("write_html", targetNodeId="root", mode="insert-children", html="".join(h for _, h in chunk))
        for w in r.get("warnings", []):
            print("  warn:", w)
        if r.get("ignoredStyles"):
            print("  ignored:", sorted(set(r["ignoredStyles"]))[:6])
        for (name, _), nid in zip(chunk, r["createdNodeIds"]):
            P.c("create_component", nodeId=nid, name=name)
            ids[name] = nid
    sizes = {c["name"]: (c["width"], c["height"]) for c in P.c("get_components")["components"]}
    y = y0
    gap = 24
    for title, names in groups.items():
        t = P.add("root", T(f"Library / {title}", title, 20, weight=600, position="absolute", left=f"{x0}px", top=f"{y}px"))
        y += 40
        x, rowh = x0, 0
        for n in names:
            w, h = sizes[n]
            if x > x0 and x + w > x0 + max_w:
                x, y, rowh = x0, y + rowh + gap, 0
            P.c("update_styles", updates=[{"nodeIds": [ids[n]], "styles": {"left": x, "top": y}}])
            x += w + gap
            rowh = max(rowh, h)
        y += rowh + 48
    return ids, y
