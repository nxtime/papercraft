"""Main components of the Candidate page (IDE pieces, system-check rows, session cards). Names: `Group / Variant`."""
import html as _h
import re

from cand_common import *

SPECS, GROUPS, NOTES = [], {}, {}


def reg(group, name, html, note=""):
    SPECS.append((name, html))
    GROUPS.setdefault(group, []).append(name)
    NOTES[name] = note


def D(c):
    return f"var(--color-ide-{c})"


BRAND = fg("brand-primary")
TOOLBAR_W = MAIN_W
CENTER_W = 666     # 1192 - 210 file tree - 316 rail
BODY_H = 848       # 900 - 52 toolbar


# ------------------------------------------------------------------ IDE toolbar
def tb_tab(icon, label, active):
    c = fg("text-inverse") if active else D("text-muted")
    return col(f"Tab / {label}", row("Tab Body", ic(icon, 14, c, "Tab Icon") + T("Label", label, 14, c, 600 if active else 400), height="50px", padding="0 18px", gap="8px") +
               box("Ink Bar", height="2px", background=BRAND if active else "transparent"))


def timer(txt="44:12", low=False):
    c = D("fail") if low else D("text")
    return row("Timer Pill", ic("clock", 14, c, "Timer Icon") + T("Timer", txt, 14, c, 700), gap="8px", padding="5px 12px", background=D("deep"), border_radius="6px",
               border=f"1px solid {D('fail-border') if low else D('line')}")


def toolbar(name, active, low=False, timer_txt="44:12"):
    left = (row("Exit", ic("arrow-left", 14, D("text-faint"), "Exit Icon") + T("Exit Label", "Exit", 14, D("text-faint")), gap="6px", padding="0 12px 0 0") +
            box("Divider", width="1px", height="20px", background=D("sep")) + box("Spacer", width="16px"))
    tabs = "".join(tb_tab(i, l, l == active) for i, l in [("code", "Code"), ("flow-arrow", "Diagrams"), ("selection", "Whiteboard")])
    right = timer(timer_txt, low)
    if active == "Code":
        right += box("Gap", width="8px") + I("Button / Dark Small", "Run Button", Label="Run") + box("Gap", width="8px") + I("Button / Primary Small", "Submit Button", Label="Submit")
    return row(name, left + tabs + box("Spacer", flex_grow="1") + right + box("Pad", width="4px"), width=f"{TOOLBAR_W}px", height="52px", padding="0 8px 0 16px",
               background=D("panel"), border=f"1px solid {D('deep')}")


reg("IDE toolbar", "IDE Toolbar / Code", toolbar("IDE Toolbar / Code", "Code"), "challenge toolbar, Code tab, Run + Submit")
reg("IDE toolbar", "IDE Toolbar / Code Low Time", toolbar("IDE Toolbar / Code Low Time", "Code", True, "04:12"), "timer under 5 min turns red")
reg("IDE toolbar", "IDE Toolbar / Diagrams", toolbar("IDE Toolbar / Diagrams", "Diagrams"), "Diagrams tab")
reg("IDE toolbar", "IDE Toolbar / Whiteboard", toolbar("IDE Toolbar / Whiteboard", "Whiteboard"), "Whiteboard tab")


# ------------------------------------------------------------------ File tree
def tree_file(name, active, depth=1, ext_color="syntax-type"):
    c = fg("text-inverse") if active else D("text-dim")
    return row(f"File / {name}", ic("diamond", 10, fg(ext_color), "File Icon") + T("File Name", name, 14, c), gap="8px", padding=f"5px 8px 5px {depth * 14 + 10}px", border_radius="var(--radius-sm)",
               background=D("active") if active else "transparent")


def file_tree(name, active):
    files = [("SearchBox.tsx", "syntax-type"), ("SearchBox.test.tsx", "syntax-type"), ("useDebounce.ts", "syntax-type"), ("styles.css", "ide-css")]
    tree = (row("Folder / src", ic("caret-down", 10, D("text-muted"), "Folder Icon") + T("Folder Name", "src", 14, D("text-muted"), 600), gap="6px", padding="5px 8px 5px 10px") +
            "".join(tree_file(n, n == active, 1, c) for n, c in files))
    return col(name, T("Explorer Title", "EXPLORER", 12, D("text-faint"), 600, letter_spacing="0.6px", padding="10px 14px") + col("Tree", tree, padding="8px", gap="0px"),
               width="210px", height=f"{BODY_H}px", background=D("panel"), border=f"1px solid {D('deep')}")


reg("File tree", "File Tree / SearchBox Active", file_tree("File Tree / SearchBox Active", "SearchBox.tsx"), "Explorer, SearchBox.tsx selected")
reg("File tree", "File Tree / Test Active", file_tree("File Tree / Test Active", "SearchBox.test.tsx"), "Explorer, test file selected")


def editor_tab(name, active):
    c = fg("text-inverse") if active else D("text-muted")
    return col(f"Editor Tab / {name}", box("Ink Bar", height="2px", background=BRAND if active else "transparent") +
               row("Tab Body", ic("diamond", 10, fg("syntax-type"), "File Icon") + T("File Name", name, 14, c) + (ic("x", 12, D("text-faint"), "Close Icon") if active else ""), gap="8px", padding="7px 14px 8px"),
               background=fg("code-bg") if active else "transparent", border=f"1px solid {D('deep')}")


for nm, (a, b) in {"SearchBox Active": ("SearchBox.tsx", "useDebounce.ts"), "Debounce Active": ("useDebounce.ts", "SearchBox.tsx")}.items():
    reg("Editor tabs", f"Editor Tabs / {nm}",
        row(f"Editor Tabs / {nm}", editor_tab(a, True) + editor_tab(b, False), width=f"{CENTER_W}px", height="37px", background=D("panel"), border=f"1px solid {D('deep')}", align_items="stretch"),
        "open file tabs")

# ------------------------------------------------------------------ Code editor (port of CodeEditor tokenizer)
KW = re.compile(r"^(?:import|from|export|default|const|let|var|function|return|if|else|for|while|do|switch|case|break|continue|await|async|new|class|interface|enum|extends|implements|public|private|protected|readonly|type|of|in|try|catch|finally|throw|typeof|instanceof|null|undefined|true|false|void|this|super|yield|as|static|get|set)\b")
RULES = [("comment", re.compile(r"^//.*")), ("string", re.compile(r"^`(?:[^`\\]|\\.)*`?|^\"(?:[^\"\\]|\\.)*\"?|^'(?:[^'\\]|\\.)*'?")), ("keyword", KW),
         ("type", re.compile(r"^[A-Z][A-Za-z0-9_]*")), ("number", re.compile(r"^\d+(?:\.\d+)?")), ("fn", re.compile(r"^[a-zA-Z_$][\w$]*(?=\s*\()")),
         ("ident", re.compile(r"^[a-zA-Z_$][\w$]*")), ("ws", re.compile(r"^\s+"))]
TOK_COLOR = {"comment": "syntax-comment", "string": "syntax-string", "keyword": "syntax-keyword", "type": "syntax-type", "number": "syntax-number", "fn": "syntax-fn"}


def code_line_html(line):
    out, i = [], 0
    while i < len(line):
        s = line[i:]
        for cls, rx in RULES:
            m = rx.match(s)
            if m:
                t = m.group(0)
                out.append((cls, t))
                i += len(t)
                break
        else:
            out.append(("punct", line[i]))
            i += 1
    return "".join(f'<span style="color:var(--color-{TOK_COLOR[c]})">{_h.escape(t)}</span>' if c in TOK_COLOR else _h.escape(t) for c, t in out)


SOLUTION = """import { useEffect, useRef, useState } from 'react';
import { useDebounce } from './useDebounce';

type Result = { id: string; title: string };

export function SearchBox({ fetchResults }: Props) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<Result[]>([]);
  const [loading, setLoading] = useState(false);
  const debounced = useDebounce(query, 300);
  const latest = useRef(0);

  useEffect(() => {
    // ignore responses that arrive out of order
    const ticket = ++latest.current;
    if (!debounced) return setResults([]);
    setLoading(true);
    fetchResults(debounced)
      .then((rows) => {
        if (ticket === latest.current) setResults(rows);
      })
      .finally(() => setLoading(false));
  }, [debounced, fetchResults]);

  return <input value={query} onChange={(e) => setQuery(e.target.value)} />;
}"""
DEBOUNCE = """import { useEffect, useState } from 'react';

export function useDebounce<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);

  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(id);
  }, [value, delayMs]);

  return debounced;
}"""


def code_editor(name, src, h=609, w=CENTER_W, readonly=False):
    lines = src.split("\n")
    lh, fs = 20, 13
    gutter = col("Gutter", "".join(T(f"Ln {i}", str(i), fs, fg("syntax-gutter"), lh=lh, text_align="right", width="26px") for i in range(1, len(lines) + 1)),
                 padding="12px 10px 12px 0", width="40px")
    body = []
    for i, ln in enumerate(lines, 1):
        ind = len(ln) - len(ln.lstrip(" "))
        body.append(box(f"Line {i}", code_line_html(ln.strip()) or "&nbsp;", font_size=f"{fs}px", line_height=f"{lh}px", color=fg("code-fg"), font_weight=400,
                        padding_left=f"{ind * 4}px", height=f"{lh}px"))
    return row(name, gutter + col("Code", "".join(body), padding="12px 16px", gap="0px"), width=f"{w}px", height=f"{h}px", background=D("deep"), overflow="hidden", align_items="flex-start")


reg("Code editor", "Code Editor / SearchBox", code_editor("Code Editor / SearchBox", SOLUTION), "dark code area: gutter + highlighted lines (CodeEditor.tsx tokenizer)")
reg("Code editor", "Code Editor / useDebounce", code_editor("Code Editor / useDebounce", DEBOUNCE), "second file")
reg("Code editor", "Code Editor / Empty", code_editor("Code Editor / Empty", "// Write your solution here\n"), "seeded when the challenge ships no files")


# ------------------------------------------------------------------ Console
def ctab(label, active):
    c = fg("text-inverse") if active else D("text-muted")
    return col(f"Console Tab / {label}", row("Tab Body", T("Label", label, 12, c), padding="8px 16px 7px") + box("Ink Bar", height="2px", background=BRAND if active else "transparent"))


CONS = {"sys": "ide-text-muted", "pass": "ide-pass", "fail": "ide-fail", "info": "code-fg", "warn": "ide-warn"}


def console(name, lines, running=False, empty=False):
    head = row("Console Tabs", ctab("Console", True) + ctab("Tests", False) + ctab("Terminal", False) + box("Spacer", flex_grow="1") + T("Clear", "Clear", 12, D("text-faint"), padding="0 14px"),
               background=D("panel"), border=f"1px solid {D('deep')}", height="34px")
    out = "".join(T(f"Output {i}", t, 13, fg(CONS[k]), lh=20) for i, (k, t) in enumerate(lines, 1))
    if empty:
        out = T("Output 1", "$ Console ready — run the tests to see output.", 13, D("text-faint"), lh=20)
    if running:
        out += row("Running", ic("cursor-text", 12, D("text-muted"), "Cursor Icon") + T("Running Label", "running…", 13, D("text-muted"), lh=20), gap="6px")
    return col(name, head + col("Output", out, padding="10px 16px", gap="0px", background=D("console"), height="162px", overflow="hidden"), width=f"{CENTER_W}px", height="196px")


RUN_HEAD = [("sys", "$ npm test"), ("sys", "vitest run")]
PASS = [("info", "  ✓ debounces input by 300ms (312ms)"), ("info", "  ✓ cancels stale requests (88ms)"), ("info", "  ✓ shows loading state (24ms)"),
        ("info", "  ✓ renders empty state (19ms)"), ("pass", "  ✓ handles out-of-order responses (41ms)"), ("info", "  ✓ cleans up on unmount (12ms)")]
reg("Console", "Console / Empty", console("Console / Empty", [], empty=True), "no output yet")
reg("Console", "Console / Running", console("Console / Running", RUN_HEAD + PASS[:3], running=True), "tests streaming")
reg("Console", "Console / Passed", console("Console / Passed", PASS[2:] + [("pass", "  Test Files  1 passed (1)"), ("pass", "       Tests  6 passed (6)"), ("pass", "    Duration  2.05s")]), "all tests pass")
reg("Console", "Console / Failed", console("Console / Failed", PASS[:2] + [("fail", "  ✗ handles out-of-order responses (41ms)"), ("fail", "    AssertionError: expected 'a' to equal 'ab'"),
                                                                   ("info", "  ✓ cleans up on unmount (12ms)"), ("fail", "  Tests  1 failed | 5 passed (6)")]), "a failing test")

# ------------------------------------------------------------------ Problem rail
PROMPT_P1 = "Build a SearchBox React component that queries a remote API as the user types."
PROMPT_P2 = "Requests must be debounced by 300ms, stale in-flight requests must be ignored, and the UI must show loading and empty states. Make the provided tests pass."


def problem_rail(name, open_=True):
    chips = row("Chips", T("Language Chip", "TypeScript", 12, fg("syntax-type"), lh=16, padding="2px 8px", background=D("blue-bg"), border_radius="var(--radius-sm)") +
                row("Time Chip", ic("clock", 12, D("text-muted"), "Clock Icon") + T("Time Label", "45m", 12, D("text-muted"), lh=16), gap="4px", padding="2px 8px", background=D("chip"), border_radius="var(--radius-sm)"), gap="8px")
    head = col("Rail Header", T("Title", "Debounced search box", 16, fg("text-inverse"), 700) + chips, gap="8px", padding="14px 16px 12px", border=f"1px solid {D('line')}")
    acc_h = row("Accordion Header", ic("caret-down" if open_ else "caret-right", 12, D("text"), "Caret Icon") + T("Accordion Title", "Problem brief", 14, "var(--color-ide-text)", 600, flex_grow="1"), gap="8px", padding="12px 16px")
    acc_b = col("Accordion Body", T("Brief", PROMPT_P1, 14, D("text-dim"), lh=22) + T("Brief 2", PROMPT_P2, 14, D("text-dim"), lh=22), gap="12px", padding="0 16px 16px") if open_ else ""
    return col(name, head + col("Accordion", acc_h + acc_b, border=f"1px solid {D('line')}"), width="316px", height=f"{BODY_H}px", background=D("rail"))


reg("Problem rail", "Problem Rail / Brief", problem_rail("Problem Rail / Brief"), "right rail, brief open")
reg("Problem rail", "Problem Rail / Collapsed", problem_rail("Problem Rail / Collapsed", False), "brief collapsed")

# ------------------------------------------------------------------ Diagram and whiteboard toolbars
TEMPL = ["System design", "Sequence", "Class diagram", "State machine"]
for i, act in enumerate(TEMPL):
    chips = "".join(T(f"Template / {t}", t, 12, fg("text-inverse") if t == act else D("text-muted"), lh=16, padding="4px 12px", border_radius="var(--radius-sm)",
                      background=BRAND if t == act else D("line")) for t in TEMPL)
    reg("Diagram editor", f"Diagram Toolbar / {act}", row(f"Diagram Toolbar / {act}", T("Template Label", "Template:", 12, D("text-faint"), lh=16) + chips, gap="6px", width=f"{CENTER_W}px",
        padding="8px 14px", background=D("panel"), border=f"1px solid {D('deep')}"), "diagram template chips")

WB_COLORS = ["wb-white", "wb-blue", "ide-pass", "ide-fail", "ide-warn", "wb-purple", "wb-orange"]
WB_TOOLS = [("Select", "cursor"), ("Rectangle", "rectangle"), ("Ellipse", "circle"), ("Arrow", "arrow-up-right"), ("Freehand", "pencil-simple"), ("Text", "text-t")]


def wb_button(label, c, bg, icon=None):
    return row(f"Button / {label}", (ic(icon, 12, c, "Icon") if icon else "") + T("Label", label, 12, c, lh=16), gap="4px", padding="5px 10px", border_radius="6px", background=bg)


def wb_toolbar(name, active, color_idx=0, selected=False):
    tools = "".join(row(f"Tool / {t}", ic(i, 18, fg("text-inverse") if t == active else fg("wb-text"), "Tool Icon"), justify_content="center", width="34px", height="34px", border_radius="7px",
                        background=BRAND if t == active else fg("wb-button")) for t, i in WB_TOOLS)
    dots = "".join(box(f"Color / {c}", width="22px", height="22px", border_radius="99px", background=fg(c), border=f"2px solid {fg('text-inverse') if k == color_idx else 'transparent'}")
                   for k, c in enumerate(WB_COLORS))
    right = (wb_button("Delete", fg("ide-fail"), D("fail-bg"), "x") if selected else "") + wb_button("Undo", fg("wb-text"), fg("wb-button"), "arrow-counter-clockwise") + wb_button("Clear", fg("wb-text"), fg("wb-button"))
    return row(name, tools + box("Divider", width="1px", height="26px", background=D("sep")) + dots + box("Spacer", flex_grow="1") + right, gap="6px", width=f"{CENTER_W + 316}px",
               padding="8px 12px", background=fg("wb-toolbar"), border=f"1px solid {fg('wb-border')}")


reg("Whiteboard", "Whiteboard Toolbar / Rectangle", wb_toolbar("Whiteboard Toolbar / Rectangle", "Rectangle"), "tools, colors, undo, clear")
reg("Whiteboard", "Whiteboard Toolbar / Select", wb_toolbar("Whiteboard Toolbar / Select", "Select", 0, True), "select tool with a selected shape (Delete shown)")
reg("Whiteboard", "Whiteboard Toolbar / Arrow", wb_toolbar("Whiteboard Toolbar / Arrow", "Arrow", 1), "arrow tool, blue")


# ------------------------------------------------------------------ Theory phase
def pips(states):
    out = ""
    for i, s in enumerate(states):
        out += box(f"Pip {i + 1}", width="28px" if s == "cur" else "8px", height="6px", border_radius="3px",
                   background={"cur": fg("avatar-teal"), "done": fg("ide-pass"), "todo": fg("ide-line")}[s])
    return out


def theory_toolbar(name, states, count, timer_txt):
    return row(name, row("Exit", ic("arrow-left", 14, D("text-faint"), "Exit Icon") + T("Exit Label", "Exit", 14, D("text-faint")), gap="6px") + box("Divider", width="1px", height="20px", background=D("sep")) +
               T("Phase Label", "RECRUITER SCREEN", 12, fg("avatar-teal"), 700, letter_spacing="0.8px") +
               row("Progress", pips(states) + T("Count", count, 12, D("text-faint"), padding_left="6px"), gap="4px", flex_grow="1") + timer(timer_txt),
               gap="12px", width=f"{TOOLBAR_W}px", height="52px", padding="0 20px", background=D("panel"), border=f"1px solid {D('deep')}")


reg("Theory", "Theory Toolbar / Q2 of 5", theory_toolbar("Theory Toolbar / Q2 of 5", ["done", "cur", "todo", "todo", "todo"], "1/5", "27:45"), "question progress pips + timer")
reg("Theory", "Theory Toolbar / Q5 of 5", theory_toolbar("Theory Toolbar / Q5 of 5", ["done", "done", "done", "done", "cur"], "4/5", "08:20"), "last question")


def chip(name, label, c, bg):
    return row(name, T("Label", label, 12, fg(c), 600, lh=16), padding="3px 12px", background=bg, border_radius="99px")


reg("Theory", "Theory Chip / Category", chip("Theory Chip / Category", "Behavioral", "avatar-teal", D("teal-bg")), "category pill")
reg("Theory", "Theory Chip / Easy", chip("Theory Chip / Easy", "Easy", "ide-pass", D("pass-bg")), "difficulty")
reg("Theory", "Theory Chip / Medium", chip("Theory Chip / Medium", "Medium", "ide-warn", D("warn-bg")), "difficulty")
reg("Theory", "Theory Chip / Hard", chip("Theory Chip / Hard", "Hard", "ide-fail", D("fail-bg")), "difficulty")


def tbtn(name, label, c, bg, border="transparent", w=None, weight=500, icon=None, after=False):
    ico = ic(icon, 14, c, "Icon") if icon else ""
    kids = (T("Label", label, 14, c, weight) + ico) if after else (ico + T("Label", label, 14, c, weight))
    return row(name, kids, gap="6px", padding="9px 22px", border_radius="6px", background=bg, border=f"1px solid {border}", justify_content="center")


reg("Theory", "Theory Button / Prev", tbtn("Theory Button / Prev", "Prev", fg("ide-text-dim"), "transparent", D("line"), icon="arrow-left"), "outlined")
reg("Theory", "Theory Button / Prev Disabled", tbtn("Theory Button / Prev Disabled", "Prev", fg("ide-line"), "transparent", D("line"), icon="arrow-left"), "first question")
reg("Theory", "Theory Button / Next", tbtn("Theory Button / Next", "Next", fg("text-inverse"), fg("avatar-teal"), weight=600, icon="arrow-right", after=True), "teal")
reg("Theory", "Theory Button / Submit Partial", tbtn("Theory Button / Submit Partial", "Submit (3/5 answered)", D("submit-fg"), D("submit-bg"), "#2a4060", weight=700), "not all answered")
reg("Theory", "Theory Button / Submit All", tbtn("Theory Button / Submit All", "Submit answers", fg("text-inverse"), BRAND, BRAND, weight=700, icon="check", after=True), "all answered")


def answer(name, txt, filled):
    return col(name, T("Answer", txt, 14, fg("code-fg") if filled else D("text-faint"), lh=24), width="704px", height="260px", padding="18px", background=D("input"), border_radius="var(--radius-md)",
               border=f"1px solid {D('pass-border') if filled else D('line')}")


reg("Theory", "Theory Answer / Empty", answer("Theory Answer / Empty", "Type your answer here — explain your reasoning clearly.", False), "textarea placeholder")
reg("Theory", "Theory Answer / Filled", answer("Theory Answer / Filled", "I would start by clarifying the team's goals and constraints, then walk the interviewer through a concrete example where I disagreed with a peer and how we reached a decision together.", True), "typed answer (>= 8 words: green border)")

# ------------------------------------------------------------------ Session home
STEPS = ["Sign in", "System check", "Coding", "Submit"]


def progress(name, cur):
    items = ""
    for i, s in enumerate(STEPS):
        done, now = i < cur, i == cur
        if done:
            inner = ic("check", 14, fg("text-inverse"), "Check Icon")
        else:
            inner = T("Step Number", str(i + 1), 12, fg("text-inverse") if now else fg("text-disabled"), 700, lh=16)
        dot = row("Step Dot", inner, justify_content="center", width="28px", height="28px", border_radius="99px",
                  background=fg("success") if done else BRAND if now else fg("bg-elevated"), border="none" if (done or now) else f"2px solid {fg('border')}")
        items += col(f"Step / {s}", dot + T("Step Label", s, 12, fg("text-primary") if now else fg("text-secondary"), 600 if now else 400, lh=16, text_align="center"), align_items="center", gap="8px", flex_grow="1")
    line = row("Track", box("Line", height="2px", width="100%", background=fg("border-light")), position="absolute", left="76px", right="76px", top="13px")
    return col(name, T("Progress Title", "Your progress", 16, weight=600) + row("Steps", line + items, align_items="flex-start", position="relative", width="100%", gap="0px"), width="980px", gap="18px", padding="var(--spacing-lg)",
               background=fg("bg-elevated"), border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)", box_shadow="var(--shadow-sm)")


reg("Session home", "Progress Tracker / System check", progress("Progress Tracker / System check", 1), "step 2 current")
reg("Session home", "Progress Tracker / Coding", progress("Progress Tracker / Coding", 2), "step 3 current")


def check_item(name, label, ok):
    dot = row("Dot", ic("check", 12, fg("text-inverse"), "Check Icon") if ok else "", justify_content="center", width="22px", height="22px", border_radius="99px", background=fg("success") if ok else fg("bg-layout"))
    return row(name, dot + T("Label", label, 14, fg("text-primary"), flex_grow="1") + T("Status", "Ready" if ok else "Stub", 12, fg("success") if ok else fg("text-disabled"), 600, lh=16),
               gap="12px", padding="12px 0", width="334px", border="1px solid transparent")


reg("Session home", "Check Item / Ready", check_item("Check Item / Ready", "Browser compatible", True), "Ready")
reg("Session home", "Check Item / Stub", check_item("Check Item / Stub", "Camera access", False), "not ready")

TIPS = {"Read carefully": ("target", "Take 5 minutes to fully understand the problem before you start typing."),
        "Think aloud": ("chat-circle-dots", "Verbalize your approach. Interviewers want to hear how you reason."),
        "Manage time": ("timer", "Plan to leave 10 minutes at the end for testing and cleanup.")}
for t, (icn, d) in TIPS.items():
    reg("Session home", f"Tip Card / {t}", col(f"Tip Card / {t}", ic(icn, 22, fg("brand-primary"), "Tip Icon") + T("Tip Title", t, 14, weight=600) + T("Tip Desc", d, 12, fg("text-secondary"), lh=19),
                                                width="316px", gap="8px", padding="var(--spacing-md)", background=fg("bg-elevated"), border="1px solid var(--color-border-light)",
                                                border_radius="var(--radius-lg)", box_shadow="var(--shadow-sm)"), "tip")

# ------------------------------------------------------------------ System check (setup)
SC = {"Ready": ("syscheck-ready", "check", fg("syscheck-ready"), fg("syscheck-ready-bg")), "Checking": ("brand-primary", "dots-three", BRAND, fg("focus-ring")),
      "Failed": ("syscheck-fail", "warning", fg("syscheck-fail"), "transparent"), "Pending": ("border-light", None, fg("text-disabled"), fg("border-light"))}
BADGE = {"Ready": "Ready", "Checking": "Checking…", "Failed": "Action needed", "Pending": "—"}


def sc_row(name, state, title, desc):
    ind_bg, icn, bfg, bbg = SC[state]
    ind = row("Indicator", ic(icn, 14, fg("text-inverse"), "State Icon") if icn else "", justify_content="center", width="26px", height="26px", border_radius="99px", background=fg(ind_bg))
    badge = row("Badge", T("Badge Label", BADGE[state], 12, bfg, 600, lh=16), padding="3px 10px", border_radius="5px", background=bbg)
    retry = row("Retry", T("Retry Label", "Retry", 12, fg("text-primary"), 500, lh=16), padding="3px 10px", border_radius="5px", background=fg("bg-elevated"), border=f"1px solid {fg('border')}") if state == "Failed" else ""
    return row(name, ind + col("Texts", T("Title", title, 14, weight=600, lh=18) + T("Desc", desc, 12, fg("text-secondary"), lh=16), flex_grow="1", gap="2px") + row("Status", badge + retry, gap="8px"),
               gap="14px", width="360px", padding="14px 0", border="1px solid transparent")


for st in SC:
    reg("System check", f"Syscheck Row / {st}", sc_row(f"Syscheck Row / {st}", st, "Browser compatibility", "Chrome, Edge, or Firefox — up to date"), f"check row, {st}")

reg("System check", "Syscheck Step / Done", row("Syscheck Step / Done", row("Step Dot", ic("check", 12, fg("text-inverse"), "Check Icon"), justify_content="center", width="22px", height="22px", border_radius="99px", background=fg("syscheck-ready")) +
                                                T("Step Label", "Account setup", 14, fg("syscheck-ready"), 500), gap="8px"), "header step done")
reg("System check", "Syscheck Step / Active", row("Syscheck Step / Active", row("Step Dot", T("Step Number", "2", 12, fg("text-inverse"), 700, lh=16), justify_content="center", width="22px", height="22px", border_radius="99px", background=BRAND) +
                                                  T("Step Label", "System check", 14, BRAND, 500), gap="8px"), "header step active")

reg("Toast", "Message / Success", row("Message / Success", ic("check-circle", 16, fg("success"), "Success Icon") + T("Message", "Submission recorded", 14), gap="8px", padding="9px 12px",
                                       background=fg("bg-elevated"), border_radius="var(--radius-md)", box_shadow="var(--shadow-md)"), "antd message.success")

LIB_NAMES = [n for n, _ in SPECS]


def build(P, x0, y0):
    ids, y = place_components(P, SPECS, x0, y0, 6000, GROUPS)
    return ids, y
