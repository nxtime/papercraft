"""Admin app shell + Auth, Dashboard, Configuration, Criteria and Schedule screens (page `Admin`).

Idempotent: deletes its own `Section / Admin / <X>` frames and its own main components, then rebuilds.
    python3 admin_core.py [auth dashboard configuration criteria schedule]   (default: all)
"""
import json
import math
import sys

sys.path.insert(0, "/Volumes/Storage/Projects/papercraft/examples/interview")
from ds_common import *  # noqa: F401,F403,E402

OUT = "/private/tmp/claude-501/-Volumes-Storage-Projects-photocraft/11f4040a-b4f4-4b9e-965a-bde08b400450/scratchpad/"
SECTIONS = {k: f"Section / Admin / {k.title()}" for k in ["auth", "dashboard", "configuration", "criteria", "schedule"]}
LIB_Y = 1400  # my new main components live here (above my sections)


def fg(c):
    return f"var(--color-{c})"


def I(comp, name=None, style="", **ov):
    """Instance html with overrides and optional root style."""
    o = json.dumps(ov, ensure_ascii=False).replace("'", "&#39;")
    n = f' layer-name="{name}"' if name else ""
    s = f' style="{style}"' if style else ""
    return f"<x-paper-instance component=\"{comp}\"{n}{s} overrides='{o}'/>"


def card(name, kids, w=None, pad="var(--spacing-lg)", gap="16px", **st):
    if w:
        st["width"] = f"{w}px"
    return col(name, kids, padding=pad, gap=gap, background=fg("bg-elevated"), border="1px solid var(--color-border-light)",
               border_radius="var(--radius-lg)", box_shadow="var(--shadow-sm)", **st)


def div(name="Divider"):
    return row(name, "", height="1px", background=fg("border-light"), align_self="stretch")


def h5(t, name="Card Title"):
    return T(name, t, 16, fg("text-primary"), 600, lh=24)


def sec(t, name="Description", mb=None):
    return T(name, t, 14, fg("text-secondary"))


# ------------------------------------------------------------------ shell
def screen(name, route, active, title, sub, content, wide=False, actions="", overlay="", centered=True):
    """Screen / <name> 1440x900: sidebar instance + header instance + content area."""
    header = I("Admin Shell Header", "Admin Shell Header", "width:1192px", Title=title, Subtitle=sub)
    if actions:
        header = col("Header Wrap", header + row("Header Actions", actions, position="absolute", right="72px", top="34px"),
                     position="relative", width="1192px", height="102px")
    inner_w = "1100px" if wide else "900px"
    inner = col("Content Inner", content, width=inner_w, gap="0px")
    body = col("Content", inner, align_items="center", padding="var(--page-padding)", flex_grow="1", overflow="hidden", background=fg("bg-layout"), width="1192px")
    main = col("Main", header + body, width="1192px", height="900px", background=fg("bg-layout"), overflow="hidden")
    kids = I(f"Admin Sidebar / {active} Active", "Admin Sidebar", "") + main
    if overlay:
        kids += row("Overlay", overlay, position="absolute", left="0px", top="0px", width="1440px", height="900px",
                    background=fg("bg-overlay"), justify_content="center", align_items="center")
    frame = row(f"Screen / {name}", kids, width="1440px", height="900px", background=fg("bg-layout"), overflow="hidden", position="relative", align_items="stretch")
    return col(f"Screen Wrap / {name}", T("Route Caption", route, 12, fg("text-secondary"), lh=16) + frame, gap="8px")


def raw_screen(name, route, content, bg="var(--color-bg-layout)"):
    frame = col(f"Screen / {name}", content, width="1440px", height="900px", background=bg, overflow="hidden", position="relative")
    return col(f"Screen Wrap / {name}", T("Route Caption", route, 12, fg("text-secondary"), lh=16) + frame, gap="8px")


def section(name, title, screens):
    return row(name, col("Section Body", T("Section Title", title, 24, fg("text-primary"), 600, lh=32) + row("Screens", "".join(screens), gap="var(--spacing-xl)", align_items="flex-start"),
                         gap="var(--spacing-xl)"), padding="var(--spacing-xl)", background="transparent", width="fit-content")


# ------------------------------------------------------------------ new main components (to promote)
def donut_svg(segs, size=132, th=16):
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
    center = col("Center", T("Total", "11", 26, fg("text-primary"), 700, lh=32) + T("Caption", "sessions", 12, fg("text-secondary"), lh=16), align_items="center",
                 justify_content="center", position="absolute", left="0px", top="0px", width=f"{size}px", height=f"{size}px")
    return ring + center


def avatar(initials, size, color, name="Avatar"):
    fs = {30: 12, 36: 14}.get(size, 12)
    return I(f"Avatar / {size}", name, f"background-color:{color}", Initials=initials)


def comp_specs():
    """[(name, html, width)] of new main components."""
    L = []
    L.append(("Funnel Row", row("Funnel Row", T("Label", "Applied", 12, fg("text-secondary"), lh=16, width="90px", text_align="right") +
              row("Track", row("Fill", T("Count", "28", 12, fg("text-primary"), 600, lh=16), width="362px", height="26px", padding="0 10px", background="#bae0ff", border_radius="var(--radius-sm)"),
                  flex_grow="1", height="26px", background=fg("bg-layout"), border_radius="var(--radius-sm)", overflow="hidden"), gap="12px", width="464px")))
    L.append(("Funnel Row / Inverse", row("Funnel Row / Inverse", T("Label", "Onsite", 12, fg("text-secondary"), lh=16, width="90px", text_align="right") +
              row("Track", row("Fill", T("Count", "9", 12, fg("text-inverse"), 600, lh=16), width="116px", height="26px", padding="0 10px", background="#4096ff", border_radius="var(--radius-sm)"),
                  flex_grow="1", height="26px", background=fg("bg-layout"), border_radius="var(--radius-sm)", overflow="hidden"), gap="12px", width="464px")))
    L.append(("Challenge Usage Row", col("Challenge Usage Row", row("Head", T("Title", "Two-sum with streaming input", 14, fg("text-primary"), 500, flex_grow="1") +
              T("Count", "18", 12, fg("text-secondary"), 600, lh=16), justify_content="space-between", gap="8px") +
              row("Track", row("Fill", "", width="196px", height="5px", background=fg("brand-primary"), border_radius="99px"), width="196px", height="5px", background=fg("border-light"), border_radius="99px", overflow="hidden"),
              gap="6px", width="196px")))
    L.append(("Review Item", row("Review Item", I("Avatar / 30", "Avatar", "", Initials="PI") +
              col("Texts", T("Name", "Priya Iyer", 14, fg("text-primary"), 600) + T("Meta", "Live coding · 45m", 12, fg("text-secondary"), lh=16), flex_grow="1") +
              I("Button / Primary Small", "Score Button", "", Label="Score"), gap="12px", padding="8px 12px", width="316px", background=fg("bg-elevated"),
              border="1px solid var(--color-border-light)", border_radius="var(--radius-md)")))
    for st, tone in [("Scheduled", "Blue"), ("In Progress", "Gold")]:
        L.append((f"Upcoming Row / {st}", row(f"Upcoming Row / {st}", box("Accent Bar", "", width="4px", height="36px", border_radius="var(--radius-sm)", background=fg("brand-primary")) +
                  I("Avatar / 36", "Avatar", "", Initials="SA") +
                  col("Texts", T("Name", "Sara Ahmed", 14, fg("text-primary"), 600) + T("Meta", "Live coding · 45m", 12, fg("text-secondary"), lh=16), flex_grow="1") +
                  col("When Col", T("When", "Today · 14:00", 12, fg("text-secondary"), lh=16, text_align="right") + I(f"Tag Dot / {tone}", "Status Tag", "", Label=st), align_items="flex-end", gap="4px"),
                  gap="12px", padding="12px 0px", width="316px")))
    L.append(("Donut / Score Distribution", box("Donut / Score Distribution", donut_svg([(3, "#52c41a"), (6, "#1677ff"), (2, "#faad14")]), position="relative", display="flex", width="132px", height="132px")))
    L.append(("Legend Row", row("Legend Row", box("Swatch", "", width="10px", height="10px", border_radius="3px", background=fg("success")) +
              T("Label", "Strong (4.5+)", 14, fg("text-secondary"), flex_grow="1") + T("Value", "3", 14, fg("text-primary"), 700), gap="8px", width="196px")))
    L.append(("Criterion Row / Weight", row("Criterion Row / Weight", T("Label", "Problem solving", 14, fg("text-primary"), 500, flex_grow="1") +
              row("Weight Input", T("Value", "30", 14, fg("text-primary"), flex_grow="1") + T("Suffix", "%", 14, fg("text-secondary")), width="80px", height="24px", padding="0 7px", gap="4px",
                  background=fg("bg-elevated"), border="1px solid var(--color-border)", border_radius="var(--radius-md)") +
              I("Ant Button / Text Danger Small", "Remove Button", "", Label="×"), gap="16px", padding="8px 16px", width="512px", background=fg("bg-layout"), border_radius="var(--radius-md)")))
    for on in (True, False):
        nm = f"Setting Row / {'On' if on else 'Off'}"
        L.append((nm, col(nm, row("Row", col("Texts", T("Label", "Session created", 14, fg("text-primary"), 500) + T("Desc", "When a new interview session is scheduled", 12, fg("text-secondary"), lh=16), gap="2px", flex_grow="1") +
                  I("Switch / On" if on else "Switch / Off", "Switch", ""), gap="16px", padding="16px 0px", width="512px"))))
    for av in (True, False):
        nm = f"ATS Tile / {'Available' if av else 'Soon'}"
        L.append((nm, row(nm, T("Name", "Greenhouse", 14, fg("text-primary"), 600, flex_grow="1") +
                  (I("Ant Button / Default Small", "Connect Button", "", Label="Connect") if av else I("Ant Tag / Default", "Soon Tag", "", Label="Soon")),
                  justify_content="space-between", padding="16px", width="248px", border="1px solid var(--color-border-light)", border_radius="var(--radius-md)")))
    L.append(("Pipeline Row / Dark", row("Pipeline Row / Dark", row("Who", box("Dot", "", width="8px", height="8px", border_radius="99px", background="#4ade80") + T("Name", "Amara Okafor", 14, fg("sidebar-text")), gap="8px") +
              T("Stage", "Onsite loop", 12, fg("sidebar-text-faint"), lh=16), justify_content="space-between", padding="8px 0px", width="100%")))
    return L


def build_components(P):
    names = [n for n, _ in comp_specs()]
    root = P.c("get_children", nodeId="root")["children"]
    ids = [c["id"] for c in root if c["name"] in names]
    if ids:
        P.c("delete_nodes", nodeIds=ids)
    x = 0
    for n, html in comp_specs():
        r = P.c("write_html", targetNodeId="root", mode="insert-children", html=html)
        for k in ("warnings", "ignoredStyles"):
            if r.get(k):
                print("  ", n, k, r[k][:3] if isinstance(r[k], list) else r[k])
        nid = r["createdNodeIds"][0]
        P.c("create_component", nodeId=nid, name=n)
        P.c("update_styles", updates=[{"nodeIds": [nid], "styles": {"left": x, "top": LIB_Y}}])
        x += 360


# ------------------------------------------------------------------ placement
def place(P, name, html, top):
    root = P.c("get_children", nodeId="root")["children"]
    old = [c["id"] for c in root if c["name"] == name]
    if old:
        P.c("delete_nodes", nodeIds=old)
    r = P.add("root", html)
    P.c("update_styles", updates=[{"nodeIds": [r[0]], "styles": {"left": 0, "top": top}}])
    info = P.c("get_node_info", nodeId=r[0])
    return r[0], top + info.get("height", 1200) + 200


def start_top(P):
    root = P.c("get_children", nodeId="root")["children"]
    mine = set(SECTIONS.values())
    tops = [c.get("y", 0) + c["height"] for c in root if c["name"].startswith("Section / ") and c["name"] not in mine and c.get("x", 0) < 20000]
    return max(tops + [2400]) + 200


# ------------------------------------------------------------------ AUTH
def brand_mark():
    return row("Brand", box("Brand Mark", T("Brand Mark Letter", "H", 14, fg("text-inverse"), 700, lh=18), display="flex", justify_content="center", align_items="center",
                            width="28px", height="28px", border_radius="var(--radius-md)", background=fg("brand-primary")) +
               T("Brand Name", "Hireloop", 16, fg("text-inverse"), 700, lh=20), gap="10px")


def pipeline_card(w=None):
    rows = "".join(I("Pipeline Row / Dark", f"Pipeline Row / {n}", "width:100%", Name=n, Stage=s) for n, s in
                   [("Amara Okafor", "Onsite loop"), ("Mei Lin Tan", "Debrief"), ("Sofia Bianchi", "Offer")])
    head = row("Pipeline Header", T("Pipeline Title", "CANDIDATE PIPELINE", 12, fg("sidebar-text-faint"), 700, lh=16, letter_spacing="0.84px") +
               T("Live Pill", "Live", 12, "#4ade80", 600, lh=16, padding="2px 8px", background="#16a34a2e", border_radius="99px"), justify_content="space-between")
    return col("Pipeline Card", head + rows, gap="8px", padding="16px", background="#ffffff0d", border="1px solid #ffffff14", border_radius="10px", **({"width": f"{w}px"} if w else {}))


FEATURES = [("squares-four", "Team-wide visibility", "Track every candidate across the loop, from screen to offer."),
            ("brackets-curly", "Live coding & system design", "Watch candidates work in a real shared editor and whiteboard."),
            ("diamond", "Calibrated scorecards", "A shared rubric keeps every interviewer's ratings comparable.")]


def login_screen(error=False):
    feats = "".join(row(f"Feature / {t}", box("Feature Icon", ic(i, 16, fg("sidebar-text")), display="flex", justify_content="center", align_items="center", width="32px", height="32px",
                                              border_radius="6px", background="#ffffff12") +
                        col("Texts", T("Feature Title", t, 14, fg("text-inverse"), 600, lh=18) + T("Feature Desc", d, 12, fg("sidebar-text-muted"), lh=18), gap="2px", flex_grow="1"),
                        gap="16px", align_items="flex-start") for i, t, d in FEATURES)
    hero = col("Hero", T("Hero Title", "Run better interviews, together.", 32, fg("text-inverse"), 800, lh=38, letter_spacing="-0.6px") +
               T("Hero Sub", "Structured scorecards, live coding sessions, and calibrated hiring decisions — all in one workspace for your team.", 14, fg("sidebar-text-muted"), lh=24), gap="16px")
    mid = col("Marketing Body", hero + col("Features", feats, gap="20px") + pipeline_card(), gap="32px", flex_grow="1", justify_content="center", padding="56px 0px 0px")
    left = col("Marketing Panel", brand_mark() + mid + T("Footer Text", "© 2026 Hireloop. All rights reserved.", 12, "#ffffff38", lh=16), padding="32px", width="662px", height="900px", background="#0f1117")
    alert = row("Alert / Error", ic("x-circle", 16, fg("score-poor"), "Alert Icon") + T("Message", "Invalid credentials", 14, fg("text-primary"), flex_grow="1"), gap="8px", width="400px",
                padding="8px 12px", background=fg("tag-red-bg"), border="1px solid var(--color-tag-red-bd)", border_radius="var(--radius-md)") if error else ""
    pw_label = row("Password Label", T("Label", "Password", 14, fg("text-primary"), lh=22) + T("Forgot", "Forgot password?", 13, fg("text-link"), lh=18), justify_content="space-between", width="400px")
    form = col("Sign In Form", col("Heading", T("Title", "Sign in to your workspace", 30, fg("text-primary"), 700, lh=38, letter_spacing="-0.3px") +
                                   T("Subtitle", "Enter your credentials to continue to Hireloop.", 14, fg("text-secondary")), gap="4px") + alert +
               col("Fields", I("Form Field / Required", "Form Field / Work email", "width:400px", Label="Work email", Value="you@company.com") +
                   col("Form Field / Password", pw_label + I("Input / Password", "Input", "width:400px"), gap="8px"), gap="24px") +
               I("Checkbox / Unchecked", "Remember", "", Label="Keep me signed in") +
               I("Ant Button / Primary Large", "Sign In Button", "width:400px", Label="Sign in") +
               row("Links", T("Request", "Don't have access? ", 13, fg("text-secondary"), lh=18) + T("Invite Link", "Ask your workspace admin for an invite", 13, fg("text-link"), lh=18), justify_content="center", width="400px"),
               gap="24px", width="400px")
    right = col("Form Panel", form, flex_grow="1", justify_content="center", align_items="center", background=fg("bg-elevated"), padding="48px", height="900px")
    return row("Login Split", left + right, width="1440px", height="900px")


def landing_screen():
    nav = row("Nav", brand_mark() + I("Ant Button / Primary", "Sign In Button", "", Label="Sign in"), justify_content="space-between")
    eyebrow = row("Eyebrow", box("Dot", "", width="6px", height="6px", border_radius="99px", background="#4ade80") +
                  T("Eyebrow Label", "INTERVIEW PLATFORM FOR HIRING TEAMS", 12, fg("sidebar-text"), 700, lh=16, letter_spacing="1.2px"),
                  gap="8px", padding="6px 12px", background="#ffffff0a", border="1px solid #ffffff14", border_radius="99px", align_self="flex-start")
    title = row("Hero Title", T("Title Plain", "Run better interviews, ", 44, fg("text-inverse"), 800, lh=48, letter_spacing="-1.3px") +
                T("Title Accent", "together.", 44, fg("brand-secondary"), 800, lh=48, letter_spacing="-1.3px"), gap="0px", flex_wrap="wrap")
    stats = row("Stats", "".join(col(f"Stat / {l}", T("Value", v, 28, fg("text-inverse"), 800, lh=32) + T("Label", l, 12, fg("sidebar-text-muted"), lh=16), gap="2px")
                                 for v, l in [("4.1", "Avg. rubric score"), ("34%", "Offer rate"), ("12", "Active candidates")]), gap="36px")
    hero = col("Hero", eyebrow + col("Title Block", title, width="580px") +
               T("Hero Sub", "Structured scorecards, live coding sessions, and calibrated hiring decisions — all in one workspace for your team.", 16, fg("sidebar-text-muted"), lh=26, width="540px") +
               row("CTA Row", I("Ant Button / Primary Large", "Sign In Button", "", Label="Sign in to your workspace") +
                   T("Hint", "No access yet? Ask your workspace admin.", 13, fg("sidebar-text-muted"), lh=18), gap="16px") + stats,
               gap="20px", width="620px")
    hero_row = row("Hero Row", hero + pipeline_card(320), gap="48px", flex_grow="1", align_items="center")
    grid = row("Feature Grid", "".join(col(f"Feature / {t}", box("Feature Icon", ic(i, 20, fg("brand-secondary")), display="flex", justify_content="center", align_items="center",
                                                                width="36px", height="36px", border_radius="var(--radius-md)", background="#69b1ff1f") +
                                         T("Feature Title", t, 14, fg("text-inverse"), 600, lh=20) + T("Feature Desc", d, 14, fg("sidebar-text-muted"), lh=22), gap="8px", padding="20px", flex_grow="1",
                                         background="#ffffff0a", border="1px solid #ffffff12", border_radius="var(--radius-lg)") for i, t, d in FEATURES), gap="16px")
    foot = row("Footer", T("Left", "© 2026 Hireloop. All rights reserved.", 12, "#ffffff40", lh=16) + T("Right", "Built for structured, calibrated hiring.", 12, "#ffffff40", lh=16),
               justify_content="space-between", padding="16px 0px 0px", border="1px solid transparent")
    glow = box("Glow", "", position="absolute", left="340px", top="-160px", width="760px", height="560px", background="radial-gradient(circle, #1677ff47 0%, #1677ff14 40%, transparent 70%)")
    inner = col("Landing Inner", nav + hero_row + grid + foot, width="1080px", padding="28px 40px", gap="24px", height="900px", position="absolute", left="180px", top="0px")
    return box("Landing", glow + inner, position="relative", width="1440px", height="900px", background="#0f1117", overflow="hidden")


def build_auth():
    return section(SECTIONS["auth"], "Admin / Auth", [
        raw_screen("Admin Login", "/login", login_screen()),
        raw_screen("Admin Login Error", "/login (invalid credentials)", login_screen(True)),
        raw_screen("Admin Landing", "/", landing_screen(), bg="#0f1117"),
    ])


# ------------------------------------------------------------------ DASHBOARD
def stats_row(vals):
    comps = ["Blue", "Purple", "Orange Down", "Green", "Teal"]
    labels = ["ACTIVE CANDIDATES", "INTERVIEWS SCHEDULED", "AWAITING REVIEW", "AVG. SCORE", "OFFER RATE"]
    out = ""
    for c, l, (v, d, t) in zip(comps, labels, vals):
        arrow = "▼" if c == "Orange Down" else "▲"
        out += I(f"Stat / {c}", f"Stat / {l.title()}", "width:167px", Label=l, Value=v, Delta=d, Trend=t, **{"Delta Arrow": arrow})
    return row("Stats Row", out, gap="var(--spacing-md)", align_items="stretch", width="900px")


FUNNEL = [("Applied", 28, "#bae0ff"), ("Screen", 18, "#91caff"), ("Phone", 14, "#69b1ff"), ("Onsite", 9, "var(--color-button-primary-hover)"), ("Offer", 5, "var(--color-brand-primary)"), ("Hired", 3, "#0958d9")]
TOP = [("Two-sum with streaming input", 18, "var(--color-brand-primary)"), ("REST API: pagination + caching", 14, "var(--color-avatar-purple)"),
       ("SQL: window functions deep-dive", 11, "var(--color-avatar-teal)"), ("Build a Markdown previewer", 8, "var(--color-warning)"), ("Debug a memory leak", 5, "var(--color-success)")]


def funnel_card(empty=False):
    rows = ""
    for i, (s, n, c) in enumerate(FUNNEL):
        nn = 0 if empty else n
        w = max(0.06, nn / 28) * 362
        rows += I("Funnel Row" if i < 3 else "Funnel Row / Inverse", f"Funnel Row / {s}", "", Label=s, Count=str(nn), Fill={"width": f"{w:.0f}px", "backgroundColor": c})
    return card("Card / Hiring pipeline", I("Section Title", "Section Title", "width:464px", Title="Hiring pipeline", Subtitle="Across all open roles") +
                col("Funnel", rows, gap="10px"), w=512, gap="16px")


def score_card(empty=False):
    if empty:
        body = I("Empty / Simple", "Empty", "width:196px", Description="No completed sessions")
    else:
        body = col("Donut Block", row("Donut Wrap", I("Donut / Score Distribution", "Donut", ""), justify_content="center", width="196px") +
                   col("Legend", I("Legend Row", "Legend / Strong", "", Label="Strong (4.5+)", Value="3") +
                       I("Legend Row", "Legend / Solid", "", Label="Solid (3.5–4.5)", Value="6", Swatch={"backgroundColor": fg("brand-primary")}) +
                       I("Legend Row", "Legend / Borderline", "", Label="Borderline (<3.5)", Value="2", Swatch={"backgroundColor": fg("score-warn")}), gap="12px"), gap="16px")
    return card("Card / Score distribution", I("Section Title", "Section Title", "width:196px", Title="Score distribution", Subtitle="Completed sessions") + body, w=244)


def top_card(empty=False):
    if empty:
        body = I("Empty / Simple", "Empty", "width:196px", Description="No challenge usage yet")
    else:
        body = col("Usage List", "".join(I("Challenge Usage Row", f"Usage / {t}", "", Title=t, Count=str(n), Fill={"width": f"{n / 18 * 196:.0f}px", "backgroundColor": c}) for t, n, c in TOP), gap="12px")
    return card("Card / Most-used challenges", I("Section Title / With Action", "Section Title", "width:196px", Title="Most-used challenges", Action="Library →", Subtitle={"display": "none"}) + body, w=244)


def review_card(empty=False):
    head = row("Review Head", col("Texts", T("Title", "Awaiting review", 16, fg("text-primary"), 600, lh=22) +
                                  T("Subtitle", "0 sessions need your scorecard" if empty else "2 sessions need your scorecard", 12, fg("text-secondary"), lh=16)) +
               ("" if empty else I("Tag Dot / Gold", "Action Tag", "", Label="Action needed")), justify_content="space-between", gap="8px")
    if empty:
        items = I("Empty State", "Empty State", "width:316px", Title="All caught up", Description="No sessions are waiting for your scorecard.")
    else:
        items = col("Review Items", I("Review Item", "Review Item / Priya Iyer", "", Name="Priya Iyer", Meta="Live coding · 45m", Avatar={"backgroundColor": fg("avatar-orange")}, Initials="PI") +
                    I("Review Item", "Review Item / Marcus Chen", "", Name="Marcus Chen", Meta="Fullstack · 60m", Avatar={"backgroundColor": fg("warning")}, Initials="MC"), gap="8px")
    return card("Card / Awaiting review", head + items, w=364, background="linear-gradient(180deg, #fffdf5, #ffffff)")


UPC = [("Sara Ahmed", "SA", "Live coding", 45, "Today · 14:00", "Scheduled", "brand-primary", "avatar-teal"),
       ("Tomás López", "TL", "Fullstack", 60, "Today · 16:30", "Scheduled", "avatar-purple", "avatar-teal"),
       ("Aisha Khan", "AK", "SQL", 30, "Tomorrow · 10:00", "Scheduled", "avatar-teal", "avatar-pink"),
       ("Owen Park", "OP", "Live coding", 45, "Thu · 11:00", "In Progress", "brand-primary", "avatar-indigo")]


def upcoming_card(empty=False):
    head = I("Section Title", "Section Title", "width:316px", Title="Upcoming interviews", Subtitle={"display": "none"})
    if empty:
        return card("Card / Upcoming interviews", head + I("Empty / Simple", "Empty", "width:316px", Description="No interviews scheduled"), w=364)
    rows = ""
    for i, (n, ini, t, m, when, st, acc, avc) in enumerate(UPC):
        if i:
            rows += div()
        rows += I(f"Upcoming Row / {st}", f"Upcoming Row / {n}", "", Name=n, Initials=ini, Meta=f"{t} · {m}m", When=when, **{"Accent Bar": {"backgroundColor": fg(acc)}, "Avatar": {"backgroundColor": fg(avc)}})
    return card("Card / Upcoming interviews", head + col("Upcoming List", rows, gap="0px"), w=364, gap="8px")


def dashboard_content(empty=False):
    st = stats_row([("0", "0", "this week")] * 5 if empty else [("12", "+3", "this week"), ("8", "+2", "vs last wk"), ("2", "−1", "cleared"), ("4.1", "+0.2", "this month"), ("34%", "+5%", "this quarter")])
    left = col("Left Column", funnel_card(empty) + row("Pair", score_card(empty) + top_card(empty), gap="var(--spacing-lg)", align_items="flex-start"), gap="var(--spacing-lg)")
    right = col("Right Column", review_card(empty) + upcoming_card(empty), gap="var(--spacing-lg)")
    return col("Dashboard", st + row("Dashboard Grid", left + right, gap="var(--spacing-lg)", align_items="flex-start"), gap="var(--spacing-lg)")


def build_dashboard():
    return section(SECTIONS["dashboard"], "Admin / Dashboard", [
        screen("Admin Dashboard", "/dashboard", "Dashboard", "Good afternoon", "Welcome to your dashboard", dashboard_content()),
        screen("Admin Dashboard Empty", "/dashboard (no data)", "Dashboard", "Good afternoon", "Welcome to your dashboard", dashboard_content(True)),
    ])


# ------------------------------------------------------------------ CONFIGURATION
def field(label, control, w=512, required=False):
    lab = (T("Required Mark", "*", 14, fg("error"), lh=22) if required else "") + T("Label", label, 14, fg("text-primary"), lh=22)
    return col(f"Field / {label}", row("Label Row", lab, gap="4px") + control, gap="8px", width=f"{w}px")


def cfg_screen(tab_idx, name, route, content):
    tabs = I(f"Tabs / Bar 5 Large Active {tab_idx}", "Tabs", "width:1100px", **{"Tab 1 Label": "Workspace", "Tab 2 Label": "Team", "Tab 3 Label": "Scoring", "Tab 4 Label": "Notifications", "Tab 5 Label": "Integrations"})
    return screen(name, route, "Configuration", "Configuration", "Workspace settings, team, scoring & integrations", col("Config", tabs + col("Tab Body", content, padding="16px 0px 0px"), gap="0px"), wide=True)


def tab_workspace():
    general = card("Card / General", h5("General") + col("Form", field("Workspace name", I("Input / Large Filled", "Input", "width:512px", Value="Netcracker"), required=True) +
                   field("Workspace URL", I("Input / With Prefix", "Input", "width:512px", Prefix="hireloop.io/", Value={"color": fg("text-primary")})) +
                   field("Default timezone", I("Select / Large Filled", "Select", "width:512px", Value="UTC+3 (Moscow)")) +
                   field("Description", I("Textarea", "Textarea", "width:512px", Value="Brief description of your team or hiring process")), gap="24px") +
                   I("Ant Button / Primary", "Save Button", "", Label="Save changes"), w=560)
    danger = card("Card / Danger zone", col("Texts", h5("Danger zone") + T("Description", "Irreversible actions. Proceed with caution.", 14, fg("text-secondary")), gap="4px") +
                  I("Ant Button / Danger", "Delete Button", "", Label="Delete workspace"), w=560)
    return col("Workspace Tab", general + danger, gap="20px")


MEMBERS = [("Elena Vasquez", "elena.vasquez@netcracker.com", "Admin", "Active"), ("Dmitri Sokolov", "d.sokolov@netcracker.com", "Interviewer", "Active"),
           ("Priya Nair", "p.nair@netcracker.com", "Interviewer", "Active"), ("James Obi", "j.obi@netcracker.com", "Viewer", "Pending")]
ROLE_TAG = {"Admin": "Tag / Blue", "Interviewer": "Tag / Purple", "Viewer": "Ant Tag / Default"}


def th_row(cols, widths, name="Header Row"):
    cells = "".join(T(f"Col {i}", c, 14, fg("text-primary"), 600, **({"width": f"{w}px"} if w else {"flex_grow": "1"})) for i, (c, w) in enumerate(zip(cols, widths), 1))
    return row(name, cells, gap="16px", padding="16px", background=fg("bg-sider"), align_self="stretch")


def td_row(name, cells, widths, pad="16px"):
    out = "".join(row(f"Cell {i}", c, **({"width": f"{w}px"} if w else {"flex_grow": "1"})) for i, (c, w) in enumerate(zip(cells, widths), 1))
    return row(name, out, gap="16px", padding=pad, align_self="stretch", background=fg("bg-elevated"))


def tab_team():
    W = [None, 120, 120, 160]
    rows = th_row(["Name", "Role", "Status", ""], W)
    for n, e, r, s in MEMBERS:
        rows += div() + td_row(f"Row / {n}", [col("Name Cell", T("Name", n, 14, fg("text-primary"), 600) + T("Email", e, 12, fg("text-secondary"), lh=16), gap="0px"),
                                              I(ROLE_TAG[r], "Role Tag", "", Label=r), I("Ant Tag / Success" if s == "Active" else "Ant Tag / Default", "Status Tag", "", Label=s),
                                              row("Actions", I("Ant Button / Text Small", "Edit", "", Label="Edit") + I("Ant Button / Text Danger Small", "Remove", "", Label="Remove"), gap="4px", justify_content="flex-end")], W)
    table = col("Table", rows, width="700px", background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)", overflow="hidden", box_shadow="var(--shadow-sm)")
    top = row("Team Head", T("Count", "4 members in this workspace", 14, fg("text-secondary")) + I("Ant Button / Primary", "Invite Button", "", Label="Invite member"), justify_content="space-between", width="700px")
    return col("Team Tab", top + table, gap="20px")


CRIT = [("Problem solving", 30), ("Code quality", 25), ("Communication", 20), ("System design", 15), ("Culture fit", 10)]


def tab_scoring():
    scale = card("Card / Scoring scale", col("Texts", h5("Scoring scale") + T("Description", "All scorecard ratings use this scale.", 14, fg("text-secondary")), gap="4px") +
                 row("Scale Form", row("Min Field", T("Label", "Min:", 14, fg("text-primary")) + I("Input / Large Filled", "Input", "width:72px", Value="1"), gap="8px") +
                     row("Max Field", T("Label", "Max:", 14, fg("text-primary")) + I("Input / Large Filled", "Input", "width:72px", Value="5"), gap="8px") +
                     row("Step Field", T("Label", "Step:", 14, fg("text-primary")) + I("Select / Large Filled", "Select", "width:88px", Value="1"), gap="8px"), gap="16px"), w=600)
    items = "".join(I("Criterion Row / Weight", f"Criterion Row / {l}", "", Label=l, Value=str(w)) for l, w in CRIT)
    rub = card("Card / Rubric criteria", row("Rubric Head", col("Texts", h5("Rubric criteria") + T("Description", "Weights must sum to 100%.", 14, fg("text-secondary")), gap="4px") +
                                            I("Ant Button / Default", "Add Button", "", Label="Add criterion"), justify_content="space-between", align_items="flex-start") +
               col("Criteria", items, gap="8px") + I("Ant Button / Primary", "Save Button", "", Label="Save rubric"), w=600)
    return col("Scoring Tab", scale + rub, gap="20px")


NOTIFS = [("Session created", "When a new interview session is scheduled", True), ("Candidate joined", "When a candidate enters their session", True),
          ("Review ready", "When all scorecards are submitted for a session", True), ("Invite expired", "When a candidate invite link expires unused", False),
          ("Weekly digest", "Summary of sessions, scores, and pipeline activity", True)]


def tab_notifications():
    rows = ""
    for i, (l, d, on) in enumerate(NOTIFS):
        rows += I(f"Setting Row / {'On' if on else 'Off'}", f"Setting Row / {l}", "", Label=l, Desc=d)
        if i < len(NOTIFS) - 1:
            rows += div()
    c = card("Card / Email notifications", col("Texts", h5("Email notifications") + T("Description", "Sent to your work email. You can override per-session in session settings.", 14, fg("text-secondary")), gap="4px") +
             col("Settings", rows, gap="0px") + I("Ant Button / Primary", "Save Button", "", Label="Save preferences"), w=560)
    return c


def tab_integrations():
    hook = card("Card / Webhook", col("Texts", h5("Webhook") + T("Description", "POST events (session created, review complete) to your endpoint.", 14, fg("text-secondary")), gap="4px") +
                col("Form", field("Endpoint URL", I("Input / Large", "Input", "width:552px", Value="https://your-app.com/webhooks/hireloop"), 552) +
                    field("Signing secret", I("Input / Password", "Input", "width:552px"), 552), gap="24px") + I("Ant Button / Primary", "Save Button", "", Label="Save webhook"), w=600)
    tiles = [("Greenhouse", True), ("Lever", True), ("Workday", False), ("BambooHR", False)]
    t = "".join(I(f"ATS Tile / {'Available' if a else 'Soon'}", f"ATS Tile / {n}", "", Name=n) for n, a in tiles)
    ats = card("Card / ATS integrations", col("Texts", h5("ATS integrations") + T("Description", "Sync candidates and decisions directly with your applicant tracking system.", 14, fg("text-secondary")), gap="4px") +
               row("ATS Grid", t, flex_wrap="wrap", gap="12px", width="552px"), w=600)
    return col("Integrations Tab", hook + ats, gap="20px")


def build_configuration():
    return section(SECTIONS["configuration"], "Admin / Configuration", [
        cfg_screen(1, "Admin Configuration Workspace", "/configuration (Workspace)", tab_workspace()),
        cfg_screen(2, "Admin Configuration Team", "/configuration (Team)", tab_team()),
        cfg_screen(3, "Admin Configuration Scoring", "/configuration (Scoring)", tab_scoring()),
        cfg_screen(4, "Admin Configuration Notifications", "/configuration (Notifications)", tab_notifications()),
        cfg_screen(5, "Admin Configuration Integrations", "/configuration (Integrations)", tab_integrations()),
    ])


# ------------------------------------------------------------------ CRITERIA
CRITERIA = [("Problem solving", 30, 0, False), ("Code quality", 25, 1, False), ("Communication", 20, 2, False), ("System design", 15, 3, False), ("Culture fit", 10, 4, False), ("Whiteboarding", 0, 5, True)]
CW = [None, 100, 100, 120, 260]


def criteria_table(rows_=CRITERIA, state="data"):
    out = th_row(["Label", "Weight", "Position", "Status", "Actions"], CW)
    if state == "empty":
        out += div() + row("Empty Row", I("Empty / Simple", "Empty", "", Description="No data"), justify_content="center", padding="var(--spacing-xl)", align_self="stretch", background=fg("bg-elevated"))
    elif state == "loading":
        out += div() + row("Loading Row", I("Spin / Large", "Spin", ""), justify_content="center", padding="var(--spacing-xxl)", align_self="stretch", background=fg("bg-elevated"))
    for i, (l, w, p, arch) in enumerate(rows_ if state == "data" else []):
        dis = "opacity:.5"
        btns = (I("Ant Button / Default Small", "Edit", "", Label="Edit") + I("Ant Button / Default Small", "Archive", "", Label="Unarchive" if arch else "Archive") +
                I("Ant Button / Default Small", "Up", dis if i == 0 else "", Label="↑") + I("Ant Button / Default Small", "Down", dis if i == len(rows_) - 1 else "", Label="↓"))
        out += div() + td_row(f"Row / {l}", [T("Label", l, 14, fg("text-primary")), T("Weight", f"{w}%", 14, fg("text-primary")), T("Position", str(p), 14, fg("text-primary")),
                                              I("Ant Tag / Default" if arch else "Ant Tag / Success", "Status Tag", "", Label="Archived" if arch else "Active"), row("Actions", btns, gap="8px")], CW)
    return col("Table", out, align_self="stretch", background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-md)", overflow="hidden")


def criteria_screen(name, route, state="data", overlay=""):
    c = card("Card / Criteria", T("Description", "Active criteria are shown in the interview scorecard. Archived criteria are hidden from the scorecard but retained (scores referencing them are never deleted).", 14, fg("text-secondary")) +
             criteria_table(state=state), w=1100)
    return screen(name, route, "Rubric", "Rubric", "Manage scorecard criteria — create, edit, archive, reorder", c, wide=True,
                  actions=I("Ant Button / Primary", "Add Button", "", Label="Add criterion"), overlay=overlay)


def modal_form(title, ok, vals):
    def f(label, value, ph=False):
        ctl = I("Input" if ph else "Input / Filled", "Input", "width:472px", Value=value)
        return col(f"Field / {label}", row("Label Row", T("Required Mark", "*", 14, fg("error"), lh=22) + T("Label", label, 14, fg("text-primary"), lh=22), gap="4px") + ctl, gap="8px")
    fields = col("Form", f("Label", vals[0], vals[0].startswith("e.g.")) + f("Weight (%)", vals[1]) + f("Position", vals[2]), gap="24px")
    header = row("Header", T("Title", title, 16, fg("text-primary"), 600, lh=24) + ic("x", 14, fg("text-secondary"), "Close Icon"), justify_content="space-between")
    footer = row("Footer", I("Ant Button / Default", "Cancel Button", "", Label="Cancel") + I("Ant Button / Primary", "OK Button", "", Label=ok), justify_content="flex-end", gap="8px")
    return col("Modal", header + fields + footer, width="520px", gap="16px", padding="24px", background=fg("bg-elevated"), border_radius="var(--radius-lg)", box_shadow="var(--shadow-lg)")


def build_criteria():
    return section(SECTIONS["criteria"], "Admin / Criteria", [
        criteria_screen("Admin Criteria", "/criteria"),
        criteria_screen("Admin Criteria Empty", "/criteria (no criteria)", "empty"),
        criteria_screen("Admin Criteria Loading", "/criteria (loading)", "loading"),
        criteria_screen("Admin Criteria Add Modal", "/criteria (Add criterion modal)", overlay=modal_form("Add criterion", "Create", ("e.g. Problem solving", "0", "6"))),
        criteria_screen("Admin Criteria Edit Modal", "/criteria (Edit criterion modal)", overlay=modal_form("Edit criterion", "Save", ("Code quality", "25", "1"))),
    ])


# ------------------------------------------------------------------ SCHEDULE
SESS = [("Backend Engineer · Sara Ahmed", "K7M2QX", "Open", "Blue", "2026-07-14 10:00:00"), ("Fullstack · Tomás López", "R4TP9D", "In progress", "Gold", "2026-07-14 14:30:00"),
        ("SQL deep-dive · Aisha Khan", "B8WN3C", "Submitted", "Purple", "2026-07-15 09:00:00"), ("Live coding · Owen Park", "H2VZ6L", "Graded", "Green", "2026-07-16 11:00:00"),
        ("Frontend Engineer · Mei Lin Tan", "Q5XJ1S", "Draft", "Gray", None), ("Platform · Dmitri Sokolov", "M9AE7F", "Cancelled", "Red", None), ("Data Engineer · Priya Iyer", "T3GY8U", "Open", "Blue", None)]
SW = [None, 130, 140, 300]


def schedule_table():
    head = row("Header Row", "".join(T(f"Col {i}", c, 12, fg("text-secondary"), 600, lh=16, letter_spacing="0.36px", **({"width": f"{w}px"} if w else {"flex_grow": "1"}))
                                    for i, (c, w) in enumerate(zip(["SESSION", "CODE", "STATUS", "SCHEDULED START"], SW), 1)), gap="16px", padding="12px 16px", background=fg("bg-sider"), align_self="stretch")
    out = head
    for t, code, st, tone, ts in SESS:
        chip = row("Code Chip", T("Code", code, 12, fg("text-secondary"), lh=16), padding="2px 8px", background=fg("bg-sider"), border="1px solid var(--color-border-light)", border_radius="var(--radius-sm)")
        sched = row("Schedule Cell", I("Date Picker", "Date Picker", "width:200px", Value={"color": fg("text-primary" if ts else "text-placeholder")} if False else (ts or "Select date")) +
                    (I("Button / Ghost Small", "Clear Button", "", Label="Clear") if ts else ""), gap="8px")
        out += div() + td_row(f"Row / {code}", [T("Title", t, 14, fg("text-primary"), 600), chip, I(f"Tag Dot / {tone}", "Status Tag", "", Label=st), sched], SW, pad="12px 16px")
    return col("Data Table", out, align_self="stretch", background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)", overflow="hidden", box_shadow="var(--shadow-sm)")


def schedule_screen(name, route, state="data"):
    search = row("Toolbar", I("Search Input", "Search Input", "width:240px", Placeholder="Search by title or code…"), justify_content="flex-end", align_self="stretch", padding="0px 0px 16px")
    if state == "data":
        body = schedule_table()
    elif state == "empty":
        body = I("Empty State", "Empty State", "width:1100px", Title="Nothing to schedule", Description="Sessions you create will appear here so you can plan when they run.")
    else:
        body = row("Loading", I("Spin / Large", "Spin", ""), justify_content="center", padding="var(--spacing-xxl)", align_self="stretch")
    return screen(name, route, "Schedule", "Schedule", "Plan when sessions run", col("Schedule", search + body, gap="0px"), wide=True)


def build_schedule():
    return section(SECTIONS["schedule"], "Admin / Schedule", [
        schedule_screen("Admin Schedule", "/schedule"),
        schedule_screen("Admin Schedule Empty", "/schedule (no sessions)", "empty"),
        schedule_screen("Admin Schedule Loading", "/schedule (loading)", "loading"),
    ])


BUILDERS = {"auth": build_auth, "dashboard": build_dashboard, "configuration": build_configuration, "criteria": build_criteria, "schedule": build_schedule}


def main():
    fid = next(i["id"] for i in call("list_resources")["items"] if i["name"] == PROJECT)
    P = pages(fid)["Admin"]
    call("import_components", {"fromPageId": pages(fid)["Design System"].id, "pageId": P.id})
    want = sys.argv[1:] or list(BUILDERS)
    # remove my old sections first (instances of my own components go with them), then rebuild components
    root = P.c("get_children", nodeId="root")["children"]
    old = [c["id"] for c in root if c["name"] in SECTIONS.values() and any(SECTIONS[k] == c["name"] for k in (want if want != ['comps'] else []))]
    if old:
        P.c("delete_nodes", nodeIds=old)
    if "comps" in want or len(sys.argv) == 1 or not [c for c in comp_specs() if c[0] in {x["name"] for x in root}]:
        build_components(P)
    top = start_top(P)
    for k in [w for w in want if w in BUILDERS]:
        html = BUILDERS[k]()
        nid, top = place(P, SECTIONS[k], html, top)
        r = P.c("get_node_info", nodeId=nid)
        print(k, nid, r.get("width"), r.get("height"))


if __name__ == "__main__":
    main()
