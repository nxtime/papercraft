"""Candidate page: `Section / Candidate Access` (/, /invite/[code], /invite/[code]/verify) and
`Section / Candidate Session` (/session/[id], /setup, /review). build(P, y0) is called by candidate.py (for "access" and "session")."""
from cand_common import *

# colours of the login split / hero / system-check page that have no token (globals.css, session page)
VARS = {
    "color-login-sub": "#ffffffa6", "color-login-icon-bg": "#ffffff12", "color-login-icon-fg": "#ffffff8c", "color-login-desc": "#ffffff99",
    "color-login-card": "#ffffff0d", "color-login-card-line": "#ffffff14", "color-login-card-head": "#ffffff99", "color-login-row": "#ffffffc7",
    "color-login-footer": "#ffffff8c", "color-hero-chip": "#ffffff33", "color-hero-text": "#ffffffeb", "color-cam-label": "#ffffff99",
    "color-cam-hint": "#ffffff99", "color-cam-ring": "#ffffff40",
}
BRAND = fg("brand-primary")
INV = fg("text-inverse")
SUB = fg("text-secondary")


def card(name, kids, pad="var(--spacing-lg)", **st):
    return col(name, kids, padding=pad, background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)",
               box_shadow="var(--shadow-sm)", **st)


def page(name, body, bg="var(--color-bg-layout)", direction="row"):
    return box(name, body, display="flex", flex_direction=direction, align_items="stretch", width="1440px", height="900px", background=bg, overflow="hidden")


def label(text_, req=False, right=""):
    star = T("Required Mark", "*", 14, fg("error")) if req else ""
    return row("Label Row", row("Label Group", star + T("Label", text_, 14), gap="4px") + right, justify_content="space-between", height="32px", width="100%")


def field(name, label_, ctrl, req=False, right="", mb="24px"):
    return col(name, label(label_, req, right) + ctrl, width="100%", gap="0px", padding=f"0 0 {mb} 0")


def brand_mark(dark=True):
    return row("Brand", row("Mark", T("Letter", "H", 14, INV, 800, lh=18), justify_content="center", width="28px", height="28px", border_radius="7px", background=BRAND) +
               T("Brand Name", "Hireloop", 16, INV if dark else fg("text-primary"), 700, lh=20), gap="12px")


def alert(msg, w, icon=True):
    mw = w - 26 - (24 if icon else 0)
    return row("Alert / Error", (ic("x-circle", 16, fg("score-poor"), "Alert Icon") if icon else "") + T("Message", msg, 14, fg("text-primary"), lh=22, width=f"{mw}px"),
               gap="8px", width=f"{w}px", padding="8px 12px", background=fg("tag-red-bg"), border="1px solid var(--color-tag-red-bd)", border_radius="var(--radius-md)")


# ------------------------------------------------------------------ / (login split)
FEATURES = [("squares-four", "Team-wide visibility", "Track every candidate across the loop, from screen to offer."),
            ("brackets-curly", "Live coding & system design", "Watch candidates work in a real shared editor and whiteboard."),
            ("diamonds-four", "Calibrated scorecards", "A shared rubric keeps every interviewer's ratings comparable.")]
PIPELINE = [("Amara Okafor", "Onsite loop"), ("Mei Lin Tan", "Debrief"), ("Sofia Bianchi", "Offer")]


def marketing():
    feats = "".join(row(f"Feature / {t}", row("Feature Icon", ic(i, 16, fg("login-icon-fg"), "Feature Glyph"), justify_content="center", width="32px", height="32px",
                                                 border_radius="6px", background=fg("login-icon-bg")) +
                        col("Feature Text", T("Feature Title", t, 14, INV, 600, lh=18) + T("Feature Desc", d, 12, fg("login-desc"), lh=18), gap="4px", flex_grow="1", width="500px"),
                        gap="16px", align_items="flex-start") for i, t, d in FEATURES)
    prow = "".join(row(f"Row / {n}", row("Name", box("Dot", width="8px", height="8px", border_radius="99px", background=fg("login-live-fg")) + T("Name", n, 14, fg("login-row"), lh=18), gap="8px") +
                       T("Stage", s, 12, fg("login-desc"), lh=16), justify_content="space-between", padding="8px 0") +
                       (box("Divider", width="100%", height="1px", background=fg("login-card-line")) if k < 2 else "")
                       for k, (n, s) in enumerate(PIPELINE))
    pipe = col("Pipeline Card", row("Pipeline Header", T("Pipeline Title", "CANDIDATE PIPELINE", 12, fg("login-card-head"), 700, lh=16, letter_spacing="0.7px") +
                                    row("Live Pill", T("Live Label", "Live", 12, fg("login-live-fg"), 600, lh=16), padding="4px 8px", border_radius="99px", background=fg("login-live-bg")),
                                    justify_content="space-between", padding="0 0 8px 0") + prow,
               padding="16px", background=fg("login-card"), border="1px solid var(--color-login-card-line)", border_radius="12px", width="100%")
    hero = col("Hero", T("Hero Title", "Run better interviews, together.", 30, INV, 800, lh=36, letter_spacing="-0.6px") +
               T("Hero Sub", "Structured scorecards, live coding sessions, and calibrated hiring decisions — all in one workspace for your team.", 14, fg("login-sub"), lh=22), gap="16px")
    mid = col("Marketing Body", hero + col("Features", feats, gap="20px") + pipe, gap="32px", flex_grow="1", justify_content="center", padding="56px 0 0 0")
    return col("Marketing Panel", brand_mark() + mid + T("Footer", "© 2026 Hireloop. All rights reserved.", 12, fg("login-footer"), lh=16),
               width="662px", padding="32px 44px 32px 40px", background=fg("login-bg"))


G_LOGO = ('<svg layer-name="Google Logo" width="16" height="16" viewBox="0 0 24 24"><path d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" fill="#4285F4"/>'
          '<path d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" fill="#34A853"/>'
          '<path d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" fill="#FBBC05"/>'
          '<path d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" fill="#EA4335"/></svg>')
M_LOGO = ('<svg layer-name="Microsoft Logo" width="16" height="16" viewBox="0 0 24 24"><rect x="1" y="1" width="10" height="10" fill="#F25022"/><rect x="13" y="1" width="10" height="10" fill="#7FBA00"/>'
          '<rect x="1" y="13" width="10" height="10" fill="#00A4EF"/><rect x="13" y="13" width="10" height="10" fill="#FFB900"/></svg>')


def sso(name, logo):
    return row(f"Button / {name}", logo + T("Label", name, 16, lh=24), justify_content="center", gap="8px", height="44px", flex_grow="1", background=fg("bg-elevated"),
               border="1px solid var(--color-border)", border_radius="var(--radius-md)")


def login(error=False):
    err = alert("Email/password sign-in not yet configured. Use an invite link.", 400, True) if error else ""
    err = col("Alert Wrap", err, padding="0 0 20px 0", width="100%") if error else ""
    forgot = T("Forgot", "Forgot password?", 14, BRAND)
    form = (field("Field / Work email", "Work email", I("Input / Large", "Email", style="width:100%", Value="you@company.com"), True) +
            field("Field / Password", "Password", I("Input / Password", "Password", style="width:100%", Value="" if not error else "••••••••••"), True, forgot) +
            col("Remember", I("Checkbox / Unchecked", "Remember", Label="Keep me signed in"), padding="0 0 16px 0") +
            I("Ant Button / Primary Large", "Sign in", style="width:100%", Label="Sign in"))
    divider = row("Divider", box("Line", height="1px", flex_grow="1", background=fg("border-light")) + T("Divider Text", "or continue with", 12, SUB, lh=16) +
                  box("Line", height="1px", flex_grow="1", background=fg("border-light")), gap="12px", width="100%", padding="16px 0")
    links = col("Links", row("Admin Link", T("Text", "Don't have access?", 14, SUB, lh=20) + T("Link", "Ask your workspace admin for an invite", 14, BRAND, lh=20), justify_content="center", gap="4px") +
                row("Candidate Link", T("Text", "Interviewing for a role?", 14, SUB, lh=20) + T("Link", "Candidate sign in →", 14, SUB, lh=20), justify_content="center", gap="4px"),
                gap="8px", padding="24px 0 0 0", width="100%")
    inner = col("Login Form", T("Heading", "Sign in to your workspace", 30, fg("text-primary"), 700, lh=38, letter_spacing="-0.3px") +
                col("Sub Wrap", T("Sub", "Enter your credentials to continue to Hireloop.", 14, SUB, lh=22), padding="4px 0 24px 0") + err + form + divider +
                row("SSO Row", sso("Google", G_LOGO) + sso("Microsoft", M_LOGO), gap="12px", width="100%") + links, width="400px")
    right = row("Form Panel", inner, justify_content="center", flex_grow="1", background=fg("bg-elevated"), padding="48px 40px")
    return page("Screen / Login" + (" Error" if error else ""), marketing() + right)


# ------------------------------------------------------------------ /invite/[code]
def public(name, kids):
    c = col("Card", kids, width="384px", padding="24px", background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-md)")
    return page(name, row("Main", c, justify_content="center", width="100%", padding="48px"))


def h3(t):
    return col("Heading Wrap", T("Heading", t, 24, fg("text-primary"), 600, lh=32), padding="0 0 12px 0")


def para(t, secondary=False):
    return col("Para Wrap", T("Paragraph", t, 14, SUB if secondary else fg("text-primary"), lh=22), padding="0 0 16px 0")


def invite(state):
    disabled = state != "ready"
    head = h3("Join interview")
    head += {"ready": para("Senior Frontend Engineer — technical interview"), "loading": para("Loading…", True), "error": ""}[state]
    ctrl = I("Input / Disabled" if disabled else "Input", "Email", style="width:100%", Value="")
    btn = I("Ant Button / Primary", "Send code", style="width:100%" + (";opacity:.5" if disabled else ""), Label="Send code")
    err = col("Alert Wrap", alert("Invite not found or expired", 336, True), padding="16px 0 0 0", width="100%") if state == "error" else ""
    name = {"ready": "Invite", "loading": "Invite Loading", "error": "Invite Error"}[state]
    return public(f"Screen / {name}", head + field("Field / Email", "Email", ctrl, True) + btn + err)


def verify(error=False):
    err = col("Alert Wrap", alert("Invalid code", 336, False), padding="0 0 16px 0", width="100%") if error else ""
    ctrl = I("Input / Error" if error else "Input", "Code", style="width:100%", Value="482913" if error else "")
    return public("Screen / Verify" + (" Error" if error else ""), h3("Enter your code") + para("We sent a 6-digit code to aisha.khan@example.com.") + err +
                  field("Field / Code", "6-digit code", ctrl, True) + I("Ant Button / Primary Large", "Verify", style="width:100%", Label="Verify"))


# ------------------------------------------------------------------ /session/[id]
def hero():
    chip = row("Up Next Chip", T("Chip Label", "UP NEXT", 12, INV, 600, lh=16), padding="4px 12px", border_radius="99px", background=fg("hero-chip"))
    top = col("Hero Head", row("Hero Meta", chip + T("Meta", "·  Algorithm challenge", 12, fg("hero-text"), lh=16), gap="8px") +
              T("Hero Title", "Algorithm interview", 22, INV, 700, lh=28) + T("Hero Sub", "Build a debounced SearchBox", 14, fg("hero-text"), lh=22),
              gap="8px", padding="24px", width="100%", background_color="var(--color-tag-blue-fg)", background_image="linear-gradient(135deg, var(--color-brand-primary), var(--color-tag-blue-fg))")
    meta = "".join(col(f"Meta / {k}", T("Key", k.upper(), 12, SUB, lh=16, letter_spacing="0.5px") + T("Value", v, 14, fg("text-primary"), 600, lh=22), gap="4px")
                   for k, v in [("Scheduled", "Mar 14, 2026, 10:00 AM"), ("Language", "TypeScript"), ("Editor", "Run + tests built in"), ("Format", "Coding + theory")])
    body = col("Hero Body", T("Intro", "You'll solve a live coding problem in a shared editor while your interviewer watches. Run tests as often as you like before submitting.", 14, SUB, lh=22) +
               row("Meta Row", meta, gap="20px", width="100%") +
               row("Actions", I("Button / Primary Large", "Enter session", Label="Enter session") + I("Button / Default Large", "Practice", Label="Practice problem"), gap="12px"),
               gap="20px", padding="24px", width="100%")
    return col("Card / Up next", top + body, width="574px", background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)",
               box_shadow="var(--shadow-sm)", overflow="hidden")


def syscheck_card():
    return card("Card / System check", T("Title", "System check", 16, fg("text-primary"), 600, lh=24) +
                col("Desc Wrap", T("Desc", "We'll verify your setup before you start.", 12, SUB, lh=16), padding="8px 0 12px 0") +
                I("Check Item / Ready", "Browser", Label="Browser compatible") + I("Check Item / Ready", "Mic", Label="Microphone access") + I("Check Item / Ready", "Camera", Label="Camera access"),
                width="382px")


def home_body(state):
    if state == "loading":
        return I("Skeleton", "Skeleton", style="width:100%")
    if state == "error":
        return I("Empty / Simple", "Empty", style="width:100%", Description="Session not found")
    greet = col("Greeting", T("Welcome", "Welcome back, Aisha", 26, fg("text-primary"), 700, lh=32, letter_spacing="-0.26px") +
                row("Who", T("Strong", "Candidate", 14, fg("text-primary"), 600, lh=22) + T("Rest", "·  Session af17f53a", 14, SUB, lh=22), gap="8px"), gap="4px")
    tips = row("Tips", "".join(I(f"Tip Card / {t}", t) for t in ["Read carefully", "Think aloud", "Manage time"]), gap="16px")
    return (greet + row("Two Col", hero() + syscheck_card(), gap="24px", align_items="flex-start") + I("Progress Tracker / System check", "Progress") + tips)


def home(state="ready"):
    name = {"ready": "Session Home", "loading": "Session Home Loading", "error": "Session Home Error"}[state]
    content = row("Page Center", col("Page", home_body(state), width="980px", gap="24px"), justify_content="center", align_items="flex-start", width="100%")
    return shell(f"Screen / {name}", "Home", "Aisha Khan", "Session af17f53a", content)


# ------------------------------------------------------------------ /session/[id]/setup
CHECKS = [("Browser compatibility", "Chrome, Edge, or Firefox — up to date"), ("Camera access", "Required for onsite & behavioral rounds"),
          ("Microphone access", "Required so interviewers can hear you"), ("Network speed", "At least 5 Mbps recommended")]


def setup_screen(name, states, mic):
    hdr = row("Header", brand_mark(False) + row("Steps", I("Syscheck Step / Done", "Step 1") + box("Sep", width="24px", height="1px", background=fg("border")) + I("Syscheck Step / Active", "Step 2"), gap="12px"),
              justify_content="space-between", width="100%", height="56px", padding="0 32px", background=fg("bg-elevated")) + box("Header Line", width="100%", height="1px", background=fg("border-light"))
    cam = col("Camera Preview", T("Preview Label", "preview", 12, fg("cam-label"), lh=16, position="absolute", left="12px", top="10px") +
              col("Placeholder", row("Cam Icon", ic("camera", 18, fg("cam-hint"), "Camera Off"), justify_content="center", width="36px", height="36px", border_radius="99px", border="2px solid var(--color-cam-ring)") +
                  T("Hint", "camera preview", 12, fg("cam-hint"), lh=16), align_items="center", gap="8px"),
              position="relative", align_items="center", justify_content="center", width="300px", height="225px", border_radius="var(--radius-md)", background=fg("syscheck-camera"), overflow="hidden")
    miclevel = row("Mic Row", T("Mic Label", "Mic level", 12, SUB, lh=16) + row("Mic Bar", box("Mic Fill", width=f"{int(2.2 * mic)}px", height="6px", border_radius="99px", background=fg("syscheck-mic")),
                                                                            flex_grow="1", height="6px", border_radius="99px", background=fg("border-light"), overflow="hidden"), gap="12px", width="100%")
    cam_col = col("Camera Col", cam + miclevel, width="340px", align_self="stretch", padding="20px", gap="16px")
    rows = "".join(I(f"Syscheck Row / {s}", f"Check / {t}", Title=t, Desc=d) for s, (t, d) in zip(states, CHECKS))
    checks = col("Checks Col", rows, width="400px", padding="20px", gap="0px")
    sc = row("Check Card", cam_col + box("Col Line", width="1px", align_self="stretch", background=fg("border-light")) + checks, width="740px", background=fg("bg-elevated"), border_radius="var(--radius-lg)", box_shadow="var(--shadow-sm)", overflow="hidden", align_items="stretch")
    back = row("Back Button", T("Label", "Back", 14, fg("text-primary"), lh=22), padding="8px 24px", background=fg("bg-elevated"), border="1px solid var(--color-border)", border_radius="6px")
    main = col("Main", T("Title", "Let's check your setup", 22, fg("text-primary"), 700, lh=28, text_align="center") +
               col("Sub Wrap", T("Sub", "We'll verify your browser, camera, microphone, and connection before your interview begins.", 14, SUB, lh=22, text_align="center", width="480px"), padding="8px 0 28px 0") +
               sc + col("Back Wrap", back, padding="28px 0 0 0"), align_items="center", justify_content="center", flex_grow="1", padding="40px 24px")
    return page(name, hdr + main, "var(--color-syscheck-bg)", "column")


# ------------------------------------------------------------------ /session/[id]/review
ROWS = [("3f2a9c1e-7b4d-4e0a-9c55-1d8e2b6f40a7", "Graded", "", "2026-03-14T10:42:17Z"),
        ("a91c5d02-34be-4f7a-8e11-6c2d90b3e5f4", "Errored", "Runner timed out after 30s", "2026-03-14T10:58:03Z"),
        ("c7e48b36-0d15-42a9-b3f8-95a1e7d20c68", "Running", "", "2026-03-14T11:05:49Z")]


LINE = '<tr layer-name="Divider"><td style="padding:0;height:1px;background:var(--color-border-light)"></td></tr>'


def cell(tag, inner, w=None, **extra):
    st = dict(padding="16px", border="none")
    if w:
        st["width"] = w
    st.update(extra)
    return f'<{tag} style="{css(**st)}">{inner}</{tag}>'


def review(state="ready"):
    hd = dict(font_size="var(--font-size-sm)", font_weight="600", color="var(--color-text-primary)", background_color="var(--color-bg-sider)")
    head = '<tr layer-name="Row / Header">' + cell("th", "Challenge", **hd) + cell("th", "Status", "160px", **hd) + cell("th", "Error", "240px", **hd) + cell("th", "Submitted at", "200px", **hd) + "</tr>" + LINE
    body = ""
    if state == "ready":
        for cid, st, e, at in ROWS:
            err = T("Error", e, 14, fg("error"), lh=22) if e else T("Error", "—", 14, lh=22)
            body += (f'<tr layer-name="Row / {cid[:8]}">' + cell("td", T("Challenge Id", cid, 14, lh=22)) + cell("td", I(f"Submission Status Badge / {st}", "Status"), "160px") +
                     cell("td", err, "240px") + cell("td", T("Submitted", at, 14, lh=22), "200px") + "</tr>" + LINE)
    tbl = f'<table layer-name="Table" style="width:100%">{head}{body}</table>'
    if state == "empty":
        tbl += col("Empty Wrap", I("Empty / Simple", "Empty", style="width:100%", Description="No data"), width="100%", padding="24px 0")
    pager = row("Pagination", row("Page 1", T("Page Number", "1", 14, BRAND, lh=22), justify_content="center", width="32px", height="32px", border="1px solid var(--color-brand-primary)", border_radius="var(--radius-md)"),
                justify_content="flex-end", width="100%", padding="16px 0 0 0") if state == "ready" else ""
    err = col("Alert Wrap", alert("Failed to load submissions: network request failed", 1094, True), padding="0 0 16px 0", width="100%") if state == "error" else ""
    headbar = row("Card Head", T("Title", "Review", 24, fg("text-primary"), 600, lh=32) + T("Back Link", "Back to session", 14, BRAND, lh=22), justify_content="space-between", width="100%",
                  padding="16px 24px") + box("Head Line", width="100%", height="1px", background=fg("border-light"))
    inner = col("Card Body", err + col("Table Wrap", tbl + pager, width="100%"), padding="24px", width="100%")
    c = col("Card", headbar + inner, width="100%", background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-md)")
    name = {"ready": "Review", "empty": "Review Empty", "error": "Review Error"}[state]
    return shell(f"Screen / {name}", "", "Review", "Your submissions", c)


# ------------------------------------------------------------------ sections
def section(P, name, title, screens, y, per_row=4):
    blocks = [col(f"Block / {c}", T("Caption", f"{c}  ·  {r}", 14, SUB, 600) + h, gap="8px") for c, r, h in screens]
    rows = "".join(row(f"Screens Row {i // per_row + 1}", "".join(blocks[i:i + per_row]), gap="var(--spacing-xl)", align_items="flex-start") for i in range(0, len(blocks), per_row))
    sec = col(name, T("Section Title", title, 24, weight=700) + col("Screens", rows, gap="var(--spacing-xl)"), gap="var(--spacing-xl)", padding="var(--spacing-xl)", background="transparent")
    nid = write(P, "root", sec)[0]
    place(P, nid, 0, y)
    return nid


def build(P, y0):
    call("set_variables", {"fileId": P.file, "variables": VARS})
    print("deleted", delete_named(P, lambda n: n in ("Section / Candidate Access", "Section / Candidate Session")))
    a = section(P, "Section / Candidate Access", "Candidate / Access", [
        ("Login", "/", login()), ("Login error", "/", login(True)),
        ("Invite", "/invite/[code]", invite("ready")), ("Invite loading", "/invite/[code]", invite("loading")),
        ("Invite error", "/invite/[code]", invite("error")), ("Verify", "/invite/[code]/verify", verify()), ("Verify error", "/invite/[code]/verify", verify(True))], y0)
    h = next(c["height"] for c in top_children(P) if c["id"] == a)
    s = section(P, "Section / Candidate Session", "Candidate / Session", [
        ("Session home", "/session/[id]", home()), ("Session home loading", "/session/[id]", home("loading")), ("Session home error", "/session/[id]", home("error")),
        ("System check ready", "/session/[id]/setup", setup_screen("Screen / System Check Ready", ["Ready"] * 4, 60)),
        ("System check checking", "/session/[id]/setup", setup_screen("Screen / System Check Checking", ["Ready", "Checking", "Checking", "Pending"], 0)),
        ("System check failed", "/session/[id]/setup", setup_screen("Screen / System Check Failed", ["Ready", "Failed", "Failed", "Failed"], 0)),
        ("Review", "/session/[id]/review", review()), ("Review empty", "/session/[id]/review", review("empty")), ("Review error", "/session/[id]/review", review("error"))], int(y0 + h + 200))
    return {"access": a, "session": s}
