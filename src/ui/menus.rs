//! The menu bar's building blocks: top-level menus that open a little below their button, and
//! items with a leading icon.

use crate::editor::Tool;
use crate::editor::shortcuts::Action;
use crate::editor::vector_ops::BoolOp;
use crate::theme::{self, Icon};
use egui::{Atom, Button, Id, InnerResponse, Popup, UiKind, UiStackInfo, Ui, Vec2};
use egui::containers::menu::{MenuConfig, SubMenuButton};

/// Room kept for an item's icon, with or without one, so the labels line up.
const ICON_SLOT: f32 = 16.0;

/// A menu title that opens its menu [`theme::POPUP_GAP`] below the button, unlike egui's own
/// `menu_button`, which butts the menu up against it. Inside a menu use `ui.menu_button`.
pub fn menu_button<R>(
    ui: &mut Ui,
    title: &str,
    content: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<Option<R>> {
    let response = ui.add(Button::new(title));
    let config = MenuConfig::find(ui);
    let inner = Popup::menu(&response)
        .gap(theme::POPUP_GAP)
        .close_behavior(config.close_behavior)
        .style(config.style.clone())
        .info(UiStackInfo::new(UiKind::Menu).with_tag_value(MenuConfig::MENU_CONFIG_TAG, config))
        .show(content);
    InnerResponse::new(inner.map(|i| i.inner), response)
}

/// An item: its icon (if it has one), the label, and the shortcut on the right.
pub fn item(ui: &mut Ui, label: &str, shortcut: String, icon: Option<Icon>) -> egui::Response {
    let slot = Id::new(("menu-icon", label));
    let button = Button::new((Atom::custom(slot, Vec2::splat(ICON_SLOT)), label)).shortcut_text(shortcut);
    let out = button.atom_ui(ui);
    if let (Some(icon), Some(rect)) = (icon, out.rect(slot)) {
        let ink = ui.style().interact(&out.response).fg_stroke.color;
        theme::paint_icon(ui.painter(), rect, icon, ink);
    }
    out.response
}

/// A submenu, with the same icon slot as [`item`] so its label lines up with theirs.
pub fn submenu(ui: &mut Ui, label: &str, icon: Option<Icon>, content: impl FnOnce(&mut Ui)) {
    let (response, _) = SubMenuButton::new((Atom::custom(Id::new(("menu-icon", label)), Vec2::splat(ICON_SLOT)), label)).ui(ui, content);
    if let Some(icon) = icon {
        let left = response.rect.left() + ui.spacing().button_padding.x;
        let rect = egui::Rect::from_center_size(egui::pos2(left + ICON_SLOT / 2.0, response.rect.center().y), Vec2::splat(ICON_SLOT));
        theme::paint_icon(ui.painter(), rect, icon, ui.style().interact(&response).fg_stroke.color);
    }
}

/// The icon an action's menu item wears, where there is a fitting one.
pub fn icon_of(action: Action) -> Option<Icon> {
    use Action as A;
    Some(match action {
        A::Tool(t) => tool_icon(t),
        A::Undo => Icon::Restore,
        A::Redo => Icon::Redo,
        A::Cut => Icon::Cut,
        A::Copy => Icon::Copy,
        A::Paste => Icon::Paste,
        A::Duplicate | A::SaveAs => Icon::Duplicate,
        A::Save => Icon::Save,
        A::CopyProps => Icon::CopyProps,
        A::PasteProps => Icon::PasteProps,
        A::Forward => Icon::Forward,
        A::Backward => Icon::Backward,
        A::ToFront => Icon::ToFront,
        A::ToBack => Icon::ToBack,
        A::FlipH => Icon::FlipH,
        A::FlipV => Icon::FlipV,
        A::Flatten => Icon::Flatten,
        A::ToggleSnapping => Icon::Magnet,
        A::ExportPresentation => Icon::Presentation,
        A::Delete => Icon::Trash,
        A::SelectAll => Icon::Pointer,
        A::Group => Icon::Group,
        A::Ungroup => Icon::Ungroup,
        A::WrapInFrame => Icon::Frame,
        A::CreateComponent => Icon::Component,
        A::DetachInstance => Icon::Instance,
        A::PlaceImage => Icon::Image,
        A::ExportPng | A::ExportSvg | A::ExportPdf => Icon::Export,
        A::CopyAsPng => Icon::Image,
        A::CopyAsSvg => Icon::PenTool,
        A::Home => Icon::Grid,
        A::New => Icon::Plus,
        A::Open => Icon::Page,
        A::Align(0, v) => align_x(v),
        A::Align(_, v) => align_y(v),
        A::Distribute(0) => Icon::DistributeH,
        A::Distribute(_) => Icon::DistributeV,
        A::ToggleVisible => Icon::Eye,
        A::ToggleLock => Icon::Lock,
        A::AddAutoLayout => Icon::LayoutAdd,
        A::RemoveAutoLayout => Icon::LayoutRemove,
        A::Boolean(op) => bool_icon(op),
        A::Bold => Icon::Bold,
        A::Italic => Icon::Italic,
        A::Underline => Icon::Underline,
        A::Strikethrough => Icon::Strikethrough,
        A::Rename => Icon::Pencil,
        A::ToggleSketch => Icon::Sketch,
        A::CommandPalette | A::Ask => Icon::Search,
        A::ToggleUi => Icon::Eye,
        A::TogglePanels => Icon::Sidebar,
        A::ToggleGrid => Icon::Grid,
        A::BreakpointPreview => Icon::Scale,
        A::ToggleAnnotations => Icon::Note,
        A::OpenVariables => Icon::Database,
        A::ToggleHelp => Icon::Question,
        A::ZoomIn => Icon::Plus,
        A::ZoomOut => Icon::Minus,
        A::Zoom100 => Icon::Zoom100,
        A::ZoomFit => Icon::Corners,
        A::ZoomSelection => Icon::ZoomSelection,
        A::NextPage => Icon::PageNext,
        A::PrevPage => Icon::PagePrev,
        A::ToggleChat => Icon::Zap,
        _ => return None,
    })
}

fn tool_icon(t: Tool) -> Icon {
    match t {
        Tool::Select => Icon::Pointer,
        Tool::Hand => Icon::Hand,
        Tool::Frame => Icon::Frame,
        Tool::Rect => Icon::Rect,
        Tool::Ellipse => Icon::Ellipse,
        Tool::Polygon => Icon::Polygon,
        Tool::Star => Icon::Star,
        Tool::Diamond => Icon::Diamond,
        Tool::Line => Icon::Line,
        Tool::Arrow => Icon::Arrow,
        Tool::Pen => Icon::Pen,
        Tool::Text => Icon::Text,
        Tool::Sticky => Icon::Sticky,
        Tool::Pencil => Icon::Pencil,
        Tool::Eraser => Icon::Eraser,
        Tool::Laser => Icon::Laser,
        Tool::Annotate => Icon::Note,
    }
}

fn align_x(at: f64) -> Icon {
    if at < 0.25 {
        Icon::AlignLeft
    } else if at > 0.75 {
        Icon::AlignRight
    } else {
        Icon::AlignHCenter
    }
}

fn align_y(at: f64) -> Icon {
    if at < 0.25 {
        Icon::AlignTop
    } else if at > 0.75 {
        Icon::AlignBottom
    } else {
        Icon::AlignVCenter
    }
}

fn bool_icon(op: BoolOp) -> Icon {
    match op {
        BoolOp::Union => Icon::Union,
        BoolOp::Subtract => Icon::Subtract,
        BoolOp::Intersect => Icon::Intersect,
        BoolOp::Exclude => Icon::Exclude,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::shortcuts::{self, MenuEntry};

    #[test]
    fn every_tool_has_an_icon_and_alignments_follow_their_edge() {
        for t in Tool::ALL {
            assert!(icon_of(Action::Tool(t)).is_some(), "{t:?}");
        }
        assert_eq!(icon_of(Action::Align(0, 0.0)), Some(Icon::AlignLeft));
        assert_eq!(icon_of(Action::Align(0, 1.0)), Some(Icon::AlignRight));
        assert_eq!(icon_of(Action::Align(1, 0.5)), Some(Icon::AlignVCenter));
        assert_eq!(icon_of(Action::Escape), None);
    }

    fn actions(entries: &[MenuEntry], out: &mut Vec<Action>) {
        for entry in entries {
            match entry {
                MenuEntry::Item(_, action) => out.push(*action),
                MenuEntry::Submenu(_, items) => actions(items, out),
                MenuEntry::Separator => {}
            }
        }
    }

    #[test]
    fn every_menu_item_has_an_icon_of_its_own() {
        for (title, entries) in shortcuts::menus() {
            let mut found = Vec::new();
            actions(&entries, &mut found);
            let mut seen = Vec::new();
            for action in found {
                let icon = icon_of(action);
                assert!(icon.is_some(), "{title}: {action:?}");
                // Every format of the export dialog is the same command.
                let exporting = icon == Some(Icon::Export);
                assert!(exporting || !seen.contains(&icon), "{title}: {action:?} repeats an icon");
                seen.push(icon);
            }
        }
    }
}
