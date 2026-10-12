"""Main components of the Design System page (top-level frames) + the registry used by the gallery and the manifest."""
from ds_common import *

SPECS = []          # [(name, html)]
GROUPS = {}         # group title -> [component names]
NOTES = {}          # component name -> short description (gallery caption)


def reg(group, name, html, note=""):
    SPECS.append((name, html))
    GROUPS.setdefault(group, []).append(name)
    NOTES[name] = note


def fg(c):  # shorthand
    return f"var(--color-{c})"


# ---------------------------------------------------------------- Buttons
BTN_SIZES = {"": (34, 16, 14), " Small": (28, 12, 12), " Large": (40, 20, 14)}
BTN_VARIANTS = {
    "Primary": dict(background="var(--color-brand-primary)", border="1px solid var(--color-brand-primary)", box_shadow=SHADOW_BTN, color="var(--color-text-inverse)"),
    "Default": dict(background="var(--color-bg-elevated)", border="1px solid var(--color-border)", color="var(--color-text-primary)"),
    "Ghost": dict(background="transparent", border="1px solid transparent", color="var(--color-text-secondary)"),
    "Danger": dict(background="var(--color-bg-elevated)", border="1px solid var(--color-error)", color="var(--color-error)"),
    "Dark": dict(background="var(--color-button-dark)", border="1px solid var(--color-button-dark)", color="var(--color-text-inverse)"),
}
for v, st in BTN_VARIANTS.items():
    for suf, (h, px, fs) in BTN_SIZES.items():
        name = f"Button / {v}{suf}"
        col_ = st["color"]
        sty = {k: x for k, x in st.items() if k != "color"}
        reg("Buttons (packages/ui Button)", name,
            row(name, T("Label", "Button", fs, col_, 500), justify_content="center", gap="8px", height=f"{h}px",
                padding=f"0 {px}px", border_radius="var(--radius-md)", **sty),
            f"{h}px high, padding 0 {px}px, {fs}px label")
reg("Buttons (packages/ui Button)", "Button / Notification Bell",
    row("Button / Notification Bell", ic("bell", 16, fg("text-primary"), "Bell") +
        box("Unread Dot", position="absolute", left="20px", top="6px", width="8px", height="8px", border_radius="99px", background=fg("error")),
        justify_content="center", position="relative", width="36px", height="36px", background=fg("bg-elevated"),
        border="1px solid var(--color-border)", border_radius="var(--radius-md)"),
    "AdminShell bell: 36x36, red unread dot")
# AntD buttons (type primary etc.): 32px, radius 8, 14px
ANT = {
    "Primary": dict(background=fg("brand-primary"), border="1px solid var(--color-brand-primary)", color=fg("text-inverse")),
    "Default": dict(background=fg("bg-elevated"), border="1px solid var(--color-border)", color=fg("text-primary")),
    "Danger": dict(background=fg("bg-elevated"), border="1px solid var(--color-error)", color=fg("error")),
    "Text": dict(background="transparent", border="1px solid transparent", color=fg("text-primary")),
    "Text Danger": dict(background="transparent", border="1px solid transparent", color=fg("error")),
}
for v, st in ANT.items():
    for suf, (h, px, fs) in {"": (32, 16, 14), " Small": (24, 8, 14), " Large": (40, 16, 16)}.items():
        name = f"Ant Button / {v}{suf}"
        sty = {k: x for k, x in st.items() if k != "color"}
        reg("Buttons (AntD Button)", name,
            row(name, T("Label", "Button", fs, st["color"]), justify_content="center", height=f"{h}px", padding=f"0 {px}px",
                border_radius="var(--radius-md)" if suf != " Small" else "var(--radius-sm)", **sty), f"AntD button {h}px")

# ---------------------------------------------------------------- Avatar
AV = {"Avatar": (32, 12), "Avatar / 30": (30, 12), "Avatar / 34": (34, 13), "Avatar / 36": (36, 14), "Avatar / 40": (40, 16)}
for name, (s, fs) in AV.items():
    reg("Avatar", name,
        row(name, T("Initials", "EV", fs, fg("text-inverse"), 600, letter_spacing="0.3px"), justify_content="center", width=f"{s}px", height=f"{s}px",
            border_radius="99px", background=fg("brand-primary")), f"{s}px circle, initials, colour by name hash")

# ---------------------------------------------------------------- Card / Stat
reg("Card", "Card",
    col("Card", T("Card Title", "Card title", 16, weight=600) + T("Card Body", "Card content goes here.", 14, fg("text-secondary")),
        width="360px", gap="8px", padding="var(--spacing-lg)", background=fg("bg-elevated"), border="1px solid var(--color-border-light)",
        border_radius="var(--radius-lg)", box_shadow="var(--shadow-sm)"), "Card: 24px padding, radius-lg, shadow-sm")
reg("Card", "Card / Hover",
    col("Card / Hover", T("Card Title", "Card title", 16, weight=600) + T("Card Body", "Card content goes here.", 14, fg("text-secondary")),
        width="360px", gap="8px", padding="var(--spacing-lg)", background=fg("bg-elevated"), border="1px solid var(--color-card-hover-border)",
        border_radius="var(--radius-lg)", box_shadow="var(--shadow-md)"), "hover: shadow-md, blue border")
reg("Card", "Card / Compact",
    col("Card / Compact", T("Card Title", "Card title", 16, weight=600) + T("Card Body", "Card content goes here.", 14, fg("text-secondary")),
        width="360px", gap="4px", padding="var(--spacing-md)", background=fg("bg-elevated"), border="1px solid var(--color-border-light)",
        border_radius="var(--radius-lg)", box_shadow="var(--shadow-sm)"), "pad=md (16px)")


def spark_svg(color, pts, name="Sparkline", w=72, h=30, gid=None):
    mn, mx = min(pts), max(pts)
    rng = (mx - mn) or 1
    n = len(pts) - 1
    p = [(i / n * w, h - (d - mn) / rng * (h - 4) - 2) for i, d in enumerate(pts)]
    line = " ".join(f"{x:.1f},{y:.1f}" for x, y in p)
    gid = gid or "g" + color[1:]
    return (f'<svg layer-name="{name}" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><linearGradient id="{gid}" x1="0" y1="0" x2="0" y2="1">'
            f'<stop offset="0%" stop-color="{color}" stop-opacity="0.22"/><stop offset="100%" stop-color="{color}" stop-opacity="0"/></linearGradient></defs>'
            f'<polygon points="0,{h} {line} {w},{h}" fill="url(#{gid})"/><polyline points="{line}" fill="none" stroke="{color}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>')


UP = [3, 5, 4, 7, 6, 9, 8, 10]
DOWN = [10, 9, 9, 7, 7, 5, 5, 4]
SPARK_COLORS = {"Blue": "#1677ff", "Purple": "#722ed1", "Orange": "#d48806", "Green": "#52c41a", "Teal": "#13a8a8"}
ACCENT_VAR = {"Blue": "brand-primary", "Purple": "avatar-purple", "Orange": "warning", "Green": "success", "Teal": "avatar-teal"}
for cn, hexc in SPARK_COLORS.items():
    reg("Sparkline", f"Sparkline / {cn}", box(f"Sparkline / {cn}", spark_svg(hexc, UP, "Line", gid="s" + cn), display="flex"), "72x30 line + gradient area")
reg("Sparkline", "Sparkline / Orange Down", box("Sparkline / Orange Down", spark_svg("#d48806", DOWN, "Line", gid="sod"), display="flex"), "downward trend")


def stat(name, cn, down=False, hexc=None):
    hexc = hexc or SPARK_COLORS[cn]
    dc = fg("error") if down else fg("success")
    body = (T("Label", "ACTIVE CANDIDATES", 12, fg("text-secondary"), 500, letter_spacing="0.48px") +
            row("Value Row", T("Value", "12", 30, fg("text-primary"), 700) + spark_svg(hexc, DOWN if down else UP, "Sparkline", gid="st" + name.replace(" ", "").replace("/", "")),
                justify_content="space-between", gap="8px") +
            row("Delta Row", T("Delta Arrow", "▼" if down else "▲", 12, dc, 600) + T("Delta", "-1" if down else "+3", 12, dc, 600) + T("Trend", "this week", 12, fg("text-disabled")), gap="4px"))
    return row(name, box("Accent Bar", background=fg(ACCENT_VAR[cn]), width="4px", align_self="stretch") +
               col("Stat Body", body, gap="8px", padding="var(--spacing-lg)", flex_grow="1"),
               align_items="stretch", width="208px", background=fg("bg-elevated"), border="1px solid var(--color-border-light)",
               border_radius="var(--radius-lg)", box_shadow="var(--shadow-sm)", overflow="hidden")


for cn in SPARK_COLORS:
    reg("Stat", f"Stat / {cn}", stat(f"Stat / {cn}", cn), "label, value 30/700, sparkline, delta line")
reg("Stat", "Stat / Orange Down", stat("Stat / Orange Down", "Orange", True), "negative delta: red arrow")

# ---------------------------------------------------------------- Tag, status
TONES = {"Blue": "blue", "Green": "green", "Gold": "gold", "Red": "red", "Purple": "purple", "Cyan": "cyan"}


def tag(name, fgc, bgc, bdc, dot=False, label="Tag"):
    kids = (box("Dot", width="8px", height="8px", border_radius="99px", background=fgc) if dot else "") + T("Label", label, 12, fgc, 500, lh=16)
    return row(name, kids, gap="8px", height="22px", padding="0 8px", background=bgc, border=f"1px solid {bdc}", border_radius="var(--radius-sm)")


for cn, t in TONES.items():
    f, b, d = (f"var(--color-tag-{t}-{x})" for x in ("fg", "bg", "bd"))
    reg("Tag", f"Tag / {cn}", tag(f"Tag / {cn}", f, b, d), "22px chip, 12px/500")
    reg("Tag", f"Tag Dot / {cn}", tag(f"Tag Dot / {cn}", f, b, d, True), "with status dot")
reg("Tag", "Tag / Gray", tag("Tag / Gray", fg("text-secondary"), fg("bg-sider"), fg("border")), "neutral")
reg("Tag", "Tag Dot / Gray", tag("Tag Dot / Gray", fg("text-secondary"), fg("bg-sider"), fg("border"), True), "neutral with dot")
for nm, f, b, d in [("Default", fg("text-primary"), fg("bg-sider"), fg("border")),
                    ("Success", fg("success"), "var(--color-tag-green-bg)", "var(--color-tag-green-bd)"),
                    ("Warning", fg("warning"), "var(--color-tag-gold-bg)", "var(--color-tag-gold-bd)"),
                    ("Error", fg("error"), "var(--color-tag-red-bg)", "var(--color-tag-red-bd)"),
                    ("Processing", fg("brand-primary"), "var(--color-tag-blue-bg)", "var(--color-tag-blue-bd)")]:
    reg("Tag (AntD Tag status colors)", f"Ant Tag / {nm}", tag(f"Ant Tag / {nm}", f, b, d), "antd <Tag color=...>")
for st, t in [("Queued", "blue"), ("Running", "gold"), ("Graded", "green"), ("Errored", "red")]:
    f, b, d = (f"var(--color-tag-{t}-{x})" for x in ("fg", "bg", "bd"))
    reg("Submission Status Badge", f"Submission Status Badge / {st}", tag(f"Submission Status Badge / {st}", f, b, d, label=st), "SubmissionStatusBadge")
reg("Submission Status Badge", "Submission Status Badge / Unknown", tag("Submission Status Badge / Unknown", fg("text-primary"), fg("bg-sider"), fg("border"), label="Unknown"), "SubmissionStatusBadge")

# ---------------------------------------------------------------- Pill, Segmented, Search
for act in (True, False):
    for cnt in (False, True):
        nm = f"Pill / {'Active' if act else 'Inactive'}{' With Count' if cnt else ''}"
        c = fg("brand-primary") if act else fg("text-secondary")
        kids = T("Label", "All", 12, c, 500, lh=16) + (T("Count", "12", 12, c, 700, lh=16, opacity="0.8") if cnt else "")
        reg("Pill (filter chip)", nm, row(nm, kids, gap="8px", height="30px", padding="0 12px", border_radius="99px",
                                          background=fg("bg-selected") if act else fg("bg-elevated"),
                                          border=f"1px solid {fg('brand-primary') if act else fg('border')}"), "filter chip, radius 99")


def search(name, state):
    b = fg("brand-primary") if state == "focus" else fg("border")
    txt = T("Placeholder", "Search…" if state != "filled" else "backend", 14, fg("text-disabled") if state != "filled" else fg("text-primary"))
    return row(name, ic("magnifying-glass", 14, fg("text-disabled"), "Search Icon") + txt, gap="8px", width="260px", height="34px", padding="0 12px",
               background=fg("bg-elevated"), border=f"1px solid {b}", border_radius="var(--radius-md)",
               **({"box_shadow": SHADOW_FOCUS} if state == "focus" else {}))


reg("Search Input", "Search Input", search("Search Input", "idle"), "260x34")
reg("Search Input", "Search Input / Focused", search("Search Input / Focused", "focus"), "focus ring")
reg("Search Input", "Search Input / Filled", search("Search Input / Filled", "filled"), "with value")


def seg(n, a, small=False):
    nm = f"Segmented / {n} Options Active {a}"
    items = ""
    for k in range(1, n + 1):
        on = k == a
        items += row(f"Segment {k}", T(f"Option {k}", f"Option {k}", 12, fg("text-primary") if on else fg("text-secondary"), 600 if on else 500, lh=16),
                     justify_content="center", height="28px", padding="0 16px", border_radius="var(--radius-sm)",
                     background=fg("bg-elevated") if on else "transparent", **({"box_shadow": "var(--shadow-sm)"} if on else {}))
    return nm, row(nm, items, padding="4px", background=fg("border-light"), border_radius="var(--radius-md)")


for n in (2, 3, 4):
    for a in range(1, n + 1):
        nm, h = seg(n, a)
        reg("Segmented (packages/ui)", nm, h, "track border-light, active = white + shadow-sm")


def aseg(a):
    nm = f"Ant Segmented / 3 Options Active {a}"
    items = ""
    for k in range(1, 4):
        on = k == a
        items += row(f"Segment {k}", T(f"Option {k}", f"Option {k}", 14, fg("text-primary") if on else fg("text-secondary")), justify_content="center",
                     height="28px", padding="0 12px", border_radius="var(--radius-md)", background=fg("bg-elevated") if on else "transparent",
                     **({"box_shadow": "var(--shadow-sm)"} if on else {}))
    return nm, row(nm, items, padding="4px", background=fg("bg-layout"), border_radius="var(--radius-md)")


for a in (1, 2, 3):
    nm, h = aseg(a)
    reg("Segmented (AntD)", nm, h, "antd Segmented (challenges page)")

# ---------------------------------------------------------------- Scores, stars, bar
for nm, fill, w in [("High", "success", 128), ("Mid", "warning", 96), ("Low", "error", 48)]:
    reg("Score Bar / Dots / Stars", f"Score Bar / {nm}",
        box(f"Score Bar / {nm}", box("Fill", width=f"{w}px", height="6px", border_radius="99px", background=fg(fill)),
            display="flex", width="160px", height="6px", border_radius="99px", background=fg("border-light"), overflow="hidden"),
        "160x6 track; set Fill width in px")
LABELS = {1: "Poor", 2: "Below bar", 3: "At bar", 4: "Above bar", 5: "Exceptional"}
for v in range(0, 6):
    c = {5: fg("success"), 4: fg("success"), 3: fg("brand-primary"), 2: fg("score-warn"), 1: fg("score-poor"), 0: fg("score-poor")}[v]
    dots = ""
    for k in range(1, 6):
        on = k <= v
        dots += row(f"Dot {k}", T(f"Dot {k} Value", str(k) if on else "", 12, fg("text-inverse"), 600, lh=16), justify_content="center", width="26px", height="26px",
                    border_radius="var(--radius-sm)", background=c if on else fg("bg-elevated"), border=f"1px solid {c if on else fg('border')}")
    nm = f"Score Dots / {v}"
    reg("Score Bar / Dots / Stars", nm, row(nm, row("Dots", dots, gap="4px") + T("Label", LABELS.get(v, ""), 12, fg("text-primary"), 600, lh=16, width="76px"), gap="16px"), "5 squares 26px + label")
for v in range(0, 6):
    nm = f"Stars / {v}"
    reg("Score Bar / Dots / Stars", nm, row(nm, box("Stars", f'<span style="color:var(--color-star)">{"★" * v}</span><span style="color:var(--color-border)">{"☆" * (5 - v)}</span>',
        font_size="13px", letter_spacing="1px", line_height="16px")), "★ gold / ☆ border")

# ---------------------------------------------------------------- Donut / Spin / Empty / Loading
import math


def donut(segs, size=132, th=16):
    r = (size - th) / 2
    c = size / 2
    tot = sum(v for v, _ in segs)
    a0 = -math.pi / 2
    paths = ""
    for v, col_ in segs:
        a1 = a0 + v / tot * 2 * math.pi
        large = 1 if (a1 - a0) > math.pi else 0
        paths += f'<path d="M{c + r * math.cos(a0):.2f} {c + r * math.sin(a0):.2f} A{r} {r} 0 {large} 1 {c + r * math.cos(a1):.2f} {c + r * math.sin(a1):.2f}" fill="none" stroke="{col_}" stroke-width="{th}"/>'
        a0 = a1
    ring = f'<svg layer-name="Ring" width="{size}" height="{size}" viewBox="0 0 {size} {size}"><circle cx="{c}" cy="{c}" r="{r}" fill="none" stroke="#f0f0f0" stroke-width="{th}"/>{paths}</svg>'
    center = col("Center", T("Total", "42", 26, fg("text-primary"), 700, lh=32) + T("Caption", "sessions", 12, fg("text-secondary"), lh=16), align_items="center",
                 justify_content="center", position="absolute", left="0px", top="0px", width=f"{size}px", height=f"{size}px")
    return ring + center


reg("Donut / Spin / Empty / Loading", "Donut", box("Donut", donut([(18, "#52c41a"), (12, "#1677ff"), (8, "#faad14"), (4, "#ff4d4f")]), position="relative", display="flex", width="132px", height="132px"),
    "132px ring, 16px stroke; segments = stroke colors")


def spin(name, s):
    r = s * 0.2
    o = s * 0.35
    c = s / 2
    pts = [(c, c - o, 1), (c + o, c, .3), (c, c + o, .3), (c - o, c, .3)]
    cs = "".join(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{r:.1f}" fill="#1677ff" fill-opacity="{op}"/>' for x, y, op in pts)
    return box(name, f'<svg layer-name="Dots" width="{s}" height="{s}" viewBox="0 0 {s} {s}">{cs}</svg>', display="flex")


reg("Donut / Spin / Empty / Loading", "Spin / Small", spin("Spin / Small", 16), "antd Spin small")
reg("Donut / Spin / Empty / Loading", "Spin", spin("Spin", 20), "antd Spin default")
reg("Donut / Spin / Empty / Loading", "Spin / Large", spin("Spin / Large", 32), "antd Spin large")
reg("Donut / Spin / Empty / Loading", "Loading Block",
    row("Loading Block", spin("Spin Icon", 32) + T("Tip", "Loading…", 14, fg("text-secondary")), justify_content="center", gap="8px", padding="var(--spacing-xl)", width="360px"),
    "LoadingBlock: Spin + tip")

EMPTY_SVG = ('<svg layer-name="Illustration" width="64" height="41" viewBox="0 0 64 41"><ellipse cx="32" cy="33" rx="32" ry="7" fill="#f5f5f5"/>'
             '<path d="M55 12.8 44.9 1.3C44.4.5 43.7 0 42.9 0H21.1c-.7 0-1.5.5-1.9 1.3L9 12.8V22h46z" fill="#fafafa" stroke="#d9d9d9"/>'
             '<path d="M41.6 16c0-1.7 1-3 2.3-3H55v18c0 2-1.5 3.5-3.4 3.5H12.4C10.5 34.5 9 33 9 31V13h11.1c1.3 0 2.3 1.3 2.3 3 0 1.6 1 3 2.3 3h14.6c1.3 0 2.3-1.4 2.3-3z" fill="#ffffff" stroke="#d9d9d9"/></svg>')
reg("Donut / Spin / Empty / Loading", "Empty / Simple",
    col("Empty / Simple", EMPTY_SVG + T("Description", "No data", 14, fg("text-disabled")), align_items="center", gap="8px", padding="var(--spacing-xl)", width="360px"), "antd Empty (simple image)")
reg("Donut / Spin / Empty / Loading", "Empty State",
    col("Empty State", EMPTY_SVG + col("Texts", T("Title", "No sessions yet", 14, fg("text-primary"), 500, text_align="center") +
        T("Description", "Create a session to get started.", 14, fg("text-secondary"), text_align="center"), align_items="center"),
        align_items="center", gap="8px", padding="var(--spacing-xl)", width="360px"), "EmptyState: title + description")

# ---------------------------------------------------------------- Page headers
HEAD_W = "1192px"


def title_block(t, s):
    return col("Titles", T("Title", t, 24, fg("text-primary"), 600, lh=32) + T("Subtitle", s, 14, fg("text-secondary")), gap="0px")


reg("Page headers", "Page Header",
    row("Page Header", title_block("Page title", "Page subtitle") , justify_content="space-between", width=HEAD_W, padding="var(--page-padding)",
        background=fg("bg-elevated"), border="1px solid var(--color-border-light)"), "PageHeader w/o actions (1192 = 1440 - sidebar)")
reg("Page headers", "Admin Shell Header",
    row("Admin Shell Header", title_block("Dashboard", "Welcome to your dashboard") +
        row("Actions", "<x-paper-instance component=\"Button / Notification Bell\"/>", gap="12px"),
        justify_content="space-between", width=HEAD_W, padding="var(--page-padding)", background=fg("bg-elevated"), border="1px solid var(--color-border-light)"),
    "AdminShell header: title, subtitle, bell (instance of Button / Notification Bell)")
reg("Page headers", "Candidate Shell Header",
    col("Candidate Shell Header", row("Header Card", title_block("Smoke Candidate", "Session af17f53a") +
        row("Exit Session", ic("sign-out", 16, fg("text-primary"), "Exit Icon") + T("Exit Label", "Exit session", 14, fg("text-primary")), justify_content="center", gap="8px", height="34px",
            padding="0 16px", background=fg("bg-elevated"), border="1px solid var(--color-border)", border_radius="var(--radius-md)"),
        justify_content="space-between", width="1144px", padding="var(--page-padding)", background=fg("bg-elevated")),
        width=HEAD_W, padding="16px 24px", background=fg("bg-layout")), "CandidateShell header (white card inset on layout bg)")
reg("Page headers", "Section Title",
    row("Section Title", col("Texts", T("Title", "Hiring pipeline", 16, fg("text-primary"), 600, lh=22) + T("Subtitle", "Across all open roles", 12, fg("text-secondary"), lh=16)),
        justify_content="space-between", width="360px"), "SectionTitle: 16/600 + 12 sub")
reg("Page headers", "Section Title / With Action",
    row("Section Title / With Action", col("Texts", T("Title", "Upcoming interviews", 16, fg("text-primary"), 600, lh=22) + T("Subtitle", "Next 7 days", 12, fg("text-secondary"), lh=16)) +
        T("Action", "Library →", 14, fg("text-secondary")), justify_content="space-between", width="360px"), "with action slot")

# ---------------------------------------------------------------- Data table
TW = "960px"


def cells(prefix, texts, **st):
    return "".join(T(f"{prefix} {i}", t, st.pop("size", 14) if False else st.get("size", 14), st.get("color", fg("text-primary")), st.get("weight", 400),
                     flex_grow="1", **({"letter_spacing": "0.36px"} if st.get("ls") else {})) for i, t in enumerate(texts, 1))


def tcells(prefix, texts, size, color, weight, ls=None):
    return "".join(T(f"{prefix} {i}", t, size, color, weight, flex="1", **({"letter_spacing": ls} if ls else {})) for i, t in enumerate(texts, 1))


reg("Data Table (packages/ui DataTable)", "Data Table / Header Row",
    row("Data Table / Header Row", tcells("Col", ["TITLE", "TYPE", "STATUS", "SCORE", "UPDATED"], 12, fg("text-secondary"), 600, "0.36px"), width=TW, padding="12px 16px", gap="16px",
        background=fg("bg-sider"), border="1px solid var(--color-border-light)"), "uppercase 12/600; hide unused Col n with display:none")
for nm, bgc in [("Row", "bg-elevated"), ("Row Hover", "bg-selected")]:
    reg("Data Table (packages/ui DataTable)", f"Data Table / {nm}",
        row(f"Data Table / {nm}", tcells("Cell", ["Two Sum", "Coding", "Active", "4.2", "Jun 12"], 14, fg("text-primary"), 400), width=TW, padding="12px 16px", gap="16px",
            background=fg(bgc), border="1px solid var(--color-border-light)"), "13px vertical padding in code (comfortable)")


def page_btn(n, on=False, name=None):
    return row(name or f"Page {n}", T("Page Label", n, 14, fg("brand-primary") if on else fg("text-primary")), justify_content="center", width="32px", height="32px",
               border_radius="var(--radius-md)", background=fg("bg-elevated"), border=f"1px solid {fg('brand-primary') if on else fg('border')}" if on else "1px solid transparent")


PAGER = (row("Prev", ic("caret-left", 14, fg("text-secondary"), "Prev Icon"), justify_content="center", width="32px", height="32px") + page_btn("1", True) + page_btn("2") + page_btn("3") +
         page_btn("…", name="Page More") + page_btn("9") + row("Next", ic("caret-right", 14, fg("text-secondary"), "Next Icon"), justify_content="center", width="32px", height="32px"))
reg("Data Table (packages/ui DataTable)", "Pagination", row("Pagination", PAGER, gap="8px"), "antd Pagination: 32px items, active = blue border")
reg("Data Table (packages/ui DataTable)", "Data Table / Footer",
    row("Data Table / Footer", T("Summary", "Showing 1-5 of 42", 12, fg("text-secondary"), lh=16) + row("Pager", PAGER, gap="8px"), justify_content="space-between", width=TW,
        padding="12px 16px", background=fg("bg-elevated")), "summary + pagination")
reg("Data Table (packages/ui DataTable)", "Data Table / Empty",
    row("Data Table / Empty", T("Message", "No rows match your filters.", 14, fg("text-disabled"), text_align="center"), justify_content="center", width=TW, padding="var(--spacing-xxl)",
        background=fg("bg-elevated")), "emptyMessage, 48px padding")
reg("Ant Table", "Ant Table / Header Row",
    row("Ant Table / Header Row", tcells("Col", ["Title", "Code", "Status", "Created", "Actions"], 14, fg("text-primary"), 600), width=TW, padding="16px", gap="16px",
        background=fg("bg-sider"), border="1px solid var(--color-border-light)"), "antd Table th (#fafafa, 14/600)")
reg("Ant Table", "Ant Table / Row",
    row("Ant Table / Row", tcells("Cell", ["Backend Engineer", "A1B2C3", "Scheduled", "Jun 12", "View"], 14, fg("text-primary"), 400), width=TW, padding="16px", gap="16px",
        background=fg("bg-elevated"), border="1px solid var(--color-border-light)"), "antd Table td, row ~55px")

# ---------------------------------------------------------------- Form controls
CTRL = dict(background=fg("bg-elevated"), border_radius="var(--radius-md)")


def inp(name, ph="", val=None, large=False, state="", w=280, prefix=None, suffix=None, disabled=False):
    h, fs, pad = (40, 16, 12) if large else (32, 14, 12)
    b = {"focus": fg("brand-primary"), "error": fg("error")}.get(state, fg("border"))
    kids = (T("Prefix", prefix, fs, fg("text-secondary")) if prefix else "")
    kids += T("Value", val if val is not None else ph, fs, fg("text-disabled") if disabled else (fg("text-primary") if val is not None else fg("text-placeholder")), flex_grow="1")
    kids += suffix or ""
    st = dict(CTRL)
    if disabled:
        st["background"] = fg("bg-layout")
    if state == "focus":
        st["box_shadow"] = "0 0 0 2px #1677ff33"
    if state == "error":
        st["box_shadow"] = "0 0 0 2px #cf132233"
    return row(name, kids, gap="4px", width=f"{w}px", height=f"{h}px", padding=f"0 {pad}px", border=f"1px solid {b}", **st)


reg("Input (AntD)", "Input", inp("Input", "Placeholder"), "32px, radius 8, border #d9d9d9")
reg("Input (AntD)", "Input / Filled", inp("Input / Filled", val="Netcracker"), "with value")
reg("Input (AntD)", "Input / Focus", inp("Input / Focus", val="Netcracker", state="focus"), "focus ring")
reg("Input (AntD)", "Input / Error", inp("Input / Error", val="invalid", state="error"), "error state")
reg("Input (AntD)", "Input / Disabled", inp("Input / Disabled", val="Disabled", disabled=True), "disabled")
reg("Input (AntD)", "Input / Large", inp("Input / Large", "Placeholder", large=True), "size=large: 40px / 16px (forms)")
reg("Input (AntD)", "Input / Large Filled", inp("Input / Large Filled", val="Netcracker", large=True), "size=large with value")
reg("Input (AntD)", "Input / Large Focus", inp("Input / Large Focus", val="Netcracker", large=True, state="focus"), "size=large focus")
reg("Input (AntD)", "Input / Large Error", inp("Input / Large Error", val="invalid", large=True, state="error"), "size=large error")
reg("Input (AntD)", "Input / With Prefix", inp("Input / With Prefix", val="netcracker", large=True, prefix="hireloop.io/"), "Workspace URL field")
reg("Input (AntD)", "Input / Password", inp("Input / Password", "Password", large=True, suffix=ic("eye", 16, fg("text-disabled"), "Eye Icon")), "Input.Password large")
reg("Input (AntD)", "Input / Search", inp("Input / Search", "Search templates…", suffix=ic("magnifying-glass", 14, fg("text-disabled"), "Search Icon")), "Input.Search")
reg("Input (AntD)", "Input Number",
    row("Input Number", T("Value", "60", 14, fg("text-primary"), flex_grow="1") +
        col("Steppers", row("Up", ic("caret-up", 10, fg("text-disabled"), "Up Icon"), justify_content="center", height="15px") + row("Down", ic("caret-down", 10, fg("text-disabled"), "Down Icon"), justify_content="center", height="15px"), width="22px"),
        gap="4px", width="120px", height="32px", padding="0 0 0 12px", border="1px solid var(--color-border)", **CTRL), "InputNumber (steppers on the right)")
reg("Input (AntD)", "Date Picker", inp("Date Picker", "Select date", w=200, suffix=ic("calendar-blank", 14, fg("text-disabled"), "Calendar Icon")), "DatePicker")
reg("Input (AntD)", "Textarea",
    row("Textarea", T("Value", "Brief description of your team or hiring process", 14, fg("text-placeholder"), flex_grow="1"), align_items="flex-start", width="280px", height="76px",
        padding="4px 12px", border="1px solid var(--color-border)", **CTRL), "Input.TextArea, 3 rows")
reg("Input (AntD)", "Textarea / Filled",
    row("Textarea / Filled", T("Value", "Write your answer here.", 14, fg("text-primary"), flex_grow="1"), align_items="flex-start", width="280px", height="76px",
        padding="4px 12px", border="1px solid var(--color-border)", **CTRL), "filled")
for large in (False, True):
    for filled in (False, True):
        nm = f"Select{' / Large' if large else ''}{' Filled' if filled and large else (' / Filled' if filled else '')}"
        reg("Select (AntD)", nm, inp(nm, "Select…", val="UTC" if filled else None, large=large, suffix=ic("caret-down", 12, fg("text-disabled"), "Arrow Icon")), "Select closed")
reg("Select (AntD)", "Select / Dropdown",
    col("Select / Dropdown", "".join(row(f"Option {i}", T(f"Option {i} Label", t, 14, fg("text-primary"), 600 if i == 2 else 400), height="32px", padding="0 12px", border_radius="var(--radius-sm)",
                                         **({"background": fg("bg-selected")} if i == 2 else {})) for i, t in enumerate(["UTC", "UTC+3 (Moscow)", "UTC+1 (CET)"], 1)),
        width="280px", padding="4px", background=fg("bg-elevated"), border_radius="var(--radius-md)", box_shadow="var(--shadow-md)"), "open options panel; selected = bg-selected + 600")

for chk in (False, True):
    nm = f"Checkbox / {'Checked' if chk else 'Unchecked'}"
    bx = row("Box", ic("check", 12, fg("text-inverse"), "Check Icon") if chk else "", justify_content="center", width="16px", height="16px", border_radius="var(--radius-sm)",
             background=fg("brand-primary") if chk else fg("bg-elevated"), border=f"1px solid {fg('brand-primary') if chk else fg('border')}")
    reg("Checkbox / Radio / Switch", nm, row(nm, bx + T("Label", "Keep me signed in", 14, fg("text-primary")), gap="8px"), "16px box")
for sel in (False, True):
    nm = f"Radio / {'Selected' if sel else 'Unselected'}"
    bx = row("Circle", box("Dot", width="8px", height="8px", border_radius="99px", background=fg("brand-primary")) if sel else "", justify_content="center", width="16px", height="16px",
             border_radius="99px", background=fg("bg-elevated"), border=f"1px solid {fg('brand-primary') if sel else fg('border')}")
    reg("Checkbox / Radio / Switch", nm, row(nm, bx + T("Label", "Option label", 14, fg("text-primary")), gap="8px"), "radio")
for on in (False, True):
    nm = f"Switch / {'On' if on else 'Off'}"
    reg("Checkbox / Radio / Switch", nm, row(nm, box("Knob", width="18px", height="18px", border_radius="99px", background=fg("bg-elevated")), width="44px", height="22px", padding="0 4px",
        border_radius="99px", background=fg("brand-primary") if on else fg("switch-off"), justify_content="flex-end" if on else "flex-start"), "44x22")


def field(name, label, req=False, err=None, help_=None, val=None, large=True):
    lab = row("Label Row", (T("Required Mark", "*", 14, fg("error")) if req else "") + T("Label", label, 14, fg("text-primary")), gap="4px", height="32px")
    ctrl = inp("Input", "Placeholder", val=val, large=large, state="error" if err else "", w=480)
    tail = T("Help", err or help_, 14, fg("error") if err else fg("text-secondary")) if (err or help_) else ""
    return col(name, lab + ctrl + tail, gap="0px", width="480px")


reg("Form Field", "Form Field", field("Form Field", "Workspace name", val="Netcracker"), "label + large input (AntD Form.Item vertical)")
reg("Form Field", "Form Field / Required", field("Form Field / Required", "Email", req=True), "* required mark")
reg("Form Field", "Form Field / Error", field("Form Field / Error", "Email", req=True, err="Please enter a valid email", val="x@"), "error + message")
reg("Form Field", "Form Field / With Help", field("Form Field / With Help", "Workspace URL", help_="Used in invite links."), "help text")

# ---------------------------------------------------------------- Modal, Drawer, Tabs, Alert, Tooltip, Progress, Skeleton
reg("Overlays", "Modal",
    col("Modal", row("Header", T("Title", "Modal title", 16, fg("text-primary"), 600) + ic("x", 14, fg("text-secondary"), "Close Icon"), justify_content="space-between") +
        T("Body", "Modal body content.", 14, fg("text-primary"), height="96px") +
        row("Footer", row("Cancel Button", T("Cancel Label", "Cancel", 14, fg("text-primary")), justify_content="center", height="32px", padding="0 16px", border="1px solid var(--color-border)", border_radius="var(--radius-md)", background=fg("bg-elevated")) +
            row("OK Button", T("OK Label", "OK", 14, fg("text-inverse")), justify_content="center", height="32px", padding="0 16px", background=fg("brand-primary"), border_radius="var(--radius-md)"),
            justify_content="flex-end", gap="8px"),
        width="520px", gap="12px", padding="20px 24px", background=fg("bg-elevated"), border_radius="var(--radius-lg)", box_shadow="var(--shadow-lg)"), "antd Modal; put on a bg-overlay frame")
reg("Overlays", "Drawer",
    col("Drawer", row("Header", T("Title", "Notes (markdown)", 16, fg("text-primary"), 600) + ic("x", 14, fg("text-secondary"), "Close Icon"), justify_content="space-between", padding="16px 24px", border="1px solid var(--color-border-light)") +
        col("Body Wrap", T("Body", "Drawer body", 14, fg("text-primary")), padding="var(--spacing-lg)"), width="420px", height="900px", background=fg("bg-elevated"), box_shadow="var(--shadow-lg)"),
    "antd Drawer (right, full height)")
reg("Overlays", "Tooltip",
    col("Tooltip", row("Bubble", T("Text", "Resolve all conflicts first", 14, fg("text-inverse")), padding="6px 12px", background=fg("tooltip-bg"), border_radius="var(--radius-md)") +
        '<svg layer-name="Arrow" width="16" height="6" viewBox="0 0 16 6"><polygon points="0,0 16,0 8,6" fill="#000000" fill-opacity="0.85"/></svg>', align_items="center"), "antd Tooltip")
for t, (bg_, bd_, icn, icc) in {"Info": ("tag-blue-bg", "tag-blue-bd", "info", "brand-primary"), "Success": ("tag-green-bg", "tag-green-bd", "check-circle", "success"),
                                "Warning": ("tag-gold-bg", "tag-gold-bd", "warning-circle", "score-warn"), "Error": ("tag-red-bg", "tag-red-bd", "x-circle", "score-poor")}.items():
    for icon_on in (False, True):
        nm = f"Alert / {t}{' With Icon' if icon_on else ''}"
        reg("Alert", nm, row(nm, (ic(icn, 16, fg(icc), "Alert Icon") if icon_on else "") + T("Message", f"{t} message goes here.", 14, fg("text-primary"), flex_grow="1"),
                             gap="8px", width="400px", padding="8px 12px", background=fg(bg_), border=f"1px solid {fg(bd_)}", border_radius="var(--radius-md)"), "antd Alert")
for i, nm_ in enumerate(["Tab"], 1):
    pass
for on in (True, False):
    for large in (False, True):
        nm = f"Tabs / Tab {'Active' if on else 'Inactive'}{' Large' if large else ''}"
        reg("Tabs", nm, col(nm, row("Tab Pad", T("Label", "Workspace", 16 if large else 14, fg("brand-primary") if on else fg("text-primary")), padding="12px 0") +
                            box("Ink Bar", align_self="stretch", height="2px", background=fg("brand-primary") if on else "transparent"), align_items="stretch"), "antd Tabs item (ink bar when active)")


def tabbar(labels, a, large):
    items = ""
    for k, l in enumerate(labels, 1):
        on = k == a
        items += col(f"Tab {k}", row(f"Tab {k} Pad", T(f"Tab {k} Label", l, 16 if large else 14, fg("brand-primary") if on else fg("text-primary")), padding="12px 0") +
                     box(f"Tab {k} Ink", align_self="stretch", height="2px", background=fg("brand-primary") if on else "transparent"))
    return items


for a in (1, 2, 3):
    nm = f"Tabs / Bar 3 Active {a}"
    reg("Tabs", nm, col(nm, row("Tabs", tabbar(["Overview", "Details", "History"], a, False), gap="32px") + box("Divider", align_self="stretch", height="1px", background=fg("border-light")), width="480px", gap="0px"), "antd Tabs bar, 14px")
L5 = ["Workspace", "Team", "Scoring", "Notifications", "Integrations"]
for a in range(1, 6):
    nm = f"Tabs / Bar 5 Large Active {a}"
    reg("Tabs", nm, col(nm, row("Tabs", tabbar(L5, a, True), gap="32px") + box("Divider", align_self="stretch", height="1px", background=fg("border-light")), width="720px", gap="0px"), "configuration page tabs (size=large)")

reg("Misc AntD", "Progress / Circle",
    box("Progress / Circle", '<svg layer-name="Ring" width="64" height="64" viewBox="0 0 64 64"><circle cx="32" cy="32" r="29" fill="none" stroke="#f0f0f0" stroke-width="6"/>'
        '<path d="M32 3 A29 29 0 1 1 8.6 49" fill="none" stroke="#52c41a" stroke-width="6" stroke-linecap="round"/></svg>' +
        row("Center", T("Value", "85", 16, fg("text-primary")), justify_content="center", position="absolute", left="0px", top="0px", width="64px", height="64px"),
        position="relative", display="flex", width="64px", height="64px"), "Progress type=circle size=64")
reg("Misc AntD", "Skeleton",
    col("Skeleton", box("Title Bar", width="152px", height="16px", border_radius="var(--radius-sm)", background=fg("border-light")) +
        "".join(box(f"Line {i}", width="400px" if i < 4 else "244px", height="16px", border_radius="var(--radius-sm)", background=fg("border-light")) for i in range(1, 5)),
        gap="16px", width="400px"), "antd Skeleton active (static)")

# ---------------------------------------------------------------- App shell
NAV = {"Workspace": [("Dashboard", "squares-four"), ("Challenges", "list-bullets"), ("Question bank", "database"), ("Sessions", "clock"), ("Schedule", "calendar")],
       "Manage": [("Configuration", "gear"), ("Rubric", "table")]}


def nav_item(label, icon, active=False):
    c = fg("text-inverse") if active else fg("sidebar-text")
    return row(f"Nav / {label}", ic(icon, 18, c, f"Nav Icon / {label}") + T(f"Nav Label / {label}", label, 14, c, 600 if active else 500, flex_grow="1"),
               gap="12px", height="36px", padding="0 12px", border_radius="var(--radius-md)", background=fg("brand-primary") if active else "transparent")


def brand(mark_html, sub):
    return row("Brand", row("Brand Mark", mark_html, justify_content="center", width="30px", height="30px", border_radius="var(--radius-md)", background=fg("brand-primary")) +
               col("Brand Text", T("Brand Name", "Hireloop", 16, fg("text-inverse"), 700, lh=18) + T("Brand Sub", sub, 12, fg("sidebar-text-muted"), lh=16), gap="0px"),
               gap="8px", padding="16px 16px 12px")


def user_footer(name, email_or_role, tag=None, admin=True):
    ini = "".join(p[0] for p in name.split()[:2]).upper()
    meta = row("User Name Row", T("User Name", name, 13, fg("text-inverse"), 600, lh=18) + (row("User Role Tag", T("User Role Tag Label", tag, 12, fg("brand-secondary"), 700, lh=16), padding="0 8px", border_radius="var(--radius-sm)", background=fg("sidebar-tag-bg")) if tag else ""), gap="8px") + \
        T("User Email", email_or_role, 12, fg("sidebar-text-muted"), lh=16)
    right = (row("Footer Actions", row("Settings Button", ic("gear", 16, fg("sidebar-text-muted"), "Settings Icon"), justify_content="center", width="28px", height="28px") +
                 row("Sign Out Button", ic("sign-out", 16, fg("sidebar-text-muted"), "Sign Out Icon"), justify_content="center", width="28px", height="28px")) if admin else ic("caret-down", 14, fg("sidebar-text-muted"), "Chevron Icon"))
    return row("User Footer", row("User Avatar", T("User Initials", ini, 13, fg("text-inverse"), 600, lh=16), justify_content="center", width="34px", height="34px", border_radius="99px", background=fg("brand-primary")) +
               col("User Meta", meta, flex_grow="1", gap="0px") + right, gap="8px", padding="var(--spacing-md)", border="1px solid var(--color-sidebar-border)")


def admin_sidebar(name, active=None):
    groups = ""
    for g, items in NAV.items():
        groups += col(f"Group / {g}", T(f"Group Label / {g}", g.upper(), 12, fg("sidebar-text-faint"), 600, lh=16, letter_spacing="0.72px", padding="0 12px 8px") +
                      "".join(nav_item(l, i, l == active) for l, i in items), gap="4px", padding="0 0 16px")
    return col(name, brand(T("Brand Mark Letter", "H", 14, fg("text-inverse"), 700, lh=18), "Admin workspace") + col("Nav", groups, gap="0px", padding="4px 16px 16px", flex_grow="1") + user_footer("Admin", "admin@test.local", "ADMIN"),
               width="248px", height="900px", gap="0px", background=fg("sidebar-bg"), border="1px solid var(--color-sidebar-border)")


reg("App shell", "Admin Sidebar", admin_sidebar("Admin Sidebar"), "248x900 dark nav; no active item")
for g, items in NAV.items():
    for l, _ in items:
        reg("App shell", f"Admin Sidebar / {l} Active", admin_sidebar(f"Admin Sidebar / {l} Active", l), f"{l} highlighted")


def cand_sidebar(name, active=None):
    its = [("Home", "house"), ("My interview", "code")]
    return col(name, brand(ic("command", 16, fg("text-inverse"), "Brand Mark Icon"), "Candidate") +
               col("Nav", "".join(nav_item(l, i, l == active) for l, i in its), gap="4px", padding="8px 16px", flex_grow="1") + user_footer("Smoke Candidate", "Candidate", admin=False),
               width="248px", height="900px", gap="0px", background=fg("sidebar-bg"), border="1px solid var(--color-sidebar-border)")


reg("App shell", "Candidate Sidebar", cand_sidebar("Candidate Sidebar"), "248x900; no active item")
reg("App shell", "Candidate Sidebar / Home Active", cand_sidebar("Candidate Sidebar / Home Active", "Home"), "Home highlighted")
reg("App shell", "Candidate Sidebar / My interview Active", cand_sidebar("Candidate Sidebar / My interview Active", "My interview"), "My interview highlighted")
