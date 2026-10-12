"""Foundation documentation sections: colors, typography, spacing, radius & shadow, page layout."""
from ds_common import *

W = 1440


def section(name, title, body, intro=None):
    kids = T("Section Title", title, 24, weight=700) + (T("Section Intro", intro, 14, fg2) if intro else "") + body
    return col(name, kids, width=f"{W}px", gap="var(--spacing-xl)", padding="var(--spacing-xl)", background="transparent")


fg2 = "var(--color-text-secondary)"


def card(name, title, body, w=None, **st):
    return col(name, T("Card Title", title, 16, weight=600) + body, gap="var(--spacing-md)", padding="var(--spacing-lg)", background="var(--color-bg-elevated)",
               border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)", **({"width": w} if w else {}), **st)


ALPHA = {"color-bg-overlay": "rgba(0,0,0,.45)", "color-text-primary": "rgba(0,0,0,.88)", "color-text-secondary": "rgba(0,0,0,.65)", "color-text-disabled": "rgba(0,0,0,.45)",
         "color-sidebar-border": "rgba(255,255,255,.07)", "color-sidebar-text": "rgba(255,255,255,.65)", "color-sidebar-text-muted": "rgba(255,255,255,.45)",
         "color-sidebar-text-faint": "rgba(255,255,255,.35)", "color-sidebar-hover": "rgba(255,255,255,.06)", "color-sidebar-badge": "rgba(255,255,255,.25)",
         "color-sidebar-tag-bg": "rgba(22,119,255,.25)", "color-text-placeholder": "rgba(0,0,0,.25)", "color-tooltip-bg": "rgba(0,0,0,.85)", "color-switch-off": "rgba(0,0,0,.25)",
         "color-focus-ring": "rgba(22,119,255,.10)"}

COLOR_GROUPS = {
    "Brand": ["color-brand-primary", "color-brand-secondary", "color-text-link", "color-info", "color-button-primary-hover"],
    "Background": ["color-bg-layout", "color-bg-elevated", "color-bg-sider", "color-bg-selected", "color-bg-overlay"],
    "Border": ["color-border", "color-border-light", "color-border-strong", "color-card-hover-border"],
    "Text": ["color-text-primary", "color-text-secondary", "color-text-disabled", "color-text-inverse", "color-text-placeholder"],
    "Status": ["color-success", "color-warning", "color-error", "color-star", "color-score-warn", "color-score-poor"],
    "Diff & code": ["color-diff-added", "color-diff-removed", "color-diff-neutral", "color-code-bg", "color-code-fg"],
    "Sidebar (dark shell)": ["color-sidebar-bg", "color-sidebar-border", "color-sidebar-text", "color-sidebar-text-muted", "color-sidebar-text-faint", "color-sidebar-hover", "color-sidebar-badge", "color-sidebar-tag-bg"],
    "Tag tones": [f"color-tag-{t}-{x}" for t in ("blue", "green", "gold", "red", "purple", "cyan") for x in ("fg", "bg", "bd")],
    "Avatar palette": ["color-brand-primary", "color-avatar-purple", "color-avatar-teal", "color-warning", "color-avatar-pink", "color-success", "color-avatar-orange", "color-avatar-indigo"],
    "Controls": ["color-button-dark", "color-tooltip-bg", "color-switch-off", "color-focus-ring"],
}


def swatch(var, val):
    shown = ALPHA.get(var) or val
    return row(f"Swatch / {var}", row("Chip Bg", box("Chip", width="40px", height="40px", border_radius="var(--radius-md)", background=f"var(--{var})", border="1px solid var(--color-border-light)"), background="var(--color-sidebar-bg)" if "sidebar" in var and var != "color-sidebar-bg" else "transparent", border_radius="var(--radius-md)") +
               col("Names", T("Token Name", var.replace("color-", ""), 14, weight=600, lh=18) + T("Token Var", f"var(--{var})", 12, fg2, lh=16), gap="0px", flex_grow="1") +
               T("Token Value", shown, 12, fg2, lh=16), gap="12px")


def colors(vals):
    cards = ""
    for g, names in COLOR_GROUPS.items():
        cards += card(f"Colors / {g}", g, col("Swatches", "".join(swatch(n, vals[n]) for n in names), gap="12px"), w="440px")
    body = row("Color Groups", cards, flex_wrap="wrap", gap="var(--spacing-lg)", align_items="flex-start")
    return section("Section / Colors", "Colors", body, "Every color is a PaperCraft variable named exactly like the CSS custom property in packages/ui/src/tokens.css, minus the leading --. Use var(--color-brand-primary) etc.")


FONT = [("font-size-xs", 12, "Labels, table headers (uppercase 600), Tag, Pill, captions", 16),
        ("font-size-sm", 14, "Body, buttons, inputs, nav items, table cells", 22),
        ("font-size-md", 16, "SectionTitle, card titles, large inputs, brand name", 24),
        ("font-size-lg", 18, "Large headings (token only)", 26),
        ("font-size-xl", 20, "Large headings (token only)", 28),
        ("font-size-xxl", 24, "Page titles (PageHeader h3, 600)", 32)]
OTHER = [(30, "Stat value (700)"), (26, "Donut total (700)"), (15, "Sidebar brand name (700)"), (13, "Sidebar user name (600), Avatar initials"), (12, "Sidebar group label, role tag (clamped from 10-11)")]


def typography():
    rows = ""
    for tok, px, use, lh in FONT:
        rows += row(f"Type / {tok}", T("Type Token", tok, 12, fg2, width="140px", lh=16) + T("Type Size", f"{px}px / {lh}px", 12, fg2, width="100px", lh=16) +
                    T("Type Sample", "The quick brown fox jumps over the lazy dog", px, lh=lh, width="620px") + T("Type Usage", use, 12, fg2, flex_grow="1", lh=16), gap="var(--spacing-md)")
    other = "".join(row(f"Type / {px}px", T("Type Token", "(literal)", 12, fg2, width="140px", lh=16) + T("Type Size", f"{px}px", 12, fg2, width="100px", lh=16) +
                        T("Type Sample", "Hireloop 12 42", px, weight=700 if px in (30, 26, 15) else 400, width="620px") + T("Type Usage", use, 12, fg2, flex_grow="1", lh=16), gap="var(--spacing-md)") for px, use in OTHER)
    wts = row("Weights", "".join(col(f"Weight / {w}", T("Weight Sample", "Aa Hireloop", 20, weight=w) + T("Weight Note", note, 12, fg2, lh=16), gap="4px") for w, note in
                                  [(400, "400 regular: body"), (500, "500 medium: buttons, tags (renders as 400: Inter regular/bold only)"), (600, "600 semibold: titles, headers (renders bold)"), (700, "700 bold: stat values, brand")]), gap="var(--spacing-xl)", align_items="flex-start")
    ant = "".join(row(f"AntD / {n}", T("Type Token", n, 12, fg2, width="140px", lh=16) + T("Type Size", f"{px}px / {lh}px", 12, fg2, width="100px", lh=16) +
                      T("Type Sample", "Page heading", px, weight=600, lh=lh, width="620px") + T("Type Usage", u, 12, fg2, flex_grow="1", lh=16), gap="var(--spacing-md)")
                  for n, px, lh, u in [("Typography.Title 1", 38, 46, "unused"), ("Typography.Title 3", 24, 32, "PageHeader title, login heading"), ("Typography.Title 4", 20, 28, "card headings"), ("Typography.Title 5", 16, 24, "card section titles (600)")])
    body = card("Type scale", "Type scale (font-size tokens)", col("Rows", rows, gap="var(--spacing-md)")) + card("Type literals", "Sizes used literally in the code", col("Rows", other, gap="var(--spacing-md)")) + \
        card("Type weights", "Weights", wts) + card("Type AntD", "AntD Typography.Title (all text is Inter)", col("Rows", ant, gap="var(--spacing-md)"))
    return section("Section / Typography", "Typography", col("Type Cards", body, gap="var(--spacing-lg)"), "Single family: Inter (the source uses the system UI stack). Line-height ~1.5 (1.35 for headings).")


def spacing():
    bars = ""
    for n, px in [("xs", 4), ("sm", 8), ("md", 16), ("lg", 24), ("xl", 32), ("xxl", 48)]:
        bars += row(f"Space / {n}", T("Space Token", f"spacing-{n}", 14, weight=600, width="120px") + T("Space Value", f"{px}px", 12, fg2, width="48px", lh=16) +
                    box("Space Bar", width=f"var(--spacing-{n})", height="16px", background="var(--color-brand-primary)", border_radius="var(--radius-sm)"), gap="var(--spacing-md)")
    for n, px in [("page-padding", 24), ("page-padding-lg", 48)]:
        bars += row(f"Space / {n}", T("Space Token", n, 14, weight=600, width="120px") + T("Space Value", f"{px}px", 12, fg2, width="48px", lh=16) +
                    box("Space Bar", width=f"var(--{n})", height="16px", background="var(--color-brand-secondary)", border_radius="var(--radius-sm)"), gap="var(--spacing-md)")
    return section("Section / Spacing", "Spacing", card("Spacing scale", "Spacing scale (4px grid)", col("Bars", bars, gap="var(--spacing-md)"), w="720px"),
                   "Tokens spacing-xs..xxl and page-padding. Off-scale values in the code: 2/5/6/7/9/10/12/14/18/20/22.")


def radius_shadow():
    rad = "".join(col(f"Radius / {n}", box("Radius Sample", width="96px", height="64px", background="var(--color-bg-selected)", border="1px solid var(--color-brand-primary)", border_radius=f"var(--radius-{n})") +
                      T("Radius Token", f"radius-{n}", 14, weight=600, lh=18) + T("Radius Value", f"{px}px: {u}", 12, fg2, lh=16), gap="8px")
                  for n, px, u in [("sm", 4, "Tag, Segmented item, ScoreDots"), ("md", 8, "Button, inputs, nav items, AntD controls"), ("lg", 12, "Card, DataTable, Modal")])
    rad += col("Radius / pill", box("Radius Sample", width="96px", height="64px", background="var(--color-bg-selected)", border="1px solid var(--color-brand-primary)", border_radius="99px") +
               T("Radius Token", "99px (literal)", 14, weight=600, lh=18) + T("Radius Value", "Pill, ScoreBar, Switch, Avatar 50%", 12, fg2, lh=16), gap="8px")
    shd = "".join(col(f"Shadow / {n}", box("Shadow Sample", width="160px", height="96px", background="var(--color-bg-elevated)", border="1px solid var(--color-border-light)", border_radius="var(--radius-lg)", box_shadow=f"var(--shadow-{n})") +
                      T("Shadow Token", f"shadow-{n}", 14, weight=600, lh=18) + T("Shadow Value", v, 12, fg2, lh=16), gap="8px")
                  for n, v in [("sm", "0 1px 2px rgba(0,0,0,.06): Card, Segmented active"), ("md", "0 4px 12px rgba(0,0,0,.08): Card hover, dropdowns"), ("lg", "0 8px 24px rgba(0,0,0,.12): Modal, Drawer")])
    stage = row("Shadow Stage", shd, gap="var(--spacing-lg)", padding="var(--spacing-lg)", background="var(--color-bg-layout)", border_radius="var(--radius-lg)", align_items="flex-start")
    return section("Section / Radius & Shadow", "Radius & Shadow", row("Radius Shadow Cards", card("Radius", "Radius", row("Radius Row", rad, gap="var(--spacing-lg)", align_items="flex-start")) + card("Shadow", "Shadows (on cards, over bg-layout)", stage), gap="var(--spacing-lg)", align_items="flex-start"))


def page_layout():
    widths = [("xs", 480, "invite/verify card"), ("sm", 420, ""), ("md / narrow", 720, "narrow content"), ("lg / default", 900, ".app-content default"), ("xl / wide", 1100, ".app-content--wide")]
    bars = "".join(row(f"Width / {n}", T("Width Token", f"page-max-width-{n}", 14, weight=600, width="220px") + T("Width Value", f"{px}px", 12, fg2, width="56px", lh=16) +
                       box("Width Bar", width=f"{px//2}px", height="16px", background="var(--color-brand-secondary)", border_radius="var(--radius-sm)") + T("Width Use", u, 12, fg2, lh=16), gap="var(--spacing-md)") for n, px, u in widths)
    shell = col("Shell facts", "".join(T(f"Fact {i}", t, 14) for i, t in enumerate([
        "App shell: sidebar 248px (flex 0 0 248) + main column; desktop frames are 1440x900.", "Page header: 24px padding, ~102px tall (title 24/600 + subtitle 14).",
        "Content: padding 24 (page-padding), max-width 900 default / 1100 wide; centered.", "Candidate: white header card inset (padding 16 24) on bg-layout.", "Mobile (<=640px): sidebar hidden, padding 16."], 1)), gap="8px")
    z = "".join(row(f"Z / {n}", T("Z Token", f"z-{n}", 14, weight=600, width="140px") + T("Z Value", str(v), 14, fg2, width="64px") + T("Z Use", u, 12, fg2, lh=16), gap="var(--spacing-md)")
                for n, v, u in [("base", 1, "sticky table header"), ("dropdown", 1000, "Select / menus"), ("modal", 1050, "Modal, Drawer"), ("toast", 1100, "message / notifications")])
    return section("Section / Page Layout", "Page Layout", row("Layout Cards", card("Max widths", "Page max-widths (bars drawn at 50%)", col("Bars", bars, gap="var(--spacing-md)")) + col("Layout Right", card("Shell", "App shell", shell) + card("Z-index", "z-index scale", col("Zs", z, gap="var(--spacing-sm)")), gap="var(--spacing-lg)"),
                                                                    gap="var(--spacing-lg)", align_items="flex-start"))


def build(P, vals):
    ids = []
    for html in (colors(vals), typography(), spacing(), radius_shadow(), page_layout()):
        ids += P.add("root", html)
    return ids
