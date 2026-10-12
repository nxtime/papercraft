"""Shared helpers for the Admin > Sessions scripts (admin_sessions.py and its adm_*.py modules)."""
import json, sys

sys.path.insert(0, "/Volumes/Storage/Projects/craft-skills/skills/papercraft/scripts")
from pc import *  # noqa: F401,F403,E402

PROJECT = "Interview"
SP = {4: "xs", 8: "sm", 16: "md", 24: "lg", 32: "xl", 48: "xxl"}
FS = {12: "xs", 14: "sm", 16: "md", 18: "lg", 20: "xl", 24: "xxl"}
LH = {10: 14, 11: 16, 12: 16, 13: 18, 14: 22, 15: 20, 16: 24, 18: 26, 20: 28, 22: 28, 24: 32, 30: 36}

# IDE / code surfaces use literal hexes in the source; they get variables so nothing is hard-coded here.
IDE_VARS = {
    "color-ide-toolbar": "#252526", "color-ide-border": "#1a1a1a", "color-ide-code-pane": "#161616",
    "color-ide-preview": "#0d1117", "color-ide-muted": "#888888", "color-ide-faint": "#555555",
    "color-ide-chip": "#2a2a2a", "color-ide-wb": "#13131f", "color-ide-wb-bar": "#1c1c2e",
    "color-ide-wb-border": "#252540", "color-ide-card": "#161616", "color-ide-card-border": "#252525",
    "color-ide-card-text": "#dddddd", "color-ide-card-label": "#484848", "color-ide-gutter": "#5a5a5a",
    "color-ide-err-bg": "#1a0808", "color-ide-err-border": "#3a1010", "color-ide-del-bg": "#3a1a1a",
    "color-wb-white": "#e0e0e0", "color-wb-blue": "#6ab0ff", "color-wb-amber": "#d29922", "color-wb-purple": "#c792ea",
    "color-wb-orange": "#ff9f43", "color-ide-wb-tool": "#9999bb", "color-ide-wb-btn": "#252540", "color-ide-sel": "#2a2d2e",
    "color-ide-console": "#181818", "color-tok-comment": "#6a9955", "color-tok-string": "#ce9178", "color-tok-keyword": "#569cd6",
    "color-tok-type": "#4ec9b0", "color-tok-number": "#b5cea8", "color-tok-fn": "#dcdcaa",
}
STATUS = {  # label: Tag Dot tone
    "Draft": "Gray", "Open": "Blue", "In progress": "Gold", "Submitted": "Purple", "Graded": "Green", "Cancelled": "Red",
}


def sp(n):
    return f"var(--spacing-{SP[n]})" if n in SP else f"{n}px"


def fs(n):
    return f"var(--font-size-{FS[n]})" if n in FS else f"{n}px"


def rad(n):
    return {4: "var(--radius-sm)", 8: "var(--radius-md)", 12: "var(--radius-lg)"}.get(n, f"{n}px")


def T(name, t, size=14, color="var(--color-text-primary)", weight=400, lh=None, **st):
    return text(name, t, size=fs(size), color=color, weight=weight, line_height=f"{lh or LH.get(size, round(size * 1.4))}px", **st)


def ic(name, size=16, color="var(--color-text-secondary)", layer=None):
    return f'<x-paper-icon name="phosphor:{name}" size="{size}" color="{color}" layer-name="{layer or "Icon / " + name}"/>'


def inst(component, name=None, style="", **ov):
    o = json.dumps(ov, ensure_ascii=False).replace("'", "&#39;")
    n = f' layer-name="{name}"' if name else ""
    s = f' style="{style}"' if style else ""
    return f"<x-paper-instance component=\"{component}\"{n}{s} overrides='{o}'/>"


def setup():
    fid = next(i["id"] for i in call("list_resources")["items"] if i["name"] == PROJECT)
    ps = pages(fid)
    A = ps["Admin"]
    call("set_variables", {"fileId": fid, "variables": IDE_VARS})
    call("import_components", {"fileId": fid, "fromPageId": ps["Design System"].id, "pageId": A.id})
    return fid, A


def reset(A, name):
    ids = [c["id"] for c in A.c("get_children", nodeId="root")["children"] if c["name"] == name]
    if ids:
        A.c("delete_nodes", nodeIds=ids)


def section(A, name, title, per_row=3):
    reset(A, name)
    w = per_row * 1440 + (per_row - 1) * 32 + 64
    sid = A.artboard(name, display="flex", flex_direction="row", flex_wrap="wrap", gap="32px", padding="32px",
                     width=f"{w}px", height="fit-content", background_color="transparent", align_content="flex-start")
    A.add(sid, T("Section Title", title, 24, weight=600, width="100%"))
    return sid


def add_screen(A, sid, name, route, body, **frame):
    """body = inner HTML of the 1440x900 screen frame."""
    st = dict(display="flex", width="1440px", height="900px", background_color="var(--color-bg-layout)", overflow="hidden")
    st.update(frame)
    html = col(f"Screen Slot / {name}", T("Route", route, 12, "var(--color-text-secondary)") + box(f"Screen / {name}", body, **st),
               gap="8px", width="1440px")
    r = A.c("write_html", targetNodeId=sid, mode="insert-children", html=html)
    for k in ("warnings", "ignoredStyles"):
        if r.get(k):
            print(f"  [{name}] {k}:", sorted(set(map(str, r[k])))[:8])
    return r["createdNodeIds"][0]


USER = dict(**{"User Name": "Elena Vasquez", "User Email": "elena@netcracker.com", "User Initials": "EV"})


def sidebar(active="Sessions"):
    return inst(f"Admin Sidebar / {active} Active", "Sidebar", **USER)


def shell(title, subtitle, content, actions="", wide=True):
    """Admin shell screen body: sidebar + main (header, content). `actions` are absolute-positioned header buttons."""
    main = col("Main", inst("Admin Shell Header", "Header", Title=title, Subtitle=subtitle) + actions + content,
               flex_grow="1", height="900px", position="relative", overflow="hidden")
    return sidebar() + main


def page_content(inner, wide=True):
    w = 1100 if wide else 900
    return row("Content Wrap", col("Content", inner, width=f"{w}px", padding="24px", gap="16px"), justify_content="center",
               align_items="flex-start", flex_grow="1", width="1192px")


def header_action(label_component, label, width=108, right=72):
    return f'<div layer-name="Header Actions" style="position:absolute;left:{1192-right-width}px;top:34px;display:flex;gap:12px">' + \
        inst(label_component, label.replace("_", " "), Label=label) + "</div>"


def card(name, inner, **st):
    base = dict(display="flex", flex_direction="column", background_color="var(--color-bg-elevated)",
                border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)", overflow="hidden")
    base.update(st)
    return box(name, inner, **base)
