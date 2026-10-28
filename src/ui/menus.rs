//! The menu bar's building blocks: top-level menus that open a little below their button, and
//! items with a leading icon.

use crate::editor::Tool;
use crate::editor::shortcuts::Action;
use crate::editor::vector_ops::BoolOp;
use crate::theme::{self, Icon};
use egui::{Atom, Button, Id, InnerResponse, Popup, UiKind, UiStackInfo, Ui, Vec2};
use egui::containers::menu::MenuConfig;

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

/// The icon an action's menu item wears, where there is a fitting one.
pub fn icon_of(action: Action) -> Option<Icon> {
    use Action as A;
    Some(match action {
        A::Tool(t) => tool_icon(t),
        A::Undo => Icon::Restore,
        A::Delete => Icon::Trash,
        A::SelectAll => Icon::Pointer,
        A::Group | A::Ungroup | A::WrapInFrame => Icon::Group,
        A::CreateComponent => Icon::Component,
        A::DetachInstance => Icon::Instance,
        A::PlaceImage => Icon::Image,
        A::ExportPng | A::ExportSvg | A::ExportPdf | A::CopyAsPng | A::CopyAsSvg => Icon::Export,
        A::Home => Icon::Grid,
        A::New => Icon::Plus,
        A::Open | A::SaveAs => Icon::Page,
        A::Align(0, v) => align_x(v),
        A::Align(_, v) => align_y(v),
        A::Distribute(0) => Icon::DistributeH,
        A::Distribute(_) => Icon::DistributeV,
        A::ToggleVisible => Icon::Eye,
        A::ToggleLock => Icon::Lock,
        A::AddAutoLayout | A::RemoveAutoLayout => Icon::Wrap,
        A::Boolean(op) => bool_icon(op),
        A::Bold | A::Italic | A::Underline | A::Strikethrough | A::Rename => Icon::Text,
        A::ToggleSketch => Icon::Sketch,
        A::CommandPalette | A::Ask => Icon::Search,
        A::ToggleUi | A::TogglePanels => Icon::Sidebar,
        A::ToggleGrid => Icon::Grid,
        A::ToggleAnnotations => Icon::Note,
        A::OpenVariables => Icon::Database,
        A::ToggleHelp => Icon::Question,
        A::ZoomIn => Icon::Plus,
        A::ZoomOut => Icon::Minus,
        A::Zoom100 | A::ZoomFit | A::ZoomSelection => Icon::Scale,
        A::NextPage | A::PrevPage => Icon::Page,
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
        BoolOp::Union => Icon::Group,
        BoolOp::Subtract | BoolOp::Intersect | BoolOp::Exclude => Icon::Corners,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
