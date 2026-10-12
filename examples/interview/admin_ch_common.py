"""Helpers shared by admin_challenges.py and admin_bank.py (Admin page, Interview project)."""
import json
import sys

sys.path.insert(0, "/Volumes/Storage/Projects/papercraft/examples/interview")
from ds_common import *  # noqa: F401,F403,E402

DS_ID = None


def fg(c):
    return f"var(--color-{c})"


def I(component, name=None, style=None, **ov):
    """Instance html with overrides and optional root style (dict or str)."""
    o = json.dumps(ov, ensure_ascii=False).replace("'", "&#39;")
    n = f' layer-name="{name}"' if name else ""
    st = ""
    if style:
        st = " style=\"" + (style if isinstance(style, str) else ";".join(f"{k.replace('_','-')}:{v}" for k, v in style.items())) + "\""
    return f"<x-paper-instance component=\"{component}\"{n}{st} overrides='{o}'/>"


def init():
    fid = next(i["id"] for i in call("list_resources")["items"] if i["name"] == PROJECT)
    ps = pages(fid)
    call("import_components", {"fromPageId": ps["Design System"].id, "pageId": ps["Admin"].id})
    return fid, ps["Admin"]


def delete_section(P, name):
    ids = [c["id"] for c in P.c("get_children", nodeId="root")["children"] if c["name"] == name]
    if ids:
        P.c("delete_nodes", nodeIds=ids)


def write(P, parent, html):
    r = P.c("write_html", targetNodeId=parent, mode="insert-children", html=html)
    if r.get("ignoredStyles"):
        print("  ignored:", sorted(set(r["ignoredStyles"]))[:8])
    for w in r.get("warnings", []):
        print("  warn:", w)
    return r["createdNodeIds"]


def ensure_components(P):
    """Local main components (name `Group / Variant`): created on first run, kept afterwards."""
    have = {c["name"] for c in P.c("get_components")["components"]}
    specs = []
    toast = lambda nm, ic_, col_, bg, bd: row(nm, ic(ic_, 16, col_, "Icon") + T("Text", "Saved", 14, fg("text-primary")), gap="8px",
                                              padding="10px 16px", background=fg("bg-elevated"), border_radius="var(--radius-md)",
                                              box_shadow="var(--shadow-md)")
    specs.append(("Message / Success", toast("Message / Success", "check-circle", fg("success"), "", "")))
    specs.append(("Message / Error", toast("Message / Error", "x-circle", fg("error"), "", "")))
    hdr = row("Admin Shell Header / With Action",
              col("Titles", T("Title", "Challenges", 24, fg("text-primary"), 600, lh=32) + T("Subtitle", "Subtitle", 14, fg("text-secondary"))) +
              row("Actions", I("Button / Notification Bell") + I("Button / Primary", Label="New challenge"), gap="12px"),
              justify_content="space-between", width="1192px", padding="var(--page-padding)", background=fg("bg-elevated"),
              border="1px solid var(--color-border-light)")
    specs.append(("Admin Shell Header / With Action", hdr))
    for nm, h in specs:
        if nm in have:
            continue
        ids = write(P, "root", h)
        P.c("create_component", nodeId=ids[0], name=nm)
        P.c("update_styles", updates=[{"nodeIds": [ids[0]], "styles": {"left": 70000 + 400 * len(have), "top": 0}}])
        have.add(nm)


# --------------------------------------------------------------------------- antd-like pieces
def label(txt, required=False):
    return row("Label", (T("Required Mark", "*", 14, fg("error")) if required else "") + T("Label Text", txt, 14, fg("text-primary")), gap="4px")


def field(name, lab, ctrl, required=False, help_=None, err=None, width=None):
    kids = label(lab, required) + ctrl
    if err:
        kids += T("Error Message", err, 14, fg("error"))
    elif help_:
        kids += T("Help", help_, 12, fg("text-secondary"), lh=16)
    return col(name, kids, gap="8px", **({"width": width} if width else {}))


def acard(name, title, body, extra_head=""):
    """antd Card size=small with a title bar."""
    return col(name, row("Card Header", T("Card Title", title, 14, fg("text-primary"), 600), padding="0px 12px", height="38px", background=fg("bg-elevated")) +
               box("Divider", height="1px", background=fg("border-light")) +
               col("Card Body", body, padding="12px", gap="16px"),
               background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)", overflow="hidden")


def pcard(name, kids, **st):
    """packages/ui Card / antd Card default (24 padding)."""
    return col(name, kids, padding="var(--spacing-lg)", background=fg("bg-elevated"), border="1px solid var(--color-border-light)",
               border_radius="var(--radius-lg)", **st)


def dashed_btn(label_, name="Button / Dashed", block=False, small=False):
    h = 24 if small else 32
    return row(name, ic("plus", 12, fg("text-primary"), "Plus") + T("Label", label_.lstrip("+ "), 14, fg("text-primary")), gap="4px", justify_content="center",
               height=f"{h}px", padding="0px 12px" if small else "0px 16px", border="1px dashed var(--color-border)", border_radius="var(--radius-md)",
               background=fg("bg-elevated"), **({} if block else {"width": "fit-content"}))


def x_btn(name="Remove"):
    return row(name, ic("x", 14, fg("text-secondary"), "Close"), justify_content="center", width="32px", height="32px", border_radius="var(--radius-md)")


def circle_letter(txt, on=False, size=24):
    return row("Letter", T("Letter Text", txt, 12, fg("text-inverse") if on else fg("text-secondary"), 600, lh=16), justify_content="center",
               width=f"{size}px", height=f"{size}px", border_radius="99px", background=fg("success") if on else "#f0f0f0")


def md_editor(text_lines, height=320):
    tb = row("Toolbar", "".join(ic(n, 14, fg("text-disabled"), n) for n in ["text-b", "text-italic", "text-strikethrough", "text-h", "link", "quotes", "code", "image", "list-bullets", "list-numbers", "question"]),
             gap="16px", padding="0px 12px", height="32px", background=fg("code-bg"), border_bottom="1px solid #ffffff1f")
    body = col("Source", "".join(T("Line", l or "&nbsp;", 12, fg("code-fg"), 400, lh=16) for l in text_lines), gap="0px", padding="12px", flex_grow="1")
    return col("Markdown Editor", tb + body, height=f"{height}px", background=fg("code-bg"), border_radius="var(--radius-md)", overflow="hidden")


def code_block(path, lines, height=None):
    return col("Code Preview", T("File Path", path, 12, fg("text-secondary"), lh=16, padding="6px 12px", background=fg("bg-sider"), border_bottom="1px solid var(--color-border-light)") +
               col("Code", "".join(T("Line", l or "&nbsp;", 12, fg("code-fg"), 400, lh=18) for l in lines), padding="12px", background=fg("code-bg"), gap="0px",
                   **({"height": f"{height}px"} if height else {})),
               border="1px solid var(--color-border-light)", border_radius="var(--radius-md)", overflow="hidden")


TYPE_TAG = {"Algo": "Blue", "Fullstack": "Purple", "MCQ": "Cyan", "SQL": "Gold", "Theory": "Green"}


def type_tag(t):
    return I(f"Tag / {TYPE_TAG[t]}", Label=t)


# --------------------------------------------------------------------------- shell
def screen(name, route, active, title, subtitle, content, actions=False, action_label="New challenge", height=900, overlay=None, toast=None, content_gap=0):
    """Screen frame 1440 wide: dark sidebar + header + content. Returns (caption + screen) html."""
    sb = col("Sidebar", I(f"Admin Sidebar / {active} Active", style="flex-shrink:0"), width="248px", background=fg("sidebar-bg"))
    if actions:
        hd = I("Admin Shell Header / With Action", Title=title, Subtitle=subtitle, **{"Label": action_label})
    else:
        hd = I("Admin Shell Header", Title=title, Subtitle=subtitle)
    main = col("Main", hd + row("Content Area", col("Content", content, width="1100px", padding="var(--page-padding)", gap=f"{content_gap}px"),
                                justify_content="center", align_items="flex-start"), width="1192px")
    extra = overlay or ""
    if toast:
        extra += row("Toast Layer", toast, justify_content="center", position="absolute", left="248px", top="16px", width="1192px")
    return row(f"Screen / {name}", sb + main + extra, width="1440px", background=fg("bg-layout"), position="relative", overflow="hidden" if height == 900 else "visible", height=("900px" if height == 900 else "fit-content"))


def cap(route, extra=""):
    return T("Route Caption", route + (f"   {extra}" if extra else ""), 12, fg("text-secondary"), lh=16)


def shell(name, route, *a, caption_extra="", **kw):
    return col(f"Frame / {name}", cap(route, caption_extra) + screen(name, route, *a, **kw), gap="8px")


def section(name, title, items, x, y, cols=1):
    """items: html strings (each a `Frame / ...`). Arranged in rows of `cols`."""
    rows_ = ""
    for i in range(0, len(items), cols):
        rows_ += row(f"Row {i // cols + 1}", "".join(items[i:i + cols]), gap="var(--spacing-xl)", align_items="flex-start")
    return col(name, T("Section Title", title, 24, fg("text-primary"), 700, lh=32) + col("Screens", rows_, gap="var(--spacing-xl)"),
               padding="var(--spacing-xl)", gap="var(--spacing-xl)", background="transparent", position="absolute", left=f"{x}px", top=f"{y}px")


def overlay_html(inner, w=1440, h=900, name="Overlay"):
    return row(name, inner, justify_content="center", align_items="center", position="absolute", left="0px", top="0px", right="0px", bottom="0px",
               background=fg("bg-overlay"))
