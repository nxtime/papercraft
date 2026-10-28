//! UI kits: a library's look and its code, as data. Each kit has its tokens (colors in light and
//! dark, radius, control height, type size) and components drawn in its style and density, each
//! with a `CodeLink` so the code export and the AI spec use the library component instead of
//! rebuilding it (`apply_ui_kit` builds a page from a kit; `specs/ui-kits/spec.md`).
//!
//! In templates `{Name}` is the text of the instance's layer named `Name` (see `layer-name` in the
//! markup). Web kits also carry an egui template for the components egui has, so one design can
//! be handed off to either.

pub struct Kit {
    pub id: &'static str,
    pub name: &'static str,
    pub about: &'static str,
    pub light: &'static [(&'static str, &'static str)],
    pub dark: &'static [(&'static str, &'static str)],
    pub tokens: &'static [(&'static str, &'static str)],
    /// The kit page's background variable.
    pub surface: &'static str,
    pub components: &'static [KitComponent],
    /// Screen layouts built from the kit's components: (name, markup with `<x-paper-instance>`).
    pub layouts: &'static [(&'static str, &'static str)],
    /// The library's code per component, for a kit drawn with another kit's markup; empty means
    /// each component's own.
    pub code: derived::Code,
}

pub struct KitComponent {
    pub name: &'static str,
    pub html: &'static str,
    pub import: &'static str,
    pub react: &'static str,
    pub egui: &'static str,
}

mod derived;
mod shadcn;

pub(crate) const EGUI_BUTTON: &str = "if ui.button({Label}).clicked() {\n    // TODO: action\n}";
pub(crate) const EGUI_INPUT: &str = "ui.add(egui::TextEdit::singleline(&mut text).hint_text({Placeholder}));";

pub const KITS: [Kit; 8] = [
    Kit {
        id: "shadcn",
        name: "shadcn/ui",
        about: "Neutral, quiet components on Radix and Tailwind (new-york style): 36px controls, 14px text.",
        light: &[
            ("background", "#ffffff"), ("foreground", "#09090b"), ("card", "#ffffff"), ("primary", "#18181b"),
            ("primary-foreground", "#fafafa"), ("secondary", "#f4f4f5"), ("secondary-foreground", "#18181b"),
            ("muted", "#f4f4f5"), ("muted-foreground", "#71717a"), ("border", "#e4e4e7"), ("input", "#e4e4e7"),
            ("destructive", "#dc2626"), ("destructive-foreground", "#fafafa"), ("ring", "#a1a1aa"), ("accent", "#f4f4f5"),
            ("accent-foreground", "#18181b"), ("popover", "#ffffff"), ("overlay", "#00000080"),
        ],
        dark: &[
            ("background", "#09090b"), ("foreground", "#fafafa"), ("card", "#18181b"), ("primary", "#fafafa"),
            ("primary-foreground", "#18181b"), ("secondary", "#27272a"), ("secondary-foreground", "#fafafa"),
            ("muted", "#27272a"), ("muted-foreground", "#a1a1aa"), ("border", "#27272a"), ("input", "#3f3f46"),
            ("destructive", "#ef4444"), ("destructive-foreground", "#fafafa"), ("ring", "#52525b"), ("accent", "#27272a"),
            ("accent-foreground", "#fafafa"), ("popover", "#18181b"), ("overlay", "#000000b3"),
        ],
        tokens: &[("radius-control", "8px"), ("radius-card", "12px"), ("control-height", "36px"), ("text-sm", "14px"), ("space-4", "16px"), ("space-6", "24px")],
        surface: "background",
        components: shadcn::COMPONENTS,
        layouts: shadcn::LAYOUTS,
        code: &[],
    },
    Kit {
        id: "antd",
        name: "Ant Design",
        about: "Enterprise components (antd v5): blue primary, 32px controls, 6px radius, dense tables and forms.",
        light: &[
            ("background", "#f5f5f5"), ("container", "#ffffff"), ("text", "#1f1f1f"), ("text-secondary", "#737373"),
            ("primary", "#1677ff"), ("border", "#d9d9d9"), ("success", "#52c41a"), ("warning", "#faad14"),
            ("error", "#ff4d4f"), ("info-bg", "#e6f4ff"), ("info-border", "#91caff"),
        ],
        dark: &[
            ("background", "#000000"), ("container", "#141414"), ("text", "#e6e6e6"), ("text-secondary", "#8c8c8c"),
            ("primary", "#1668dc"), ("border", "#424242"), ("success", "#49aa19"), ("warning", "#d89614"),
            ("error", "#dc4446"), ("info-bg", "#111a2c"), ("info-border", "#15325b"),
        ],
        tokens: &[("radius", "6px"), ("control-height", "32px"), ("font-size", "14px"), ("padding", "16px"), ("padding-lg", "24px")],
        surface: "background",
        components: &[
            KitComponent {
                name: "Button / Primary",
                html: r#"<div layer-name="Button / Primary" style="display:flex;align-items:center;justify-content:center;height:32px;padding:0 15px;border-radius:6px;background:var(--primary)"><span layer-name="Label" style="font-size:14px;color:#ffffff">Primary Button</span></div>"#,
                import: r#"import { Button } from "antd""#,
                react: r#"<Button type="primary">{Label}</Button>"#,
                egui: EGUI_BUTTON,
            },
            KitComponent {
                name: "Button / Default",
                html: r#"<div layer-name="Button / Default" style="display:flex;align-items:center;justify-content:center;height:32px;padding:0 15px;border-radius:6px;background:var(--container);border:1px solid var(--border)"><span layer-name="Label" style="font-size:14px;color:var(--text)">Default Button</span></div>"#,
                import: r#"import { Button } from "antd""#,
                react: "<Button>{Label}</Button>",
                egui: EGUI_BUTTON,
            },
            KitComponent {
                name: "Button / Dashed",
                html: r#"<div layer-name="Button / Dashed" style="display:flex;align-items:center;justify-content:center;height:32px;padding:0 15px;border-radius:6px;background:var(--container);border:1px dashed var(--border)"><span layer-name="Label" style="font-size:14px;color:var(--text)">Dashed Button</span></div>"#,
                import: r#"import { Button } from "antd""#,
                react: r#"<Button type="dashed">{Label}</Button>"#,
                egui: EGUI_BUTTON,
            },
            KitComponent {
                name: "Button / Danger",
                html: r#"<div layer-name="Button / Danger" style="display:flex;align-items:center;justify-content:center;height:32px;padding:0 15px;border-radius:6px;background:var(--error)"><span layer-name="Label" style="font-size:14px;color:#ffffff">Delete</span></div>"#,
                import: r#"import { Button } from "antd""#,
                react: r#"<Button type="primary" danger>{Label}</Button>"#,
                egui: EGUI_BUTTON,
            },
            KitComponent {
                name: "Input",
                html: r#"<div layer-name="Input" style="display:flex;align-items:center;width:240px;height:32px;padding:0 11px;border-radius:6px;border:1px solid var(--border);background:var(--container)"><span layer-name="Placeholder" style="font-size:14px;color:var(--text-secondary)">Basic usage</span></div>"#,
                import: r#"import { Input } from "antd""#,
                react: r#"<Input placeholder="{Placeholder}" />"#,
                egui: EGUI_INPUT,
            },
            KitComponent {
                name: "Tag",
                html: r#"<div layer-name="Tag" style="display:flex;align-items:center;padding:0 7px;height:22px;border-radius:4px;background:var(--info-bg);border:1px solid var(--info-border)"><span layer-name="Label" style="font-size:12px;color:var(--primary)">blue</span></div>"#,
                import: r#"import { Tag } from "antd""#,
                react: r#"<Tag color="blue">{Label}</Tag>"#,
                egui: "ui.label(egui::RichText::new({Label}).small());",
            },
            KitComponent {
                name: "Card",
                html: r#"<div layer-name="Card" style="display:flex;flex-direction:column;width:300px;border-radius:8px;border:1px solid var(--border);background:var(--container)"><div style="display:flex;align-items:center;height:56px;padding:0 24px;border-bottom:1px solid var(--border)"><span layer-name="Title" style="font-size:16px;font-weight:600;color:var(--text)">Card title</span></div><div style="padding:24px"><span layer-name="Description" style="font-size:14px;color:var(--text)">Card content</span></div></div>"#,
                import: r#"import { Card } from "antd""#,
                react: "<Card title=\"{Title}\">{Description}</Card>",
                egui: "egui::Frame::group(ui.style()).show(ui, |ui| {\n    ui.strong({Title});\n    ui.separator();\n    ui.label({Description});\n});",
            },
            KitComponent {
                name: "Switch",
                html: r#"<div layer-name="Switch" style="display:flex;align-items:center;justify-content:flex-end;width:44px;height:22px;padding:2px;border-radius:11px;background:var(--primary)"><div style="width:18px;height:18px;border-radius:9px;background:#ffffff"></div></div>"#,
                import: r#"import { Switch } from "antd""#,
                react: "<Switch defaultChecked />",
                egui: "ui.toggle_value(&mut on, \"\");",
            },
            KitComponent {
                name: "Alert",
                html: r#"<div layer-name="Alert" style="display:flex;flex-direction:column;gap:4px;width:360px;padding:9px 12px;border-radius:8px;background:var(--info-bg);border:1px solid var(--info-border)"><span layer-name="Title" style="font-size:14px;color:var(--text)">Informational Notes</span><span layer-name="Description" style="font-size:14px;color:var(--text-secondary)">Additional description and information.</span></div>"#,
                import: r#"import { Alert } from "antd""#,
                react: "<Alert message=\"{Title}\" description=\"{Description}\" type=\"info\" showIcon />",
                egui: "ui.group(|ui| {\n    ui.strong({Title});\n    ui.label({Description});\n});",
            },
        ],
        layouts: &[],
        code: &[],
    },
    Kit {
        id: "reactbits",
        name: "React Bits",
        about: "Animated text, backgrounds and interactive pieces (reactbits.dev) to put on top of any style: dark stages, glowing accents.",
        light: &[
            ("stage", "#f4f1ff"), ("surface", "#ffffff"), ("text", "#110b2a"), ("text-muted", "#5b5677"),
            ("accent", "#5227ff"), ("accent-2", "#7cff67"), ("border", "#e2dcfa"),
        ],
        dark: &[
            ("stage", "#060010"), ("surface", "#120a24"), ("text", "#ffffff"), ("text-muted", "#a59cc5"),
            ("accent", "#7b5cff"), ("accent-2", "#7cff67"), ("border", "#2a1f4a"),
        ],
        tokens: &[("radius", "16px"), ("radius-pill", "999px")],
        surface: "stage",
        components: &[
            KitComponent {
                name: "Split Text",
                html: r#"<div layer-name="Split Text" style="display:flex"><span layer-name="Text" style="font-size:48px;font-weight:700;color:var(--text)">Hello, you!</span></div>"#,
                import: r#"import SplitText from "@/components/SplitText""#,
                react: r#"<SplitText text="{Text}" className="text-5xl font-bold" delay={40} />"#,
                egui: "ui.heading({Text});",
            },
            KitComponent {
                name: "Shiny Text",
                html: r#"<div layer-name="Shiny Text" style="display:flex"><span layer-name="Text" style="font-size:16px;font-weight:500;color:var(--text-muted)">Just some shiny text!</span></div>"#,
                import: r#"import ShinyText from "@/components/ShinyText""#,
                react: r#"<ShinyText text="{Text}" speed={3} />"#,
                egui: "ui.label({Text});",
            },
            KitComponent {
                name: "Gradient Text",
                html: r#"<div layer-name="Gradient Text" style="display:flex"><span layer-name="Text" style="font-size:40px;font-weight:700;color:var(--accent)">Gradient magic</span></div>"#,
                import: r#"import GradientText from "@/components/GradientText""#,
                react: r##"<GradientText colors={["#40ffaa", "#4079ff", "#40ffaa"]} animationSpeed={3}>{Text}</GradientText>"##,
                egui: "ui.heading({Text});",
            },
            KitComponent {
                name: "Aurora",
                html: r#"<div layer-name="Aurora" style="width:480px;height:200px;border-radius:16px;background:linear-gradient(135deg, #5227ff 0%, #7cff67 55%, #5227ff 100%)"></div>"#,
                import: r#"import Aurora from "@/components/Aurora""#,
                react: r##"<Aurora colorStops={["#5227FF", "#7CFF67", "#5227FF"]} amplitude={1} blend={0.5} />"##,
                egui: "",
            },
            KitComponent {
                name: "Spotlight Card",
                html: r#"<div layer-name="Spotlight Card" style="display:flex;flex-direction:column;gap:8px;width:300px;padding:28px;border-radius:20px;border:1px solid var(--border);background:radial-gradient(circle at 30% 20%, #2b1a5c 0%, #120a24 70%)"><span layer-name="Title" style="font-size:18px;font-weight:600;color:#ffffff">Spotlight</span><span layer-name="Description" style="font-size:14px;color:#a59cc5">Move your pointer over the card.</span></div>"#,
                import: r#"import SpotlightCard from "@/components/SpotlightCard""#,
                react: "<SpotlightCard spotlightColor=\"rgba(82, 39, 255, 0.25)\">\n  <h3>{Title}</h3>\n  <p>{Description}</p>\n</SpotlightCard>",
                egui: "egui::Frame::group(ui.style()).show(ui, |ui| {\n    ui.strong({Title});\n    ui.label({Description});\n});",
            },
            KitComponent {
                name: "Dock",
                html: r#"<div layer-name="Dock" style="display:flex;gap:12px;align-items:flex-end;padding:10px 14px;border-radius:20px;border:1px solid var(--border);background:var(--surface)"><div style="width:44px;height:44px;border-radius:12px;background:#2a1f4a"></div><div style="width:52px;height:52px;border-radius:14px;background:var(--accent)"></div><div style="width:44px;height:44px;border-radius:12px;background:#2a1f4a"></div><div style="width:44px;height:44px;border-radius:12px;background:#2a1f4a"></div></div>"#,
                import: r#"import Dock from "@/components/Dock""#,
                react: "<Dock items={items} panelHeight={68} baseItemSize={50} magnification={70} />",
                egui: "",
            },
            KitComponent {
                name: "Magnet Button",
                html: r#"<div layer-name="Magnet Button" style="display:flex;align-items:center;justify-content:center;height:44px;padding:0 22px;border-radius:999px;background:var(--accent)"><span layer-name="Label" style="font-size:15px;font-weight:600;color:#ffffff">Hover me</span></div>"#,
                import: r#"import Magnet from "@/components/Magnet""#,
                react: "<Magnet padding={50} magnetStrength={3}>\n  <button className=\"rounded-full bg-[#5227ff] px-5 py-2.5 font-semibold text-white\">{Label}</button>\n</Magnet>",
                egui: EGUI_BUTTON,
            },
        ],
        layouts: &[],
        code: &[],
    },
    Kit {
        id: "egui",
        name: "egui",
        about: "Native egui widgets for Rust apps: compact (20px rows), small type, square-ish corners, the default dark visuals.",
        light: &[
            ("panel", "#f8f8f8"), ("widget", "#e6e6e6"), ("widget-hover", "#dcdcdc"), ("text", "#3c3c3c"),
            ("text-strong", "#000000"), ("text-weak", "#8c8c8c"), ("selection", "#90d1ff"), ("stroke", "#bebebe"), ("field", "#ffffff"),
        ],
        dark: &[
            ("panel", "#1b1b1b"), ("widget", "#3c3c3c"), ("widget-hover", "#464646"), ("text", "#b4b4b4"),
            ("text-strong", "#ffffff"), ("text-weak", "#8c8c8c"), ("selection", "#005c80"), ("stroke", "#3c3c3c"), ("field", "#0f0f0f"),
        ],
        tokens: &[("radius", "3px"), ("row-height", "20px"), ("body", "13px"), ("item-spacing", "8px")],
        surface: "panel",
        components: &[
            KitComponent {
                name: "Button",
                html: r#"<div layer-name="Button" style="display:flex;align-items:center;justify-content:center;height:20px;padding:0 6px;border-radius:3px;background:var(--widget)"><span layer-name="Label" style="font-size:13px;color:var(--text)">Click me</span></div>"#,
                import: "",
                react: "",
                egui: EGUI_BUTTON,
            },
            KitComponent {
                name: "Checkbox",
                html: r#"<div layer-name="Checkbox" style="display:flex;align-items:center;gap:6px"><div style="width:14px;height:14px;border-radius:3px;background:var(--widget)"></div><span layer-name="Label" style="font-size:13px;color:var(--text)">Enabled</span></div>"#,
                import: "",
                react: "",
                egui: "ui.checkbox(&mut checked, {Label});",
            },
            KitComponent {
                name: "Toggle",
                html: r#"<div layer-name="Toggle" style="display:flex;align-items:center;justify-content:center;height:20px;padding:0 6px;border-radius:3px;background:var(--selection)"><span layer-name="Label" style="font-size:13px;color:var(--text-strong)">Grid</span></div>"#,
                import: "",
                react: "",
                egui: "ui.toggle_value(&mut on, {Label});",
            },
            KitComponent {
                name: "Text Edit",
                html: r#"<div layer-name="Text Edit" style="display:flex;align-items:center;width:200px;height:20px;padding:0 4px;border-radius:3px;background:var(--field);border:1px solid var(--stroke)"><span layer-name="Placeholder" style="font-size:13px;color:var(--text-weak)">Search…</span></div>"#,
                import: "",
                react: "",
                egui: EGUI_INPUT,
            },
            KitComponent {
                name: "Slider",
                html: r#"<div layer-name="Slider" style="display:flex;align-items:center;gap:8px"><div style="display:flex;align-items:center;width:140px;height:4px;border-radius:2px;background:var(--widget)"><div style="width:84px;height:4px;border-radius:2px;background:var(--selection)"></div></div><span layer-name="Label" style="font-size:13px;color:var(--text)">Volume</span></div>"#,
                import: "",
                react: "",
                egui: "ui.add(egui::Slider::new(&mut value, 0.0..=100.0).text({Label}));",
            },
            KitComponent {
                name: "Combo Box",
                html: r#"<div layer-name="Combo Box" style="display:flex;align-items:center;gap:8px"><div style="display:flex;align-items:center;justify-content:space-between;width:140px;height:20px;padding:0 6px;border-radius:3px;background:var(--widget)"><span layer-name="Value" style="font-size:13px;color:var(--text)">First</span><svg width="10" height="10" viewBox="0 0 10 10" style="color:var(--text)"><path d="M2 3.5l3 3 3-3" stroke="currentColor" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg></div><span layer-name="Label" style="font-size:13px;color:var(--text)">Choice</span></div>"#,
                import: "",
                react: "",
                egui: "egui::ComboBox::from_label({Label})\n    .selected_text({Value})\n    .show_ui(ui, |ui| {\n        // ui.selectable_value(&mut choice, Choice::First, \"First\");\n    });",
            },
            KitComponent {
                name: "Heading",
                html: r#"<div layer-name="Heading" style="display:flex"><span layer-name="Text" style="font-size:20px;color:var(--text-strong)">Settings</span></div>"#,
                import: "",
                react: "",
                egui: "ui.heading({Text});",
            },
            KitComponent {
                name: "Collapsing Header",
                html: r#"<div layer-name="Collapsing Header" style="display:flex;align-items:center;gap:6px;height:20px"><svg width="10" height="10" viewBox="0 0 10 10" style="color:var(--text)"><path d="M2 3.5l3 3 3-3" stroke="currentColor" fill="none" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg><span layer-name="Title" style="font-size:13px;color:var(--text-strong)">Advanced</span></div>"#,
                import: "",
                react: "",
                egui: "ui.collapsing({Title}, |ui| {\n    // body\n});",
            },
        ],
        layouts: &[],
        code: &[],
    },
    Kit {
        id: "mui",
        name: "Material UI",
        about: "Material Design 2 in React (@mui/material v6): blue primary, 4px radius, 36px controls.",
        light: &[
            ("background", "#ffffff"), ("foreground", "#212121"), ("card", "#ffffff"), ("primary", "#1976d2"),
            ("primary-foreground", "#ffffff"), ("secondary", "#e0e0e0"), ("secondary-foreground", "#212121"),
            ("muted", "#f5f5f5"), ("muted-foreground", "#666666"), ("border", "#e0e0e0"), ("input", "#c4c4c4"),
            ("destructive", "#d32f2f"), ("destructive-foreground", "#ffffff"), ("ring", "#1976d2"), ("accent", "#f5f5f5"),
            ("accent-foreground", "#212121"), ("popover", "#ffffff"), ("overlay", "#00000080"),
        ],
        dark: &[
            ("background", "#121212"), ("foreground", "#ffffff"), ("card", "#1e1e1e"), ("primary", "#90caf9"),
            ("primary-foreground", "#0d0d0d"), ("secondary", "#2e2e2e"), ("secondary-foreground", "#ffffff"),
            ("muted", "#2a2a2a"), ("muted-foreground", "#b3b3b3"), ("border", "#2e2e2e"), ("input", "#5c5c5c"),
            ("destructive", "#f44336"), ("destructive-foreground", "#0d0d0d"), ("ring", "#90caf9"), ("accent", "#2a2a2a"),
            ("accent-foreground", "#ffffff"), ("popover", "#1e1e1e"), ("overlay", "#00000080"),
        ],
        tokens: &[("radius-control", "4px"), ("radius-card", "4px"), ("control-height", "36px"), ("text-sm", "14px"), ("space-4", "16px"), ("space-6", "24px")],
        surface: "background",
        components: shadcn::COMPONENTS,
        layouts: shadcn::LAYOUTS,
        code: derived::MUI,
    },
    Kit {
        id: "mantine",
        name: "Mantine",
        about: "Mantine v7 React components: blue primary, 4px controls in 8px cards, 36px controls.",
        light: &[
            ("background", "#ffffff"), ("foreground", "#000000"), ("card", "#ffffff"), ("primary", "#1971c2"),
            ("primary-foreground", "#ffffff"), ("secondary", "#e7f5ff"), ("secondary-foreground", "#1864ab"),
            ("muted", "#f1f3f5"), ("muted-foreground", "#495057"), ("border", "#dee2e6"), ("input", "#ced4da"),
            ("destructive", "#c92a2a"), ("destructive-foreground", "#ffffff"), ("ring", "#1971c2"), ("accent", "#f8f9fa"),
            ("accent-foreground", "#000000"), ("popover", "#ffffff"), ("overlay", "#00000080"),
        ],
        dark: &[
            ("background", "#242424"), ("foreground", "#c9c9c9"), ("card", "#2e2e2e"), ("primary", "#1971c2"),
            ("primary-foreground", "#ffffff"), ("secondary", "#1e3a52"), ("secondary-foreground", "#a5d8ff"),
            ("muted", "#2e2e2e"), ("muted-foreground", "#a6a7ab"), ("border", "#424242"), ("input", "#424242"),
            ("destructive", "#c92a2a"), ("destructive-foreground", "#ffffff"), ("ring", "#1971c2"), ("accent", "#2e2e2e"),
            ("accent-foreground", "#c9c9c9"), ("popover", "#2e2e2e"), ("overlay", "#000000b3"),
        ],
        tokens: &[("radius-control", "4px"), ("radius-card", "8px"), ("control-height", "36px"), ("text-sm", "14px"), ("space-4", "16px"), ("space-6", "24px")],
        surface: "background",
        components: shadcn::COMPONENTS,
        layouts: shadcn::LAYOUTS,
        code: derived::MANTINE,
    },
    Kit {
        id: "bootstrap",
        name: "Bootstrap",
        about: "Bootstrap 5 through react-bootstrap: blue primary, 6px radius, 38px controls.",
        light: &[
            ("background", "#ffffff"), ("foreground", "#212529"), ("card", "#ffffff"), ("primary", "#0b5ed7"),
            ("primary-foreground", "#ffffff"), ("secondary", "#6c757d"), ("secondary-foreground", "#ffffff"),
            ("muted", "#e9ecef"), ("muted-foreground", "#6c757d"), ("border", "#dee2e6"), ("input", "#dee2e6"),
            ("destructive", "#bb2d3b"), ("destructive-foreground", "#ffffff"), ("ring", "#86b7fe"), ("accent", "#f8f9fa"),
            ("accent-foreground", "#212529"), ("popover", "#ffffff"), ("overlay", "#00000080"),
        ],
        dark: &[
            ("background", "#212529"), ("foreground", "#dee2e6"), ("card", "#2b3035"), ("primary", "#0b5ed7"),
            ("primary-foreground", "#ffffff"), ("secondary", "#6c757d"), ("secondary-foreground", "#ffffff"),
            ("muted", "#343a40"), ("muted-foreground", "#adb5bd"), ("border", "#495057"), ("input", "#495057"),
            ("destructive", "#bb2d3b"), ("destructive-foreground", "#ffffff"), ("ring", "#86b7fe"), ("accent", "#343a40"),
            ("accent-foreground", "#dee2e6"), ("popover", "#2b3035"), ("overlay", "#000000b3"),
        ],
        tokens: &[("radius-control", "6px"), ("radius-card", "6px"), ("control-height", "38px"), ("text-sm", "14px"), ("space-4", "16px"), ("space-6", "24px")],
        surface: "background",
        components: shadcn::COMPONENTS,
        layouts: shadcn::LAYOUTS,
        code: derived::BOOTSTRAP,
    },
    Kit {
        id: "chakra",
        name: "Chakra UI",
        about: "Chakra UI v3 with the teal colour palette: 6px controls in 8px cards, 40px controls.",
        light: &[
            ("background", "#ffffff"), ("foreground", "#09090b"), ("card", "#ffffff"), ("primary", "#0f766e"),
            ("primary-foreground", "#ffffff"), ("secondary", "#f4f4f5"), ("secondary-foreground", "#18181b"),
            ("muted", "#f4f4f5"), ("muted-foreground", "#52525b"), ("border", "#e4e4e7"), ("input", "#e4e4e7"),
            ("destructive", "#dc2626"), ("destructive-foreground", "#ffffff"), ("ring", "#0f766e"), ("accent", "#f4f4f5"),
            ("accent-foreground", "#18181b"), ("popover", "#ffffff"), ("overlay", "#00000080"),
        ],
        dark: &[
            ("background", "#111111"), ("foreground", "#fafafa"), ("card", "#18181b"), ("primary", "#2dd4bf"),
            ("primary-foreground", "#042f2e"), ("secondary", "#27272a"), ("secondary-foreground", "#fafafa"),
            ("muted", "#27272a"), ("muted-foreground", "#a1a1aa"), ("border", "#27272a"), ("input", "#3f3f46"),
            ("destructive", "#ef4444"), ("destructive-foreground", "#fafafa"), ("ring", "#2dd4bf"), ("accent", "#27272a"),
            ("accent-foreground", "#fafafa"), ("popover", "#18181b"), ("overlay", "#000000b3"),
        ],
        tokens: &[("radius-control", "6px"), ("radius-card", "8px"), ("control-height", "40px"), ("text-sm", "14px"), ("space-4", "16px"), ("space-6", "24px")],
        surface: "background",
        components: shadcn::COMPONENTS,
        layouts: shadcn::LAYOUTS,
        code: derived::CHAKRA,
    },
];

pub fn kit(id: &str) -> Option<&'static Kit> {
    KITS.iter().find(|k| k.id == id)
}
