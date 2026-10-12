"""TaskFlow v2: variables (no hex in the file), components with instances, phosphor icons.
Run against the running app (needs `papercraft-cli icons add phosphor`): python3 examples/taskflow.py"""
import json, os, subprocess, sys

CLI = os.environ.get("PAPERCRAFT_CLI", "papercraft-cli")
ENV = {**os.environ, "PAPERCRAFT_MCP_PORT": os.environ.get("PAPERCRAFT_MCP_PORT", "29980")}

# The only place colors are written as hex: the theme, one value set per Catppuccin flavour ("mode").
# Everything else in the file is var(--x), so `papercraft-cli call set_variable_mode '{"mode":"Latte"}'`
# turns the whole design light.
def flavour(base, mantle, crust, s0, s1, s2, text, sub, muted, mauve, on_accent, green, red, peach, blue):
    return {"base": base, "mantle": mantle, "crust": crust, "surface": s0, "surface-2": s1, "surface-3": s2,
            "text": text, "subtext": sub, "muted": muted, "accent": mauve, "on-accent": on_accent,
            "success": green, "danger": red, "warning": peach, "info": blue}


MODES = {  # the first mode is the active one
    "Mocha": flavour("#1e1e2e", "#181825", "#11111b", "#313244", "#45475a", "#585b70", "#cdd6f4", "#a6adc8", "#6c7086",
                     "#cba6f7", "#11111b", "#a6e3a1", "#f38ba8", "#fab387", "#89b4fa"),
    "Macchiato": flavour("#24273a", "#1e2030", "#181926", "#363a4f", "#494d64", "#5b6078", "#cad3f5", "#a5adcb", "#6e738d",
                         "#c6a0f6", "#181926", "#a6da95", "#ed8796", "#f5a97f", "#8aadf4"),
    "Frappé": flavour("#303446", "#292c3c", "#232634", "#414559", "#51576d", "#626880", "#c6d0f5", "#a5adce", "#737994",
                      "#ca9ee6", "#232634", "#a6d189", "#e78284", "#ef9f76", "#8caaee"),
    "Latte": flavour("#eff1f5", "#e6e9ef", "#dce0e8", "#ccd0da", "#bcc0cc", "#acb0be", "#4c4f69", "#6c6f85", "#9ca0b0",
                     "#8839ef", "#eff1f5", "#40a02b", "#d20f39", "#fe640b", "#1e66f5"),
}
THEME = MODES["Mocha"]
V = {k: f"var(--{k})" for k in THEME}


def call(tool, args=None):
    p = subprocess.run([CLI, "call", tool, "-"], input=json.dumps(args or {}), capture_output=True, text=True, env=ENV)
    if p.returncode:
        sys.exit(f"{tool} failed: {p.stdout}{p.stderr}")
    try:
        return json.loads(p.stdout)
    except json.JSONDecodeError:
        return p.stdout


def css(**k):
    return ";".join(f"{a.replace('_', '-')}:{v}" for a, v in k.items())


def box(name, kids="", **style):
    return f'<div layer-name="{name}" style="{css(**style)}">{kids}</div>'


def text(name, t, size=15, color=V["text"], weight=400, **style):
    return box(name, t, font_size=f"{size}px", color=color, font_weight=weight, **style)


def row(name, kids="", **style):
    return box(name, kids, **{"display": "flex", "align_items": "center", **style})


def col(name, kids="", **style):
    return box(name, kids, **{"display": "flex", "flex_direction": "column", **style})


def icon(name, size=20, color=V["subtext"]):
    return f'<x-paper-icon name="phosphor:{name}" size="{size}" color="{color}" layer-name="Icon / {name}"/>'


# --------------------------------------------------------------------------------- component HTML
def c_button(primary):
    bg, fg = (V["accent"], V["on-accent"]) if primary else (V["surface"], V["text"])
    return row("Content", text("Label", "Button", 16, fg, 600), justify_content="center", padding="16px",
               background=bg, border_radius="14px", width="342px")


def c_chip(on):
    return row("Content", text("Label", "Chip", 13, V["on-accent"] if on else V["subtext"], 600),
               padding="8px 14px", background=V["accent"] if on else V["surface"], border_radius="999px")


def c_field():
    return col("Content", text("Label", "Label", 12, V["subtext"], 500) +
               row("Input", text("Value", "Value", 15), padding="14px", background=V["surface"], border_radius="12px",
                   border=f"1px solid {V['surface-2']}"), gap="6px", width="342px")


def c_task(done):
    mark = row("Checkbox", icon("check", 14, V["on-accent"]) if done else "", justify_content="center",
               width="24px", height="24px", border_radius="8px", background=V["success"] if done else "transparent",
               border=f"2px solid {V['success'] if done else V['accent']}")
    info = col("Info", text("Title", "Task title", 15, V["muted"] if done else V["text"], 600,
                            text_decoration="line-through" if done else "none") +
               text("Meta", "Project · 9:00", 12, V["subtext"]), gap="3px", flex_grow="1")
    dot = row("Priority", "", width="8px", height="8px", border_radius="99px", background=V["success"] if done else V["accent"])
    return row("Content", mark + info + dot, gap="14px", padding="14px", background=V["surface"], border_radius="16px", width="350px")


def c_toggle(on):
    knob = row("Knob", "", width="20px", height="20px", border_radius="99px", background=V["crust"])
    track = row("Track", knob, width="46px", height="26px", padding="3px", border_radius="99px",
                background=V["accent"] if on else V["surface-2"], justify_content="flex-end" if on else "flex-start")
    return row("Content", text("Label", "Setting", 15) + track, justify_content="space-between", padding="14px", width="350px")


def c_flavour(selected):
    sw = row("Swatch", row("Base", "", width="16px", height="16px", border_radius="99px", background=V["base"]) +
             row("Accent", "", width="16px", height="16px", border_radius="99px", background=V["accent"]),
             gap="4px", padding="8px", background=V["crust"], border_radius="99px")
    return row("Content", sw + text("Name", "Flavour", 15, V["text"], 600, flex_grow="1") +
               (icon("check", 16, V["accent"]) if selected else ""), gap="14px", padding="14px",
               background=V["surface"], border_radius="16px", width="342px")


def c_status():
    return row("Content", text("Time", "9:41", 14, V["text"], 600) + text("Indicators", "5G  100%", 12, V["subtext"]),
               justify_content="space-between", padding="14px 8px 8px 8px", width="390px")


def c_tabbar(active):
    tabs = [("Today", "house"), ("Calendar", "calendar-blank"), ("Projects", "squares-four"), ("Settings", "gear")]
    kids = "".join(col(f"Tab / {n}", icon(ic, 22, V["accent"] if n == active else V["muted"]) +
                       text("Label", n, 11, V["accent"] if n == active else V["muted"], 600),
                       gap="3px", align_items="center", flex_grow="1") for n, ic in tabs)
    return row("Content", kids, padding="10px 12px 22px 12px", background=V["mantle"],
               border_top=f"1px solid {V['surface']}", width="390px")


# --------------------------------------------------------------------------------- project plumbing
class Project:
    def __init__(self, name):
        self.file = call("create_file", {"name": name})["fileId"]
        info = call("open_file", {"fileId": self.file})
        self.first_page = info["pages"][0]["id"]
        self.used_first = False
        for mode, values in MODES.items():
            call("set_variables", {"fileId": self.file, "mode": mode, "variables": values})

    def page(self, name):
        if not self.used_first:
            self.used_first, pid = True, self.first_page
        else:
            pid = call("create_page", {"fileId": self.file})["pageId"]
        call("rename_pages", {"fileId": self.file, "updates": [{"pageId": pid, "name": name}]})
        return Page(self, pid)


class Page:
    def __init__(self, pr, pid):
        self.pr, self.id, self.comps = pr, pid, {}

    def c(self, tool, **a):
        return call(tool, {"fileId": self.pr.file, "pageId": self.id, **a})

    def artboard(self, name, html="", **styles):
        ab = self.c("create_artboard", name=name, styles=styles)["id"]
        ids = []
        if html:
            ids = self.c("write_html", targetNodeId=ab, mode="insert-children", html=html).get("createdNodeIds", [])
        return ab, ids

    def component(self, name, html):
        ab, ids = self.artboard(name, html, display="flex", flexDirection="column", padding="0px", gap="0px",
                                width="fit-content", height="fit-content", backgroundColor="transparent")
        # the content wrapper is the component: unwrap by making the artboard itself the component
        self.c("create_component", nodeId=ab, name=name)
        self.comps[name] = ab
        return ab

    def add(self, parent, html):
        return self.c("write_html", targetNodeId=parent, mode="insert-children", html=html).get("createdNodeIds", [])

    def inst(self, comp, parent, texts=None, styles=None, name=None, **kw):
        r = self.c("create_instance", componentId=self.comps[comp], parentId=parent, **({"name": name} if name else {}), **kw)
        by_name = {l["name"]: l["id"] for l in r["layers"]}
        if texts:
            self.c("set_text_content", updates=[{"nodeId": by_name[k], "textContent": v} for k, v in texts.items()])
        ups = []
        for k, st in (styles or {}).items():
            ups.append({"nodeIds": [r["id"] if k == "root" else by_name[k]], "styles": st})
        if ups:
            self.c("update_styles", updates=ups)
        return r["id"]

    def screen(self, name, html=""):
        return self.artboard(name, html, width="390px", height="844px", backgroundColor=V["base"], display="flex",
                             flexDirection="column")


def build():
    pr = Project("TaskFlow")

    # ------------------------------------------------------------------- Onboarding
    pg = pr.page("Onboarding")
    for n, h in (("Button / Primary", c_button(True)), ("Button / Secondary", c_button(False)),
                 ("Field", c_field()), ("Status Bar", c_status())):
        pg.component(n, h)
    ab, _ = pg.screen("Splash")
    pg.inst("Status Bar", ab, name="Status Bar")
    hero = pg.add(ab, col("Hero",
        row("Logo Mark", icon("check", 48, V["on-accent"]), width="96px", height="96px", justify_content="center",
            background=V["accent"], border_radius="28px") +
        text("App Name", "TaskFlow", 34, V["text"], 800) +
        text("Tagline", "Plan the day. Finish what matters.", 15, V["subtext"], text_align="center"),
        gap="16px", align_items="center", justify_content="center", flex_grow="1", width="390px"))[0]
    actions = pg.add(ab, col("Actions", "", gap="12px", padding="24px 24px 36px 24px", width="390px"))[0]
    pg.inst("Button / Primary", actions, {"Label": "Get started"}, name="Button / Get started")
    pg.inst("Button / Secondary", actions, {"Label": "I already have an account"}, name="Button / Have account")

    ab, _ = pg.screen("Sign in")
    pg.inst("Status Bar", ab, name="Status Bar")
    form = pg.add(ab, col("Form",
        text("Title", "Welcome back", 28, V["text"], 800) + text("Subtitle", "Sign in to sync your tasks.", 14, V["subtext"]) +
        col("Fields", "", gap="16px", padding="16px 0 0 0") +
        row("Forgot", text("Link", "Forgot password?", 13, V["accent"], 600), justify_content="flex-end", width="342px") +
        col("Buttons", "", gap="12px"), gap="14px", padding="24px", width="390px"))[0]
    kids = pg.c("get_children", nodeId=form)["children"]
    fields = next(k["id"] for k in kids if k["name"] == "Fields")
    buttons = next(k["id"] for k in kids if k["name"] == "Buttons")
    pg.inst("Field", fields, {"Label": "Email", "Value": "marcos@example.com"}, name="Field / Email")
    pg.inst("Field", fields, {"Label": "Password", "Value": "••••••••••"}, name="Field / Password")
    pg.inst("Button / Primary", buttons, {"Label": "Sign in"}, name="Button / Sign in")
    pg.inst("Button / Secondary", buttons, {"Label": "Continue with Apple"}, name="Button / Apple")

    # ------------------------------------------------------------------- Tasks
    pg = pr.page("Tasks")
    for n, h in (("Status Bar", c_status()), ("Task / Open", c_task(False)), ("Task / Done", c_task(True)),
                 ("Chip / Default", c_chip(False)), ("Chip / Selected", c_chip(True)), ("Field", c_field()),
                 ("Button / Primary", c_button(True)), ("Tab Bar / Today", c_tabbar("Today"))):
        pg.component(n, h)

    ab, _ = pg.screen("Today")
    pg.inst("Status Bar", ab, name="Status Bar")
    content = pg.add(ab, col("Content", "", gap="18px", padding="12px 20px", flex_grow="1", width="390px"))[0]
    pg.add(content, row("Header", col("Greeting", text("Hello", "Good morning, Marcos", 13, V["subtext"]) +
                                      text("Title", "Today", 30, V["text"], 800), gap="2px") +
                        row("Avatar", text("Initials", "MP", 14, V["on-accent"], 700), width="44px", height="44px",
                            justify_content="center", background=V["info"], border_radius="99px"),
                        justify_content="space-between", width="350px"))
    pg.add(content, col("Progress Card", row("Progress Top", text("Summary", "2 of 5 done", 15, V["text"], 700) +
                                             text("Percent", "40%", 15, V["accent"], 700), justify_content="space-between") +
                        row("Progress Track", row("Progress Fill", "", width="40%", height="8px", background=V["accent"],
                                                  border_radius="99px"), background=V["surface-2"], border_radius="99px", height="8px"),
                        gap="12px", padding="16px", background=V["surface"], border_radius="18px", width="350px"))
    filters = pg.add(content, row("Filters", "", gap="8px"))[0]
    pg.inst("Chip / Selected", filters, {"Label": "All"}, name="Chip / All")
    pg.inst("Chip / Default", filters, {"Label": "Open"}, name="Chip / Open")
    pg.inst("Chip / Default", filters, {"Label": "Done"}, name="Chip / Done")
    lst = pg.add(content, col("Task List", "", gap="10px"))[0]
    for title, meta, done, color in [("Write the spec", "Work · 9:00", True, "success"), ("Review pull requests", "Work · 10:30", True, "success"),
                                     ("Ship papercraft-cli", "Side project · 14:00", False, "danger"),
                                     ("Reply to emails", "Inbox · 16:00", False, "warning"), ("Plan next sprint", "Work · 17:30", False, "info")]:
        pg.inst("Task / Done" if done else "Task / Open", lst, {"Title": title, "Meta": meta},
                {"Priority": {"backgroundColor": V[color]}}, name=f"Task / {title}")
    pg.inst("Tab Bar / Today", ab, name="Tab Bar")

    ab, _ = pg.screen("New task")
    pg.inst("Status Bar", ab, name="Status Bar")
    sheet = pg.add(ab, col("Sheet", row("Sheet Header", text("Title", "New task", 22, V["text"], 800) +
                                        text("Close", "Cancel", 14, V["accent"], 600), justify_content="space-between", width="342px") +
                           col("Fields", "", gap="18px") + col("Due", text("Label", "Due", 12, V["subtext"], 500) + row("Options", "", gap="8px"), gap="8px") +
                           col("Priority", text("Label", "Priority", 12, V["subtext"], 500) + row("Options", "", gap="8px"), gap="8px") +
                           col("Actions", ""), gap="18px", padding="16px 24px", width="390px"))[0]
    kids = {k["name"]: k["id"] for k in pg.c("get_children", nodeId=sheet)["children"]}
    pg.inst("Field", kids["Fields"], {"Label": "Title", "Value": "Name your task"}, {"Value": {"color": V["muted"]}}, name="Field / Title")
    pg.inst("Field", kids["Fields"], {"Label": "Notes", "Value": "Add details…"}, {"Value": {"color": V["muted"]}}, name="Field / Notes")
    for grp, opts, sel in (("Due", ["Today", "Tomorrow", "Pick date"], "Today"), ("Priority", ["Low", "Medium", "High"], "Medium")):
        opt = next(k["id"] for k in pg.c("get_children", nodeId=kids[grp])["children"] if k["name"] == "Options")
        for o in opts:
            pg.inst("Chip / Selected" if o == sel else "Chip / Default", opt, {"Label": o}, name=f"Chip / {o}")
    pg.inst("Field", kids["Fields"], {"Label": "Project", "Value": "Work"}, name="Field / Project")
    pg.inst("Button / Primary", kids["Actions"], {"Label": "Create task"}, name="Button / Create task")

    ab, _ = pg.screen("Task detail")
    pg.inst("Status Bar", ab, name="Status Bar")
    det = pg.add(ab, col("Detail",
        row("Nav", row("Back", icon("caret-left", 18, V["accent"]) + text("Label", "Back", 15, V["accent"], 600), gap="2px") +
            text("Edit", "Edit", 15, V["accent"], 600), justify_content="space-between", width="342px") +
        text("Title", "Ship papercraft-cli", 28, V["text"], 800) +
        col("Meta", "".join(row(f"Meta / {k}", text("Key", k, 14, V["subtext"]) + text("Value", v, 14, c, 600),
                                justify_content="space-between", padding="14px") for k, v, c in
                            [("Due", "Today, 14:00", V["text"]), ("Project", "Side project", V["text"]), ("Priority", "High", V["danger"])]),
            background=V["surface"], border_radius="16px", width="342px") +
        col("Subtasks", text("Heading", "SUBTASKS", 11, V["subtext"], 700, letter_spacing="1px"), gap="8px") +
        col("Notes", text("Heading", "NOTES", 11, V["subtext"], 700, letter_spacing="1px") +
            text("Body", "Document the CLI in the README and attach the zip to the release.", 14, V["text"], padding="14px",
                 background=V["surface"], border_radius="16px", line_height="20px", width="314px"), gap="8px") +
        col("Actions", ""), gap="16px", padding="12px 24px", width="390px"))[0]
    kids = {k["name"]: k["id"] for k in pg.c("get_children", nodeId=det)["children"]}
    for t, d in (("Write CLI", True), ("Add tests", True), ("Publish release", False)):
        pg.inst("Task / Done" if d else "Task / Open", kids["Subtasks"], {"Title": t, "Meta": "Code"},
                {"root": {"width": "342px"}}, name=f"Subtask / {t}")
    pg.inst("Button / Primary", kids["Actions"], {"Label": "Mark complete"}, name="Button / Mark complete")

    # ------------------------------------------------------------------- Settings
    pg = pr.page("Settings")
    for n, h in (("Status Bar", c_status()), ("Toggle / On", c_toggle(True)), ("Toggle / Off", c_toggle(False)),
                 ("Flavour / Selected", c_flavour(True)), ("Flavour / Default", c_flavour(False)),
                 ("Tab Bar / Settings", c_tabbar("Settings"))):
        pg.component(n, h)
    ab, _ = pg.screen("Settings")
    pg.inst("Status Bar", ab, name="Status Bar")
    content = pg.add(ab, col("Content",
        text("Title", "Settings", 30, V["text"], 800) +
        row("Profile Card", row("Avatar", text("Initials", "MP", 16, V["on-accent"], 700), width="52px", height="52px",
                                justify_content="center", background=V["info"], border_radius="99px") +
            col("Info", text("Name", "Marcos Paulo", 16, V["text"], 700) + text("Email", "marcos@example.com", 13, V["subtext"]), gap="2px"),
            gap="14px", padding="16px", background=V["surface"], border_radius="18px", width="318px") +
        col("Group / Appearance", text("Heading", "APPEARANCE", 11, V["subtext"], 700, letter_spacing="1px") +
            col("Rows", row("Theme", text("Label", "Theme", 15) + row("Value", text("Name", "Catppuccin Mocha", 14, V["accent"], 600) +
                                                                  icon("caret-right", 14, V["accent"]), gap="4px"),
                            justify_content="space-between", padding="14px", width="322px"),
                background=V["surface"], border_radius="16px"), gap="8px") +
        col("Group / Notifications", text("Heading", "NOTIFICATIONS", 11, V["subtext"], 700, letter_spacing="1px") +
            col("Rows", "", background=V["surface"], border_radius="16px"), gap="8px"),
        gap="18px", padding="12px 20px", flex_grow="1", width="390px"))[0]
    kids = {k["name"]: k["id"] for k in pg.c("get_children", nodeId=content)["children"]}
    rows = next(k["id"] for k in pg.c("get_children", nodeId=kids["Group / Notifications"])["children"] if k["name"] == "Rows")
    for label, on in (("Daily summary", True), ("Reminders", True), ("Weekly review", False)):
        pg.inst("Toggle / On" if on else "Toggle / Off", rows, {"Label": label}, name=f"Toggle / {label}")
    pg.inst("Tab Bar / Settings", ab, name="Tab Bar")

    ab, _ = pg.screen("Theme picker")
    pg.inst("Status Bar", ab, name="Status Bar")
    cont = pg.add(ab, col("Content", row("Nav", icon("caret-left", 18, V["accent"]) + text("Label", "Settings", 15, V["accent"], 600), gap="2px") +
                          text("Title", "Theme", 28, V["text"], 800) + col("Flavours", "", gap="10px"),
                          gap="16px", padding="12px 24px", width="390px"))[0]
    fl = next(k["id"] for k in pg.c("get_children", nodeId=cont)["children"] if k["name"] == "Flavours")
    for n in ("Latte", "Frappé", "Macchiato", "Mocha"):
        look = MODES[n]  # each swatch shows its own flavour, so these two colors are literal on purpose
        pg.inst("Flavour / Selected" if n == "Mocha" else "Flavour / Default", fl, {"Name": f"Catppuccin {n}"},
                {"Base": {"backgroundColor": look["base"]}, "Accent": {"backgroundColor": look["accent"]}},
                name=f"Flavour / {n}")

    # ------------------------------------------------------------------- Design system
    pg = pr.page("Design system")
    sw = "".join(row(f"Swatch / {n}", row("Chip", "", width="40px", height="40px", border_radius="10px", background=V[n],
                                          border=f"1px solid {V['surface-2']}") +
                     col("Info", text("Name", n, 14, V["text"], 600) + text("Var", f"--{n}", 12, V["subtext"]), gap="2px"), gap="12px")
                 for n in THEME)
    pg.artboard("Colors", col("Palette", text("Title", "Colors", 24, V["text"], 800) + col("Swatches", sw, gap="10px"), gap="18px", padding="24px"),
                width="360px", height="fit-content", backgroundColor=V["base"], display="flex", flexDirection="column")
    return pr


if __name__ == "__main__":
    print("file", build().file)
