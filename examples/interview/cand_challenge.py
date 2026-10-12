"""Section / Candidate Challenge: /session/[id]/challenge/[challengeId] (IDE, theory phase, submit form). build(P, y0) -> section node id."""
import cand_components as cc
from cand_common import *

D = cc.D
IDE_BG = "var(--color-code-bg)"      # #1e1e1e
CENTER_W = 666                       # code tab centre column (1192 - 210 - 316)
WIDE_W = 876                         # diagrams / whiteboard centre column (1192 - 316)
BODY_H = 848
LOAD = "Loading challenge"


def frame(name, content):
    """Candidate shell, full bleed: sidebar + dark IDE area."""
    sb = I("Candidate Sidebar / My interview Active", "Sidebar", style="height:900px")
    return sb + col("Main", content, width=f"{MAIN_W}px", height="900px", background=IDE_BG)


def scr(name, content, **st):
    return screen(name, frame(name, content), position="relative", **st)


def rail(collapsed=False):
    return I("Problem Rail / Collapsed" if collapsed else "Problem Rail / Brief", "Problem Rail")


def code_screen(name, console, toolbar="IDE Toolbar / Code", toast=False, file="SearchBox", extra=""):
    src = {"SearchBox": ("Code Editor / SearchBox", "Editor Tabs / SearchBox Active", "File Tree / SearchBox Active"),
           "Debounce": ("Code Editor / useDebounce", "Editor Tabs / Debounce Active", "File Tree / Test Active")}[file]
    centre = col("Editor Column", I(src[1], "Editor Tabs") + I(src[0], "Code Editor") + box("Resize Handle", width="100%", height="6px", background=D("deep")) + I(console, "Console"),
                 width=f"{CENTER_W}px", height=f"{BODY_H}px")
    body = row("Body", I(src[2], "File Tree") + centre + rail(), height=f"{BODY_H}px", align_items="stretch")
    toast_html = I("Message / Success", "Toast", style="position:absolute;left:626px;top:16px") if toast else ""
    return scr(name, I(toolbar, "Toolbar") + body + toast_html + extra)


# --- diagrams
MERMAID = ["graph TD", "    Client -->|HTTPS| LB[Load Balancer]", "    LB --> API1[API Server 1]", "    LB --> API2[API Server 2]", "    API1 <--> Cache[(Redis Cache)]",
           "    API2 <--> Cache", "    API1 --> DB[(PostgreSQL Primary)]", "    API2 --> DB", "    DB -.->|replication| Replica[(Read Replica)]",
           "    API1 --> Queue([Message Queue])", "    Queue --> Worker[Background Worker]"]


def dnode(label):
    return row(f"Node / {label}", T("Label", label, 12, D("text"), lh=16), padding="8px 16px", background=D("panel"), border=f"1px solid {D('text-faint')}", border_radius="var(--radius-sm)")


def darrow(label=""):
    return row("Edge", ic("arrow-down", 14, D("text-muted"), "Edge Icon") + (T("Edge Label", label, 12, D("text-muted"), lh=16) if label else ""), gap="4px")


def diagram_body(tpl="System design"):
    toolbar = I(f"Diagram Toolbar / {tpl}", "Diagram Toolbar", style=f"width:{WIDE_W}px")
    src = "".join(T(f"Src {i + 1}", l.replace(" ", "&nbsp;"), 13, fg("code-fg"), lh=23) for i, l in enumerate(MERMAID))
    left = col("Mermaid Source", T("Pane Label", "MERMAID", 12, D("text-muted"), letter_spacing="0.6px", position="absolute", right="10px", top="8px") + src,
               width="50%", height="100%", padding="16px", background=D("input"), border_right=f"1px solid {D('deep')}", position="relative", gap="0px")
    graph = col("Rendered Graph", dnode("Client") + darrow("HTTPS") + dnode("Load Balancer") + darrow() +
                row("API Row", dnode("API Server 1") + dnode("API Server 2"), gap="32px") + darrow() +
                row("Store Row", dnode("Redis Cache") + dnode("PostgreSQL Primary") + dnode("Message Queue"), gap="16px") + darrow("replication") +
                row("Leaf Row", dnode("Read Replica") + dnode("Background Worker"), gap="32px"), align_items="center", gap="4px")
    right = row("Rendered Preview", graph, width="50%", height="100%", justify_content="center", align_items="flex-start", padding="28px 20px", background=fg("diagram-bg"))
    panes = row("Panes", left + right, width=f"{WIDE_W}px", height=f"{BODY_H - 41}px", align_items="stretch")
    return row("Body", col("Diagram Editor", toolbar + panes, width=f"{WIDE_W}px", height=f"{BODY_H}px") + rail(), height=f"{BODY_H}px", align_items="stretch")


# --- whiteboard
def shape(name, x, y, w, h, color, label, r=4):
    return box(f"Shape / {name}", T("Text", label, 14, color, text_align="center", width=f"{w}px"), position="absolute", left=f"{x}px", top=f"{y}px", width=f"{w}px", height=f"{h}px",
               border=f"2px solid {color}", border_radius=f"{r}px", display="flex", align_items="center", justify_content="center")


def whiteboard_body(tool="Rectangle", selected=False):
    W, B, G, R, O = fg("wb-white"), fg("wb-blue"), fg("ide-pass"), fg("ide-fail"), fg("wb-orange")
    ar = lambda n, i, c, x, y: ic(i, 40, c, n).replace("<x-paper-icon", f'<x-paper-icon style="position:absolute;left:{x}px;top:{y}px"')
    shapes = (shape("Client", 60, 70, 140, 64, W, "Client") + shape("API", 330, 70, 160, 64, B, "API Gateway") + shape("Auth", 330, 250, 160, 64, G, "Auth Service") +
              shape("DB", 620, 70, 160, 64, O, "Postgres", 32) + shape("Cache", 620, 250, 160, 64, R, "Redis", 32) +
              ar("Arrow / Client to API", "arrow-right", W, 220, 82) + ar("Arrow / API to DB", "arrow-right", B, 510, 82) + ar("Arrow / API to Auth", "arrow-down", B, 390, 160) +
              ar("Arrow / Auth to Cache", "arrow-right", G, 510, 262) + T("Note", "cache sessions, TTL 15m", 15, fg("wb-purple"), position="absolute", left="610px", top="340px"))
    canvas = box("Canvas", shapes, position="relative", width=f"{WIDE_W}px", height=f"{BODY_H - 51}px", background=fg("wb-bg"), overflow="hidden")
    tb = I(f"Whiteboard Toolbar / {'Select' if selected else tool}", "Whiteboard Toolbar", style=f"width:{WIDE_W}px")
    return row("Body", col("Whiteboard", tb + canvas, width=f"{WIDE_W}px", height=f"{BODY_H}px") + rail(), height=f"{BODY_H}px", align_items="stretch")


# --- theory phase
Q = ["Tell me about a time you disagreed with a teammate on a technical decision. How did you resolve it?",
     "How do you prioritise when three stakeholders each say their request is urgent?"]


def theory_body(q, cat, diff, qn, n, answer, words, last=False):
    chips = row("Chips", I(f"Theory Chip / {cat}", "Category") + I(f"Theory Chip / {diff}", "Difficulty") + box("Spacer", flex_grow="1") +
                T("Counter", f"Question {qn} of {n}", 12, D("text-muted"), lh=16), gap="8px", width="100%")
    prev = I("Theory Button / Prev Disabled" if qn == 1 else "Theory Button / Prev", "Prev")
    left = row("Left", prev + (T("Word Count", f"{words} words", 12, D("text-muted"), lh=16) if words else ""), gap="8px")
    if last:
        right = col("Submit Group", I("Theory Button / Submit Partial", "Submit", Label="Submit (3/5 answered)") +
                    T("Hint", "Unanswered questions will be marked as skipped.", 12, D("text-muted"), lh=16), align_items="flex-end", gap="8px")
    else:
        right = I("Theory Button / Next", "Next")
    page = col("Question Page", chips + col("Question Wrap", T("Question", q, 19, fg("text-inverse"), 600, lh=28, width="704px"), padding="20px 0 28px") +
               I("Theory Answer / Filled" if answer else "Theory Answer / Empty", "Answer") +
               row("Footer", left + right, justify_content="space-between", width="704px", padding="16px 0 28px"), width="760px", padding="40px 28px 0", align_items="flex-start")
    return row("Body", page, justify_content="center", align_items="flex-start", width="100%", flex_grow="1")


# --- states and form
def skeleton():
    bar = lambda i, w: box(f"Skeleton Bar {i}", width=f"{w}px", height="16px", border_radius="var(--radius-sm)", background=D("line"))
    return col("Loading", bar(1, 320) + bar(2, 880) + bar(3, 760) + bar(4, 840) + bar(5, 560) + bar(6, 880) + bar(7, 640) + bar(8, 420), padding="24px", gap="16px", width="100%", height="100%")


def error_state():
    return col("Error", T("Error Message", "Failed to load challenge", 14, fg("ide-fail")), padding="24px", width="100%", height="100%")


def form_body(inner):
    return col("Form Page", inner, padding="24px", width=f"{MAIN_W}px", height="100%", background=fg("bg-layout"), gap="16px")


def form_screen(name, inner):
    sb = I("Candidate Sidebar / My interview Active", "Sidebar", style="height:900px")
    return screen(name, sb + col("Main", form_body(inner), width=f"{MAIN_W}px", height="900px", background=fg("bg-layout")))


def mcq_form(error=None):
    qs = [("1. Which HTTP status code means the request succeeded and created a resource?", ["200 OK", "201 Created", "204 No Content", "302 Found"], 1),
          ("2. What does the CAP theorem say about a network partition?", ["You must choose consistency or availability", "You can have all three guarantees",
                                                                          "Partitions never happen in practice", "Only availability is affected"], 0)]
    items = ""
    for qi, (q, opts, sel) in enumerate(qs, 1):
        rad = "".join(I("Radio / Selected" if i == sel else "Radio / Unselected", f"Option / {o}", Label=o) for i, o in enumerate(opts))
        items += col(f"Form Item / Q{qi}", T("Label", q, 14, fg("text-primary"), lh=32) + col("Options", rad, gap="8px"), gap="4px", padding="0 0 24px")
    err = row("Alert", ic("x-circle", 16, fg("score-poor"), "Alert Icon") + T("Message", error, 14, fg("text-primary"), flex_grow="1"), gap="8px", width="720px", padding="8px 12px",
              background=fg("tag-red-bg"), border="1px solid var(--color-tag-red-bd)", border_radius="var(--radius-md)") if error else ""
    return col("ChallengeSubmitForm", err + col("Form", items + I("Ant Button / Primary", "Submit Button", Label="Submit"), gap="0px"), width="720px", gap="16px")


def text_form():
    return col("ChallengeSubmitForm", col("Form Item", T("Label", "Your answer", 14, fg("text-primary"), lh=32) +
                                          row("Textarea", T("Value", "I would split the monolith along its billing and catalogue boundaries first, because those two teams already deploy independently.", 14, fg("text-primary"), flex_grow="1"),
                                              align_items="flex-start", width="720px", height="260px", padding="4px 12px", background=fg("bg-elevated"), border="1px solid var(--color-border)", border_radius="var(--radius-md)"),
                                          gap="0px", padding="0 0 24px") + I("Ant Button / Primary", "Submit Button", Label="Submit"), width="720px")


def status_strip():
    tags = "".join(I(f"Submission Status Badge / {s}", f"Badge / {s}") for s in ["Queued", "Running", "Graded", "Errored", "Unknown"])
    return col("SubmissionStatusBadge", T("Title", "Submission status", 16, weight=600) + row("Badges", tags, gap="8px"), padding="var(--spacing-lg)", gap="12px",
               background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)", width="720px")


def build(P, y0):
    name = "Section / Candidate Challenge"
    print("deleted", delete_named(P, lambda n: n == name))
    code_idle = code_screen("Screen / Challenge Code Idle", "Console / Empty")
    sets = [
        ("Code: idle", "/session/[id]/challenge/[challengeId]", code_idle),
        ("Code: running tests", "state: Run clicked, console streaming", code_screen("Screen / Challenge Code Running", "Console / Running", file="Debounce")),
        ("Code: tests passed", "state: Run finished", code_screen("Screen / Challenge Code Passed", "Console / Passed")),
        ("Code: tests failed, low time", "state: timer under 5 minutes", code_screen("Screen / Challenge Code Failed", "Console / Failed", "IDE Toolbar / Code Low Time")),
        ("Code: submitted", "state: Submit clicked, then redirect to /session/[id]/review", code_screen("Screen / Challenge Code Submitted", "Console / Passed", toast=True)),
        ("Diagrams tab", "tab: Diagrams", scr("Screen / Challenge Diagrams", I("IDE Toolbar / Diagrams", "Toolbar") + diagram_body())),
        ("Whiteboard tab", "tab: Whiteboard", scr("Screen / Challenge Whiteboard", I("IDE Toolbar / Whiteboard", "Toolbar") + whiteboard_body())),
        ("Whiteboard: shape selected", "tool: Select", scr("Screen / Challenge Whiteboard Selected", I("IDE Toolbar / Whiteboard", "Toolbar") + whiteboard_body(selected=True))),
        ("Theory: answered", "type: THEORY", scr("Screen / Challenge Theory", I("Theory Toolbar / Q2 of 5", "Toolbar") +
                                                 theory_body(Q[0], "Category", "Medium", 2, 5, True, 38))),
        ("Theory: last question", "type: THEORY, submit", scr("Screen / Challenge Theory Submit", I("Theory Toolbar / Q5 of 5", "Toolbar") +
                                                               theory_body(Q[1], "Category", "Hard", 5, 5, False, 0, last=True))),
        ("Loading", "state: loading", scr("Screen / Challenge Loading", skeleton())),
        ("Load error", "state: error", scr("Screen / Challenge Error", error_state())),
        ("MCQ form", "type: MCQ (ChallengeSubmitForm)", form_screen("Screen / Challenge MCQ", mcq_form())),
        ("MCQ form: submit error", "state: submit failed", form_screen("Screen / Challenge MCQ Error", mcq_form("Submit failed"))),
        ("Text form + status badges", "type: text (ChallengeSubmitForm), SubmissionStatusBadge", form_screen("Screen / Challenge Text", text_form() + status_strip())),
    ]
    rows = ""
    for i in range(0, len(sets), 2):
        blocks = "".join(col(f"Block / {c}", T("Caption", f"{c}  ·  {r}", 14, fg("text-secondary"), 600) + html, gap="8px") for c, r, html in sets[i:i + 2])
        rows += row(f"Row {i // 2 + 1}", blocks, gap="var(--spacing-xl)", align_items="flex-start")
    sec = col(name, T("Section Title", "Candidate Challenge", 24, weight=700) + col("Screens", rows, gap="var(--spacing-xl)"), gap="var(--spacing-xl)",
              padding="var(--spacing-xl)", background="transparent")
    nid = write(P, "root", sec)[0]
    place(P, nid, 0, y0)
    return nid


if __name__ == "__main__":
    fid, P, DS = setup_cand()
    y0 = int(max(c.get("y", 0) + c["height"] for c in top_children(P) if not c["name"].startswith("Section / Candidate"))) + 200
    print(build(P, y0))
