"""Admin / Challenges screens (idempotent): list states, new/edit challenge editors, templates modal.

    python3 admin_challenges.py [only-screen-substring]
"""
import sys

sys.path.insert(0, "/Volumes/Storage/Projects/papercraft/examples/interview")
from admin_ch_common import *  # noqa: F401,F403

SECTION = "Section / Admin / Challenges"
X0, Y0 = 0, 12000

ROWS = [
    ("Two Sum", "Algo", False), ("Valid Parentheses", "Algo", False), ("REST Todo API", "Fullstack", False),
    ("JavaScript Closures", "MCQ", False), ("Second Highest Salary", "SQL", False),
    ("System Design: URL Shortener", "Theory", False), ("Behavioral: Conflict", "Theory", True), ("Async / Await", "MCQ", True),
]


# ------------------------------------------------------------------ list page
def filter_bar(active="All types", query=None, archived=False):
    types = ["All types", "Algo", "Fullstack", "MCQ", "SQL", "Theory"]
    pills = "".join(I("Pill / Active" if t == active else "Pill / Inactive", Label=t) for t in types)
    search = I("Search Input / Filled" if query else "Search Input", Placeholder=query or "Search challenges…", style="width:240px")
    seg = I("Segmented / 2 Options Active 2" if archived else "Segmented / 2 Options Active 1", **{"Option 1": "Active", "Option 2": "All"})
    return row("Filter Bar", row("Type Pills", pills, gap="8px") + row("Search And Scope", search + seg, gap="8px"),
               justify_content="space-between", gap="var(--spacing-md)")


def table(rows_):
    def th(t, w=None, right=False):
        return T(f"Col / {t}", t.upper(), 12, fg("text-secondary"), 600, lh=16, letter_spacing="0.36px",
                 **({"width": f"{w}px"} if w else {"flex_grow": "1"}), **({"text_align": "right"} if right else {}))
    head = row("Header Row", th("Title") + th("Type", 140) + th("Status", 130) + th("Actions", 170, True), gap="16px", padding="12px 16px",
               background=fg("bg-sider"), border_bottom="1px solid var(--color-border-light)")
    body = ""
    for title, t, arch in rows_:
        status = I("Tag Dot / Gray", Label="Archived") if arch else I("Tag Dot / Green", Label="Active")
        acts = I("Button / Default Small", Label="Edit") + ("" if arch else I("Button / Danger Small", Label="Archive"))
        body += row(f"Row / {title}", T("Cell / Title", title, 14, fg("text-primary"), 600, flex_grow="1") +
                    row("Cell / Type", type_tag(t), width="140px") + row("Cell / Status", status, width="130px") +
                    row("Cell / Actions", acts, width="170px", gap="8px", justify_content="flex-end"),
                    gap="16px", padding="12px 16px", border_bottom="1px solid var(--color-border-light)", background=fg("bg-elevated"))
    return col("Data Table", head + body, background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)", overflow="hidden")


def list_screen(name, route, content, **kw):
    return shell(name, route, "Challenges", "Challenges", "Reusable interview prompts across roles", content, actions=True, **kw)


def empty_block(title="No challenges match", desc="Adjust the filters or create a new challenge to get started."):
    return I("Empty State", Title=title, Description=desc, style="width:1052px") if False else col(
        "Empty State Block", I("Empty State", Title=title, Description=desc) + I("Button / Primary", Label="New challenge"), align_items="center", gap="8px",
        padding="var(--spacing-xl)", width="1052px")


def loading_block():
    return row("Loading", I("Spin / Large"), justify_content="center", padding="var(--spacing-xxl)", width="1052px")


def list_screens():
    out = []
    bar = lambda **k: filter_bar(**k)
    out.append(list_screen("Challenges List", "/challenges", bar() + table(ROWS[:7][:6] + [ROWS[7]][:0]), caption_extra="populated, active only") if False else
               list_screen("Challenges List", "/challenges", bar() + table(ROWS[:6]), caption_extra="populated (Active)"))
    out.append(list_screen("Challenges List Filtered", "/challenges", bar(active="MCQ", query="clos") + table([ROWS[3]]), caption_extra="type pill = MCQ, search 'clos'"))
    out.append(list_screen("Challenges List With Archived", "/challenges", bar(archived=True) + table(ROWS), caption_extra="scope = All, archived rows have no Archive button"))
    out.append(list_screen("Challenges List Empty", "/challenges", bar(active="SQL", query="zzz") + empty_block(), caption_extra="no match"))
    out.append(list_screen("Challenges List Loading", "/challenges", bar() + loading_block(), caption_extra="loading"))
    return out


# ------------------------------------------------------------------ editor pieces
MD_MCQ = ["## JavaScript Closures", "", "What does the following code log?", "", "```ts", "function makeCounter() {", "  let n = 0;", "  return () => ++n;", "}",
          "const c = makeCounter();", "console.log(c(), c(), c());", "```"]


def sel(value, w=None, disabled=False, ph=False):
    comp = "Select / Filled" if value else "Select"
    st = {"width": f"{w}px"} if w else {}
    if disabled:
        st["opacity"] = ".5"
    return I(comp, Value=value or "Select…", style=st or None)


def inp(value, w=None, err=False):
    st = f"width:{w}px" if w else None
    return I("Input / Error" if err else ("Input / Filled" if value else "Input"), Value=value or "", style=st)


def type_details(type_label, title, tmo=5, mem=256, edit=False, title_err=False):
    ctrl_w = "100%"
    return acard("Card / Type & details", "Type & details",
                 field("Field / Type", "Type", sel(type_label, 1004 - 24 - 0 if False else 560, disabled=edit), True) +
                 field("Field / Title", "Title", inp(title, 560, err=title_err), True, err="Please enter Title" if title_err else None) +
                 row("Limits", field("Field / Time limit", "Time limit (s)", I("Input Number", Value=str(tmo), style="width:120px"), True) +
                     field("Field / Memory", "Memory (MB)", I("Input Number", Value=str(mem), style="width:120px"), True), gap="16px", align_items="flex-start"))


def prompt_card(lines, err=False):
    return acard("Card / Prompt", "Prompt", field("Field / Prompt", "Prompt (Markdown)", md_editor(lines, 240), True))


def lang_card(lang="TypeScript", err=False):
    return acard("Card / Language & limits", "Language & limits",
                 field("Field / Language", "Language", sel(lang or "", 560) if lang else I("Select", Value="Select…", style="width:560px") if not err else I("Select", Value="Select…", style="width:560px"), True,
                       err="Please enter Language" if err else None))


def switch(on, txt):
    return row("Switch / " + txt, T("Switch Text", txt, 12, fg("text-inverse") if on else fg("text-inverse"), 400, lh=16), padding="0px 8px", height="22px", width="64px",
               justify_content="flex-end" if not on else "flex-start", border_radius="99px", background=fg("brand-primary") if on else fg("switch-off"))


def files_card(files):
    body = ""
    for path, contents, hidden in files:
        body += row(f"File Row / {path}", inp(path, 200) if False else I("Input / Filled", Value=path, style="width:200px") + I("Input / Filled", Value=contents, style="flex-grow:1") +
                    switch(hidden, "hidden" if hidden else "visible") + x_btn(), gap="8px")
    return acard("Card / Files", "Files", body + dashed_btn("+ Add file", "Button / Add file"))


def tests_card(tests):
    body = ""
    for n, inp_, exp, w, samp in tests:
        body += row(f"Test Row / {n}", I("Input / Filled", Value=n, style="width:120px") + I("Input / Filled", Value=inp_, style="flex-grow:1") +
                    I("Input / Filled", Value=exp, style="flex-grow:1") + I("Input Number", Value=str(w), style="width:70px") +
                    switch(samp, "sample" if samp else "-") + x_btn(), gap="8px")
    return acard("Card / Test cases", "Test cases", body + dashed_btn("+ Add test", "Button / Add test"))


def mcq_card(questions, warn=None, hint=True):
    body = T("Help", "Add as many questions as you need — each has its own options and exactly one correct answer. With a single question you can leave its text empty and let the prompt above do the talking.",
             12, fg("text-secondary"), lh=18)
    qs = ""
    for qi, (qtext, opts) in enumerate(questions):
        orow = ""
        for oi, (lab, ok) in enumerate(opts):
            L = "ABCDEFG"[oi]
            orow += row(f"Option Row / {L}", circle_letter(L, ok) + (T("Option Text", lab, 14, fg("text-primary"), flex_grow="1") if lab else T("Option Placeholder", f"Option {L}", 14, fg("text-placeholder"), flex_grow="1")) +
                        I("Checkbox / Checked" if ok else "Checkbox / Unchecked", Label="Correct") + x_btn(), gap="8px", padding="4px 8px",
                        border=f"1px solid {'var(--color-success)' if ok else 'var(--color-border)'}", border_radius="var(--radius-md)",
                        background="#f6ffed" if ok else "transparent")
        w = T("Warning", warn, 14, fg("warning")) if (warn and qi == 0) else ""
        qs += col(f"Question {qi + 1}", row("Question Head", T("Question Label", f"Question {qi + 1}", 14, fg("text-primary"), 600) +
                  (I("Input / Filled", Value=qtext, style="flex-grow:1") if qtext else I("Input", Value="Question text (required)", style="flex-grow:1")) + x_btn(), gap="8px") +
                  orow + w + dashed_btn("+ Add option", "Button / Add option", small=True), gap="8px", padding="8px 16px",
                  border="1px solid var(--color-border)", border_radius="var(--radius-md)")
    return acard("Card / MCQ questions", "MCQ questions", body + qs + dashed_btn("+ Add question", "Button / Add question", block=True))


def theory_card(qs, hint=False):
    body = ""
    for i, (cat, diff, q) in enumerate(qs, 1):
        body += row(f"Theory Row {i}", circle_letter(str(i), False) +
                    col("Fields", row("Cat And Diff", (T("Category", cat, 14, fg("text-primary"), flex_grow="1") if cat else T("Category Placeholder", "Category (e.g. JS Closures)", 14, fg("text-placeholder"), flex_grow="1")) +
                                      row("Diff Select", T("Diff", diff, 14, fg("text-primary")) + ic("caret-down", 12, fg("text-disabled"), "Caret"), gap="8px", width="110px", justify_content="space-between"),
                                      gap="8px") +
                        (T("Question", q, 14, fg("text-primary")) if q else T("Question Placeholder", "Question the recruiter asks", 14, fg("text-placeholder"))), gap="4px", flex_grow="1") + x_btn(),
                    gap="8px", padding="4px 8px", border="1px solid var(--color-border)", border_radius="var(--radius-md)", align_items="flex-start")
    if not qs:
        body += ""
    body += dashed_btn("+ Add question", "Button / Add question", block=True)
    if not qs:
        body += T("Hint", "Add at least one question — theory challenges require ≥1.", 14, fg("text-secondary"))
    return acard("Card / Theory questions", "Theory questions", body)


def submit(label_, disabled=False):
    return I("Ant Button / Primary", Label=label_, style="opacity:.5" if disabled else None)


def preview(title, prompt_nodes, body_nodes):
    def sec(t):
        return T("Section Label", t.upper(), 12, fg("text-secondary"), 600, lh=16, letter_spacing="0.48px")
    hint = lambda t: T("Empty Hint", t, 12, fg("text-disabled"), lh=16, font_style="italic")
    return col("Live Preview", T("Preview Label", "Live preview", 12, fg("text-primary"), lh=16) +
               T("Preview Title", title or "Untitled challenge", 20, fg("text-primary"), 600, lh=28) +
               col("Prompt Block", sec("Prompt") + prompt_nodes, gap="8px") + body_nodes,
               padding="var(--spacing-lg)", gap="var(--spacing-lg)", background=fg("bg-elevated"), border="1px solid var(--color-border-light)",
               border_radius="var(--radius-lg)")


def sec_label(t):
    return T("Section Label", t.upper(), 12, fg("text-secondary"), 600, lh=16, letter_spacing="0.48px")


def hint(t):
    return T("Empty Hint", t, 12, fg("text-disabled"), lh=16)


def md_rendered(h, para, code=None):
    kids = T("Heading", h, 24, fg("text-primary"), 600, lh=32) + box("Rule", height="1px", background=fg("border-light")) + T("Paragraph", para, 14, fg("text-primary"))
    if code:
        kids += col("Code Block", "".join(T("Line", l or "&nbsp;", 12, fg("text-primary"), lh=16) for l in code), padding="12px", background=fg("bg-layout"), border_radius="var(--radius-md)")
    return col("Rendered Markdown", kids, gap="12px")


def opt_preview(opts):
    return col("Options", "".join(row(f"Option / {i}", box("Radio", width="16px", height="16px", border_radius="99px", border="2px solid var(--color-border-strong)") +
                                      T("Label", o, 14, fg("text-primary"), flex_grow="1"), gap="8px", padding="10px 12px",
                                      border="1px solid var(--color-border-light)", border_radius="var(--radius-md)") for i, o in enumerate(opts)), gap="8px")


def two_pane(form_html, preview_html):
    return row("Two Pane", col("Form", form_html, gap="var(--spacing-lg)", width="615px") + col("Preview Pane", preview_html, width="385px"),
               gap="var(--spacing-lg)", align_items="flex-start")


def editor_page(title, subtitle, inner):
    return pcard("Card / Editor", T("Card Heading", title, 24, fg("text-primary"), 600, lh=32) + inner, gap="var(--spacing-md)", width="1052px")


def template_picker(applied=None):
    kids = T("Label", "Starter templates", 14, fg("text-primary"), 600) + I("Ant Button / Default", Label="Browse templates…") + I("Ant Button / Text", Label="Start blank")
    if applied:
        kids += I("Tag / Blue", Label=f"{applied} applied")
    kids += T("Help", "Templates pre-fill the form — everything stays editable.", 12, fg("text-secondary"), lh=16)
    return row("Template Picker", kids, gap="8px", flex_wrap="wrap")


def algo_form(title, with_files, errs=False):
    files = files_card([("solution.ts", "export function twoSum(nums: number[], target: number): number[] {…}", False)] if with_files else [])
    tests = tests_card([("basic", "[2,7,11,15], 9", "[0,1]", 1, True), ("negatives", "[-3,4,3,90], 0", "[0,2]", 2, False), ("no pair", "[1,2], 7", "[]", 1, False)] if with_files else [])
    return type_details("Algo", title, title_err=errs) + prompt_card(MD_MCQ[:1] and ["## Two Sum", "", "Given an array of integers nums and an integer target,", "return the indices of the two numbers that add up to target.", "", "```ts", "twoSum([2, 7, 11, 15], 9); // [0, 1]", "```"] if with_files else [""]) + \
        lang_card("TypeScript" if with_files else None, err=errs) + files + tests + submit("Create")


def build_new(errs=False, applied=None, filled=True):
    pv_prompt = md_rendered("Two Sum", "Given an array of integers nums and an integer target, return the indices of the two numbers that add up to target.") if filled else hint("Add a prompt to see it rendered here.")
    pv_body = col("Body", sec_label("Starter file") + (code_block("solution.ts", ["export function twoSum(nums: number[], target: number): number[] {", "  // TODO", "  return [];", "}"]) if filled else hint("No visible files yet — add one in the Files section.")) +
                  sec_label("Test cases") + (T("Tests", "Tests: basic, negatives, no pair", 14, fg("text-primary")) if filled else hint("No test cases yet.")), gap="8px")
    form = algo_form("Two Sum" if filled else "", filled, errs)
    return editor_page("New challenge", "", template_picker(applied) + two_pane(form, preview("Two Sum" if filled else "", pv_prompt, pv_body)))


def build_edit_mcq():
    form = (type_details("MCQ", "JavaScript Closures", edit=True) + prompt_card(MD_MCQ) + files_card([]) +
            mcq_card([("What does the code log when run once?", [("1 1 1", False), ("1 2 3", True), ("0 1 2", False), ("3 2 1", False)]),
                      ("Which keyword preserves n?", [("var", False), ("let", False)])], warn="Mark one option as correct."
                     if False else None) + submit("Save"))
    pv = preview("JavaScript Closures", md_rendered("JavaScript Closures", "What does the following code log?", MD_MCQ[6:11]),
                 col("Questions", sec_label("Question 1") + T("Q1", "What does the code log when run once?", 14, fg("text-primary")) + opt_preview(["1 1 1", "1 2 3", "0 1 2", "3 2 1"]) +
                     sec_label("Question 2") + T("Q2", "Which keyword preserves n?", 14, fg("text-primary")) + opt_preview(["var", "let"]), gap="8px"))
    return editor_page("Edit challenge", "", two_pane(form, pv))


def build_edit_mcq_validation():
    form = (type_details("MCQ", "", edit=True, title_err=True) + prompt_card(["## JavaScript Closures"]) + files_card([]) +
            mcq_card([("", [("1 1 1", False), ("1 2 3", False), ("", False)])], warn="Mark one option as correct.") + submit("Save"))
    pv = preview("Untitled challenge", md_rendered("JavaScript Closures", ""), col("Questions", sec_label("Options") + opt_preview(["1 1 1", "1 2 3", "Option 3"]), gap="8px"))
    return editor_page("Edit challenge", "", two_pane(form, pv))


def build_edit_theory(empty=False):
    qs = [] if empty else [("System Design", "Medium", "How would you design a URL shortener that handles 10k writes per second?"),
                           ("Databases", "Hard", "Explain how you would shard the mapping table."), ("", "Easy", "")]
    form = (type_details("Theory", "System Design: URL Shortener", edit=True) + prompt_card(["## System Design", "", "Discuss the trade-offs aloud."]) + files_card([]) +
            theory_card(qs) + submit("Save", disabled=empty))
    if empty:
        tp = hint("No questions yet — add at least one to make this a valid theory challenge.")
    else:
        def tq(i, cat, d, q, c):
            return row(f"Question {i}", T("Num", f"{i}.", 14, fg("text-secondary"), 700) + col("Body", row("Meta", T("Category", cat, 12, fg("text-secondary"), 600, lh=16) +
                       row("Diff", T("Diff Text", d, 12, c, 600, lh=16), padding="1px 7px", border_radius="99px", background="#f0f0f0"), gap="6px") + T("Text", q, 14, fg("text-primary")), gap="4px", flex_grow="1"),
                       gap="8px", padding="10px 12px", border="1px solid var(--color-border-light)", border_radius="var(--radius-md)")
        tp = col("Qs", tq(1, "System Design", "Medium", qs[0][2], fg("warning")) + tq(2, "Databases", "Hard", qs[1][2], fg("error")), gap="8px")
    pv = preview("System Design: URL Shortener", md_rendered("System Design", "Discuss the trade-offs aloud."), col("Questions", sec_label("Questions") + tp, gap="8px"))
    return editor_page("Edit challenge", "", two_pane(form, pv))


def build_edit_sql():
    sql = ["SELECT department, SUM(amount) AS revenue", "FROM sales", "GROUP BY department", "ORDER BY revenue DESC", "LIMIT :n;"]
    form = (type_details("SQL", "Top-N by Revenue", edit=True) + prompt_card(["## Top-N by Revenue", "", "Write a query returning the top N departments by revenue."]) +
            files_card([("solution.sql", "SELECT department, SUM(amount) …", False), ("schema.sql", "CREATE TABLE sales (…)", True)]) + submit("Save"))
    pv = preview("Top-N by Revenue", md_rendered("Top-N by Revenue", "Write a query returning the top N departments by revenue."),
                 col("SQL", sec_label("SQL") + code_block("solution.sql", sql), gap="8px"))
    return editor_page("Edit challenge", "", two_pane(form, pv))


def build_edit_fullstack():
    code = ["import express from 'express';", "const app = express();", "app.get('/todos', (_req, res) => res.json([]));", "app.listen(3000);"]
    form = (type_details("Fullstack", "REST Todo API", tmo=30, mem=512, edit=True) + prompt_card(["## REST Todo API", "", "Implement GET /todos and POST /todos."]) +
            lang_card("TypeScript") + files_card([("server.ts", "import express from 'express';…", False), ("server.test.ts", "describe('todos', …)", True)]) +
            tests_card([("lists todos", "GET /todos", "200 []", 1, True), ("creates todo", "POST /todos", "201", 2, False)]) + submit("Save"))
    pv = preview("REST Todo API", md_rendered("REST Todo API", "Implement GET /todos and POST /todos."),
                 col("Body", sec_label("Starter file") + code_block("server.ts", code) + sec_label("Test cases") + T("Tests", "Tests: lists todos, creates todo", 14, fg("text-primary")), gap="8px"))
    return editor_page("Edit challenge", "", two_pane(form, pv))


def build_loading():
    return row("Loading", I("Spin / Large"), justify_content="center", padding="var(--spacing-xxl)", width="1052px")


# ------------------------------------------------------------------ templates modal
TEMPLATES = [
    ("Algorithm", "Two Sum", "Classic array warm-up with a TS starter and 3 sample tests."),
    ("Algorithm", "Valid Parentheses", "Stack-based bracket matcher. Empty starter + sample tests."),
    ("Full-stack", "REST Todo API", "Express-style TS server stub with a sample happy-path test."),
    ("Full-stack", "Counter Component", "React counter component starter. No tests — UI focus."),
    ("Multiple choice", "JS Closures", "Four options, one correct — tests closure capture semantics."),
    ("Multiple choice", "Async / Await", "Pick the correct ordering of awaited console logs."),
    ("SQL", "Top-N by Revenue", "Schema note + prompt; candidate writes the ranking query."),
    ("SQL", "Second Highest Salary", "Classic windowing exercise. Returns null when there is none."),
    ("Theory", "System Design: URL Shortener", "Three questions across easy/medium/hard design depth."),
]
BADGE = {"Algorithm": ("brand-primary", "tag-blue-bg"), "Full-stack": ("avatar-purple", "tag-purple-bg"), "Multiple choice": ("avatar-teal", "tag-cyan-bg"),
         "SQL": ("warning", "tag-gold-bg"), "Theory": ("avatar-pink", "tag-red-bg")}


def tcard(kind, title, blurb):
    fgc, bgc = BADGE[kind]
    return col(f"Template Card / {title}", row("Head", row("Type Badge", box("Dot", width="8px", height="8px", border_radius="99px", background=fg(fgc)) + T("Label", kind, 12, fg(fgc), 600, lh=16),
                                                    gap="6px", padding="2px 8px", border_radius="99px", background=fg(bgc)), justify_content="space-between") +
               T("Title", title, 14, fg("text-primary"), 600) + T("Blurb", blurb, 12, fg("text-secondary"), lh=18),
               gap="4px", padding="var(--spacing-md)", background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)", box_shadow="var(--shadow-sm)", width="256px")


def templates_modal():
    seg = I("Segmented / 4 Options Active 1", **{"Option 1": "All", "Option 2": "Algorithm", "Option 3": "Full-stack", "Option 4": "Multiple choice"}) if False else \
        row("Type Filter", "".join(T(f"Opt {o}", o, 14, fg("text-primary") if o == "All" else fg("text-secondary"), 600 if o == "All" else 400, padding="4px 12px",
                                    **({"background": fg("bg-elevated"), "border_radius": "6px"} if o == "All" else {})) for o in ["All", "Algorithm", "Full-stack", "Multiple choice", "SQL", "Theory"]),
            padding="2px", background="#0000000f", border_radius="var(--radius-md)")
    grid = ""
    for i in range(0, len(TEMPLATES), 3):
        grid += row(f"Grid Row {i // 3 + 1}", "".join(tcard(*t) for t in TEMPLATES[i:i + 3]), gap="var(--spacing-md)")
    m = col("Modal / Starter Templates", row("Modal Head", T("Title", "Starter templates", 16, fg("text-primary"), 600, lh=24) + ic("x", 16, fg("text-secondary"), "Close"), justify_content="space-between") +
            row("Toolbar", I("Input / Search", Value="Search templates", style="width:280px") + seg, gap="8px") + col("Grid", grid, gap="var(--spacing-md)"),
            width="860px", padding="20px 24px", gap="var(--spacing-md)", background=fg("bg-elevated"), border_radius="var(--radius-lg)", box_shadow="var(--shadow-lg)")
    return overlay_html(m)


def build_all(P, only=None):
    items = []
    for f in list_screens():
        items.append(f)
    new_full = lambda **k: shell("Challenge New", "/challenges/new", "Challenges", "New challenge", "Author a reusable interview prompt", build_new(**k), wide=True) if False else None
    S = lambda name, route, title, sub, content, **kw: shell(name, route, "Challenges", title, sub, content, **{"height": 0, **kw})
    items += [
        S("Challenge New Blank", "/challenges/new", "New challenge", "Author a reusable interview prompt", build_new(filled=False), caption_extra="blank (Algo)"),
        S("Challenge New Template Applied", "/challenges/new", "New challenge", "Author a reusable interview prompt", build_new(applied="Two Sum"), caption_extra="template applied"),
        S("Challenge New Validation Errors", "/challenges/new", "New challenge", "Author a reusable interview prompt", build_new(errs=True, filled=False), caption_extra="submit with empty required fields"),
        S("Challenge New Templates Modal", "/challenges/new", "New challenge", "Author a reusable interview prompt", build_new(filled=False), overlay=templates_modal(), height=900, caption_extra="Browse templates…"),
        S("Challenge Edit Mcq", "/challenges/[id]/edit", "Edit challenge", "Update prompt, rubric & metadata", build_edit_mcq(), caption_extra="MCQ"),
        S("Challenge Edit Mcq Validation", "/challenges/[id]/edit", "Edit challenge", "Update prompt, rubric & metadata", build_edit_mcq_validation(), caption_extra="MCQ, no correct option, empty title"),
        S("Challenge Edit Theory", "/challenges/[id]/edit", "Edit challenge", "Update prompt, rubric & metadata", build_edit_theory(), caption_extra="Theory"),
        S("Challenge Edit Theory Empty", "/challenges/[id]/edit", "Edit challenge", "Update prompt, rubric & metadata", build_edit_theory(True), caption_extra="Theory without questions: Save disabled"),
        S("Challenge Edit Sql", "/challenges/[id]/edit", "Edit challenge", "Update prompt, rubric & metadata", build_edit_sql(), caption_extra="SQL"),
        S("Challenge Edit Fullstack", "/challenges/[id]/edit", "Edit challenge", "Update prompt, rubric & metadata", build_edit_fullstack(), caption_extra="Fullstack"),
        S("Challenge Edit Loading", "/challenges/[id]/edit", "Edit challenge", "Loading", build_loading(), height=900, caption_extra="loading"),
    ]
    return items


if __name__ == "__main__":
    fid, P = init()
    ensure_components(P)
    delete_section(P, SECTION)
    items = build_all(P)
    sec = section(SECTION, "Admin / Challenges", items, X0, Y0, cols=2)
    ids = write(P, "root", sec)
    print("section", ids)
