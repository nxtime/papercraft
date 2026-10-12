"""Section / Admin / Live Session: interviewer live view (tabs: Diagrams, Whiteboard, Code, Theory; Live / Waiting)."""
from adm_ide import *

TABS = [("Diagrams", "tree-structure"), ("Whiteboard", "selection"), ("Code", "brackets-curly"), ("Theory", "question")]


def toolbar(active, live):
    tabs = ""
    for t, i in TABS:
        on = t == active
        tabs += row(f"Tab / {t}", ic(i, 14, "#fff" if on else "var(--color-ide-muted)") + T("Label", t, 13, "#fff" if on else "var(--color-ide-muted)", weight=600 if on else 400),
                    gap="8px", padding="0 16px", height="52px", border_bottom=f"2px solid {'var(--color-brand-primary)' if on else 'transparent'}")
    tag = inst("Ant Tag / Success", "Live", Label="● Live") if live else inst("Ant Tag / Default", "Waiting", Label="Waiting")
    return row("Live Toolbar", T("Title", "Live session", 13, "#fff", weight=600) + col("Tabs Gap", "", width="8px") + tabs + box("Spacer", "", flex_grow="1") + tag +
               inst("Ant Button / Primary Small", "End & score", Label="End & score →") + col("Pad", "", width="4px"), gap="16px", padding="0 12px 0 16px",
               height="52px", width="100%", background_color="var(--color-ide-toolbar)", border_bottom="1px solid var(--color-ide-border)", flex_shrink="0")


def live_shell(active, live, content):
    return sidebar() + col("Main", toolbar(active, live) + content, flex_grow="1", height="900px", background_color="var(--color-code-bg)", overflow="hidden")


def build(A):
    sid = section(A, "Section / Admin / Live Session", "Admin / Live Session")
    H = 848
    r = "/sessions/[id]/live"
    add_screen(A, sid, "Live Diagrams", r, live_shell("Diagrams", True, diagram_editor(H)))
    add_screen(A, sid, "Live Whiteboard", r + " (whiteboard tab)", live_shell("Whiteboard", True, whiteboard(H)))
    add_screen(A, sid, "Live Whiteboard Waiting", r + " (waiting, nothing drawn)", live_shell("Whiteboard", False, whiteboard(H, empty=True)))
    add_screen(A, sid, "Live Code", r + " (code tab)", live_shell("Code", True, col("Code Pane", code_editor(TWO_SUM, H), height=f"{H}px", width="100%")))
    add_screen(A, sid, "Live Theory", r + " (theory tab)", live_shell("Theory", True, theory_view(H, THEORY)))
    add_screen(A, sid, "Live Theory Waiting", r + " (waiting for answers)", live_shell("Theory", False, theory_view(H)))
    return sid


if __name__ == "__main__":
    fid, A = setup()
    print(build(A))
