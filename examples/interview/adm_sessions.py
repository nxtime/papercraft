"""Section / Admin / Sessions: list (3 states), new session (2 states), session detail (3 states)."""
from adm_common import *

ROWS = [
    ("Senior backend screen — Marcos R.", "K7M2-9QXA", "Graded"),
    ("Frontend pairing — Aisha Khan", "4HTR-B2LN", "In progress"),
    ("Platform engineer loop — Diego Alvarez", "P9WD-33EC", "Submitted"),
    ("Data engineer screen — Lena Fischer", "M3QZ-81VY", "Open"),
    ("Staff SRE system design — Tom Becker", "X6JA-0RKD", "Draft"),
    ("Mobile lead interview — Priya Nair", "C2FS-77HM", "Cancelled"),
    ("Junior full-stack screen — Omar Haddad", "T8LB-45GE", "Graded"),
]
FILTERS = ["All", "Open", "In progress", "Submitted", "Graded", "Draft"]
TH = dict(padding="12px", font_size=fs(12), font_weight="600", color="var(--color-text-secondary)", background_color="var(--color-bg-sider)",
          border_bottom="1px solid var(--color-border-light)")


def code_chip(c):
    return box("Code", c, font_size=fs(12), line_height="16px", color="var(--color-text-secondary)", background_color="var(--color-bg-sider)",
               padding="2px 8px", border_radius="var(--radius-sm)", border="1px solid var(--color-border-light)")


def _cell(tag, inner, w=None, al="left", **extra):
    st = dict(padding="12px", text_align=al, border_bottom="1px solid var(--color-border-light)")
    if w:
        st["width"] = w
    st.update(extra)
    return f'<{tag} style="{css(**st)}">{inner}</{tag}>'


def table(rows, hover=None):
    hd = dict(font_size=fs(12), font_weight="600", color="var(--color-text-secondary)", background_color="var(--color-bg-sider)")
    head = ('<tr layer-name="Row / Header">' + _cell("th", "SESSION", **hd) + _cell("th", "CODE", "140px", **hd) +
            _cell("th", "STATUS", "150px", **hd) + _cell("th", "&nbsp;", "90px", "right", **hd) + "</tr>")
    body = ""
    for i, (title, code, st) in enumerate(rows):
        bg = dict(background_color="var(--color-bg-selected)") if hover == i else {}
        bd = dict(font_size=fs(14), color="var(--color-text-primary)", **bg)
        body += (f'<tr layer-name="Row / {title.split(" — ")[0]}">' +
                 _cell("td", f'<span style="font-weight:600">{title}</span>', **bd) + _cell("td", code_chip(code), "140px", **bd) +
                 _cell("td", inst(f"Tag Dot / {STATUS[st]}", "Status", Label=st), "150px", **bd) +
                 _cell("td", '<span style="color:var(--color-brand-primary);font-weight:500">Open →</span>', "90px", "right", **bd) + "</tr>")
    return card("Data Table", f'<table layer-name="Table" style="width:100%">{head}{body}</table>')


def toolbar(active="All"):
    pills = "".join(inst("Pill / Active" if f == active else "Pill / Inactive", f"Pill / {f}", Label=f) for f in FILTERS)
    return row("Toolbar", row("Filters", pills, gap="8px") + inst("Search Input", "Search", style="width:240px", Placeholder="Search by title or code…"),
               justify_content="space-between", width="100%")


def new_btn():
    return header_action("Button / Primary", "New session")


def list_screens(A, sid):
    add_screen(A, sid, "Sessions List", "/sessions",
               shell("Sessions", "All interview sessions across the team", page_content(toolbar() + table(ROWS)), new_btn()))
    add_screen(A, sid, "Sessions List Filtered", "/sessions (Graded filter, search active)",
               shell("Sessions", "All interview sessions across the team",
                     page_content(toolbar("Graded") + table([r for r in ROWS if r[2] == "Graded"], hover=0)), new_btn()))
    add_screen(A, sid, "Sessions List Loading", "/sessions (loading)",
               shell("Sessions", "All interview sessions across the team",
                     page_content(toolbar() + row("Loading", inst("Spin / Large", "Spinner"), justify_content="center", padding="48px", width="100%")), new_btn()))
    add_screen(A, sid, "Sessions List Empty", "/sessions (no sessions)",
               shell("Sessions", "All interview sessions across the team",
                     page_content(toolbar() + col("Empty", inst("Empty State", "Empty State", Title="No sessions yet",
                                                              Description="Create a session to invite a candidate and start interviewing.") +
                                                  inst("Button / Primary", "New session", Label="New session"), align_items="center", gap="16px", padding="32px", width="100%")), new_btn()))


def label_row(label, required=False):
    return row("Label", (T("Required Mark", "*", 14, "var(--color-error)") if required else "") + T("Label", label, 14), gap="4px")


def field(label, control, required=False, extra=None):
    return col(f"Form Item / {label}", label_row(label, required) + control + (T("Extra", extra, 14, "var(--color-text-secondary)") if extra else ""), gap="8px", width="100%")


def new_screens(A, sid):
    title_in = inst("Input / Large", "Title Input", style="width:100%", Value="e.g. Senior backend screen — Marcos R.")
    date_in = inst("Date Picker", "Expires Picker", style="width:100%", Value="Select date")

    def form(title_ctl, err=None):
        err_html = box("Alert / Error", row("Alert Row", ic("x-circle", 16, "var(--color-error)") + T("Message", err, 14), gap="8px"),
                       padding="8px 16px", background_color="var(--color-tag-red-bg)", border="1px solid var(--color-tag-red-bd)",
                       border_radius="var(--radius-md)") if err else ""
        inner = (T("Card Title", "New session", 24, weight=600, lh=32) + err_html +
                 field("Title", title_ctl, True) +
                 field("Invite expires", date_in, False, "Leave blank for the default (7 days from now).") +
                 inst("Ant Button / Primary", "Create", Label="Create"))
        return page_content(card("Card", col("Card Body", inner, gap="16px", padding="24px"), box_shadow="var(--shadow-sm)"), wide=False)

    add_screen(A, sid, "New Session", "/sessions/new", shell("New session", "Create an invite for a candidate", form(title_in)))
    add_screen(A, sid, "New Session Error", "/sessions/new (create failed)",
               shell("New session", "Create an invite for a candidate",
                     form(inst("Input / Large Filled", "Title Input", style="width:100%", Value="Senior backend screen — Marcos R."), "Create failed")))


def detail_card(session, status, picked):
    chips = "".join(row(f"Selected / {c}", T("Label", c, 14) + ic("x", 12, "var(--color-text-secondary)"), gap="8px",
                        padding="0 8px", height="24px", background_color="var(--color-bg-sider)", border="1px solid var(--color-border-light)",
                        border_radius="var(--radius-sm)") for c in picked)
    sel = row("Select (multiple)", row("Selected Items", chips, gap="4px", flex_wrap="wrap", flex_grow="1") + ic("caret-down", 12, "var(--color-text-secondary)"),
              padding="4px 8px", min_height="32px", border="1px solid var(--color-border)", border_radius="var(--radius-md)",
              background_color="var(--color-bg-elevated)", width="100%", align_items="center")
    head = row("Card Head", row("Title Space", T("Title", session["title"], 24, weight=600, lh=32) + inst("Ant Tag / Default", "Status", Label=status) +
                                inst("Ant Button / Primary", "Open review", Label="Open review") +
                                inst("Ant Button / Default", "Open live", Label="Open live"), gap="8px"),
               padding="16px 24px", border_bottom="1px solid var(--color-border-light)", width="100%")
    meta = col("Meta", row("Invite Code", T("Label", "Invite code:", 14, "var(--color-text-secondary)") + T("Code", session["code"], 16, lh=24), gap="4px") +
               row("Expires", T("Label", "Expires:", 14, "var(--color-text-secondary)") + T("Value", session["expires"], 14), gap="4px"), gap="8px")
    body = col("Card Body", meta + col("Challenges", T("Heading", "Challenges", 14, weight=600, lh=22) + sel +
                                       inst("Ant Button / Primary", "Save challenges", Label="Save challenges"), gap="12px", width="100%"),
               gap="16px", padding="24px", width="100%")
    return card("Card", head + body, box_shadow="var(--shadow-sm)", width="100%")


def detail_screens(A, sid):
    s = dict(title="Senior backend screen — Marcos R.", code="K7M2-9QXA", expires="2026-10-19T09:00:00Z")
    add_screen(A, sid, "Session Detail", "/sessions/[id]",
               shell("Senior backend screen — Marcos R.", "Session · Open", page_content(detail_card(s, "Open", ["Two Sum (CODE)", "System design: URL shortener (DIAGRAM)", "JavaScript Closures (MCQ)"]))))
    add_screen(A, sid, "Session Detail In Progress", "/sessions/[id] (candidate working)",
               shell("Frontend pairing — Aisha Khan", "Session · In progress",
                     page_content(detail_card(dict(title="Frontend pairing — Aisha Khan", code="4HTR-B2LN", expires="2026-10-17T16:30:00Z"), "In progress", ["React state bug (CODE)", "Event loop quiz (MCQ)"]))))
    add_screen(A, sid, "Session Detail Loading", "/sessions/[id] (loading)",
               shell("Session", "Loading", page_content(inst("Spin", "Spinner"))))
    err = row("Alert Error", ic("x-circle", 16, "var(--color-error)") + T("Message", "Load failed", 14) +
              box("Spacer", "", flex_grow="1") + inst("Ant Button / Default Small", "Retry", Label="Retry"), gap="8px", padding="8px 16px",
              background_color="var(--color-tag-red-bg)", border="1px solid var(--color-tag-red-bd)", border_radius="var(--radius-md)", width="100%")
    add_screen(A, sid, "Session Detail Error", "/sessions/[id] (error)", shell("Session", "Could not load session", page_content(err)))


def build(A):
    sid = section(A, "Section / Admin / Sessions", "Admin / Sessions")
    list_screens(A, sid)
    new_screens(A, sid)
    detail_screens(A, sid)
    return sid


if __name__ == "__main__":
    fid, A = setup()
    print(build(A))
