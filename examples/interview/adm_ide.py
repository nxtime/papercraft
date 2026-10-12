"""IDE-style read-only surfaces shared by Live Session and Review (CodeEditor, Mermaid preview, Whiteboard, Theory view)."""
import re
from adm_common import *

TOK = {"comment": "var(--color-tok-comment)", "string": "var(--color-tok-string)", "keyword": "var(--color-tok-keyword)",
       "type": "var(--color-tok-type)", "number": "var(--color-tok-number)", "fn": "var(--color-tok-fn)", "ident": "var(--color-code-fg)"}
KW = re.compile(r"^(?:import|from|export|default|const|let|var|function|return|if|else|for|while|await|async|new|class|interface|type|of|in|try|catch|throw|null|undefined|true|false|void|this)\b")
RULES = [("comment", re.compile(r"^//.*")), ("string", re.compile(r"""^`[^`]*`?|^"[^"]*"?|^'[^']*'?""")), ("keyword", KW),
         ("type", re.compile(r"^[A-Z][A-Za-z0-9_]*")), ("number", re.compile(r"^\d+(?:\.\d+)?")),
         ("fn", re.compile(r"^[a-zA-Z_$][\w$]*(?=\s*\()")), ("ident", re.compile(r"^[a-zA-Z_$][\w$]*")), ("ws", re.compile(r"^\s+"))]


def esc(t):
    return t.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace(" ", "&nbsp;")


def tokenize(line):
    out, i = "", 0
    while i < len(line):
        s = line[i:]
        for cls, rx in RULES:
            m = rx.match(s)
            if m:
                t = m.group(0)
                out += esc(t) if cls == "ws" else f'<span style="color:{TOK[cls]}">{esc(t)}</span>'
                i += len(t)
                break
        else:
            out += esc(line[i]); i += 1
    return out


def code_editor(code, height=None, size=13, name="Code Editor"):
    """Read-only CodeEditor: gutter + highlighted lines on color-code-bg."""
    lines = code.split("\n")
    lh = round(size * 1.55)
    gut = "".join(T(f"Ln {i+1}", str(i + 1), size, "var(--color-ide-gutter)", lh=lh, text_align="right") for i in range(len(lines)))
    src = "".join(box(f"Line {i+1}", tokenize(l) or "&nbsp;", font_size=f"{size}px", line_height=f"{lh}px", color="var(--color-code-fg)") for i, l in enumerate(lines))
    st = dict(background_color="var(--color-code-bg)", width="100%", overflow="hidden", padding="12px")
    if height:
        st["height"] = f"{height}px"
    return row(name, col("Gutter", gut, width="32px", flex_shrink="0") + col("Source", src, flex_grow="1", padding="0 16px"), align_items="flex-start", **st)


TWO_SUM = """// Two Sum: one pass with a hash map, O(n)
export function twoSum(nums: number[], target: number): number[] {
  const seen = new Map<number, number>();
  for (let i = 0; i < nums.length; i++) {
    const need = target - nums[i];
    if (seen.has(need)) {
      return [seen.get(need)!, i];
    }
    seen.set(nums[i], i);
  }
  return [];
}"""

MERMAID = """graph TD
    Client -->|HTTPS| LB[Load Balancer]
    LB --> API1[API Server 1]
    LB --> API2[API Server 2]
    API1 <--> Cache[(Redis Cache)]
    API2 <--> Cache
    API1 --> DB[(PostgreSQL Primary)]
    API2 --> DB
    DB -.->|replication| Replica[(Read Replica)]
    API1 --> Queue([Message Queue])
    Queue --> Worker[Background Worker]"""


def node(label, accent=False):
    return row(f"Node / {label}", T("Label", label, 12, "var(--color-ide-card-text)"), padding="8px 16px", justify_content="center",
               background_color="var(--color-ide-toolbar)", border="1px solid var(--color-ide-muted)", border_radius="var(--radius-sm)")


def arrow(label=""):
    return row("Edge", ic("arrow-down", 14, "var(--color-ide-muted)") + (T("Edge Label", label, 12, "var(--color-ide-muted)") if label else ""), gap="4px")


def diagram_editor(height, readonly_note="Read-only — candidate's diagram", active=0, name="Diagram Editor"):
    tpls = ["System design", "Sequence", "Class diagram", "State machine"]
    chips = "".join(row(f"Template / {t}", T("Label", t, 12, "#fff" if i == active else "var(--color-ide-muted)"), padding="4px 12px",
                        border_radius="5px", background_color="var(--color-brand-primary)" if i == active else "var(--color-ide-chip)")
                    for i, t in enumerate(tpls))
    bar = row("Template Bar", T("Label", "Template:", 12, "var(--color-ide-faint)") + chips + box("Spacer", "", flex_grow="1") +
              T("Note", readonly_note, 12, "var(--color-ide-faint)"), gap="8px", padding="8px 16px", background_color="var(--color-ide-toolbar)",
              border_bottom="1px solid var(--color-ide-border)", width="100%")
    src = "".join(T(f"Src {i+1}", l.replace(" ", "&nbsp;") or "&nbsp;", 13, "var(--color-code-fg)", lh=23) for i, l in enumerate(MERMAID.split("\n")))
    left = col("Mermaid Source", src + T("Pane Label", "MERMAID", 12, "var(--color-ide-faint)", position="absolute", right="12px", top="8px"),
               width="50%", padding="16px", background_color="var(--color-ide-code-pane)", border_right="1px solid var(--color-ide-border)", position="relative")
    api = row("API Row", node("API Server 1") + node("API Server 2"), gap="32px")
    store = row("Store Row", node("Redis Cache") + node("PostgreSQL Primary") + node("Message Queue"), gap="24px")
    graph = col("Rendered Graph", node("Client") + arrow("HTTPS") + node("Load Balancer") + arrow() + api + arrow() + store + arrow("replication") +
                row("Leaf Row", node("Read Replica") + node("Background Worker"), gap="32px"), align_items="center", gap="4px")
    right = row("Rendered Preview", graph, width="50%", justify_content="center", align_items="flex-start", padding="28px 20px",
                background_color="var(--color-ide-preview)")
    return col(name, bar + row("Panes", left + right, flex_grow="1", width="100%", align_items="stretch", height=f"{height - 41}px"),
               height=f"{height}px", width="100%", background_color="var(--color-code-bg)")


def shape_box(name, x, y, w, h, color, label="", radius=4):
    return box(f"Shape / {name}", T("Text", label, 14, color, text_align="center", width=f"{w}px") if label else "",
               position="absolute", left=f"{x}px", top=f"{y}px", width=f"{w}px", height=f"{h}px", border=f"2px solid {color}",
               border_radius=f"{radius}px", display="flex", align_items="center", justify_content="center")


def whiteboard(height, empty=False, name="Whiteboard"):
    shapes = ""
    if empty:
        shapes = col("Empty Hint", T("Glyph", "◻", 32, "var(--color-ide-faint)", opacity=".3") + T("Hint", "Candidate hasn't drawn anything yet", 12, "var(--color-ide-faint)"),
                     position="absolute", left="0px", top="0px", width="1192px", height=f"{height}px", align_items="center", justify_content="center", gap="8px")
    else:
        W, B, G, R, O = ("var(--color-wb-white)", "var(--color-wb-blue)", "var(--color-diff-added)", "var(--color-diff-removed)", "var(--color-wb-orange)")
        shapes = (shape_box("Client", 80, 120, 140, 64, W, "Client") + shape_box("API", 380, 120, 160, 64, B, "API Gateway") +
                  shape_box("Auth", 380, 300, 160, 64, G, "Auth Service") + shape_box("DB", 720, 120, 160, 64, O, "Postgres", 32) +
                  shape_box("Cache", 720, 300, 160, 64, R, "Redis", 32) +
                  ic("arrow-right", 40, W, "Arrow / Client to API").replace("<x-paper-icon", '<x-paper-icon style="position:absolute;left:250px;top:132px"') +
                  ic("arrow-right", 40, B, "Arrow / API to DB").replace("<x-paper-icon", '<x-paper-icon style="position:absolute;left:570px;top:132px"') +
                  ic("arrow-down", 40, B, "Arrow / API to Auth").replace("<x-paper-icon", '<x-paper-icon style="position:absolute;left:440px;top:210px"') +
                  ic("arrow-right", 40, G, "Arrow / Auth to Cache").replace("<x-paper-icon", '<x-paper-icon style="position:absolute;left:570px;top:312px"') +
                  T("Note", "cache sessions, TTL 15m", 15, "var(--color-wb-purple)", position="absolute", left="700px", top="390px") +
                  row("Read-only Badge", T("Label", "Read-only — candidate's whiteboard", 12, "var(--color-ide-faint)"), position="absolute", right="14px", bottom="10px",
                      padding="4px 12px", background_color="var(--color-ide-wb-bar)", border="1px solid var(--color-ide-wb-border)", border_radius="5px"))
    return box(name, shapes, position="relative", width="100%", height=f"{height}px", background_color="var(--color-ide-wb)", overflow="hidden")


def theory_view(height, answers=None, name="Theory Answers"):
    if not answers:
        return row(name, T("Waiting", "Waiting for candidate answers…", 13, "var(--color-ide-faint)"), width="100%", height=f"{height}px",
                   justify_content="center", align_items="center", background_color="var(--color-code-bg)")
    cards = "".join(col(f"Answer / {q}", T("Question Id", q.upper(), 12, "var(--color-ide-card-label)", letter_spacing="0.6px") +
                        T("Answer", a, 14, "var(--color-ide-card-text)", lh=24), gap="8px", padding="16px",
                        background_color="var(--color-ide-card)", border="1px solid var(--color-ide-card-border)", border_radius="var(--radius-md)")
                    for q, a in answers)
    return row(name, col("Answers", cards, gap="16px", width="760px", padding="24px"), width="100%", height=f"{height}px", justify_content="center",
               align_items="flex-start", background_color="var(--color-code-bg)", overflow="hidden")


THEORY = [("q-event-loop", "The event loop pulls tasks from the queue only when the call stack is empty. Microtasks (promise callbacks) run before the next macrotask, so a resolved promise always logs before a setTimeout(0)."),
          ("q-cap", "In a partition we must choose between consistency and availability. For the booking flow I would pick CP: reject writes on the minority side rather than double-book a slot."),
          ("q-indexes", "A composite index on (tenant_id, created_at) serves the dashboard query. The leading column filters, the second column gives ordered scans without a sort.")]
