//! The acrylic material: the backdrop around the panels glows, the canvas is left alone.
//! Own test binary, as the theme and material are process-wide.
mod common;

use common::*;
use papercraft::demo;
use papercraft::render::frame;
use papercraft::theme::{self, Choice, Material};
use papercraft::ui::UiState;

#[test]
fn acrylic_lights_the_backdrop_and_leaves_the_canvas() {
    let Some(g) = gpu() else { return };
    let mocha = Choice::Catppuccin(craft_ui::theme::catppuccin::Flavor::Mocha);
    theme::apply_choice(&egui::Context::default(), mocha);
    let render = |m: Material, chat: bool| {
        theme::set_material(m);
        let mut e = demo::showcase();
        let mut st = UiState {
            chat_mode: chat,
            ..UiState::default()
        };
        frame::capture(g, &mut e, &mut st, [1440.0, 900.0], 1.0, |e| {
            e.zoom_to_fit();
            // A frame selected, so the inspector shows its fields, selects and buttons.
            let first = e.doc.root[0];
            e.select(vec![first]);
        })
    };
    let solid = render(Material::Solid, false);
    let acrylic = render(Material::Acrylic, false);
    render(Material::Acrylic, true)
        .save(shot("app_acrylic_mocha_chat.png"))
        .unwrap();
    {
        theme::set_material(Material::Acrylic);
        let mut e = demo::showcase();
        let mut st = UiState::default();
        st.settings.open = true;
        frame::capture(g, &mut e, &mut st, [1440.0, 900.0], 2.0, |_| {})
            .save(shot("app_acrylic_mocha_settings.png"))
            .unwrap();
        // Rounder corners and a rounded font, restyled through `apply`.
        theme::set_roundness(theme::Roundness(1.5));
        theme::set_ui_font(5);
        theme::apply_choice(&egui::Context::default(), mocha);
        let mut e = demo::showcase();
        frame::capture(g, &mut e, &mut st, [1440.0, 900.0], 2.0, |_| {})
            .save(shot("app_acrylic_round_settings.png"))
            .unwrap();
        theme::set_roundness(theme::Roundness(1.0));
        theme::set_ui_font(0);
    }
    render(Material::Solid, true)
        .save(shot("app_solid_mocha_chat.png"))
        .unwrap();
    solid.save(shot("app_solid_mocha.png")).unwrap();
    acrylic.save(shot("app_acrylic_mocha.png")).unwrap();
    // The top gap a third of the way across sits under the glow: flat crust when solid,
    // brighter when acrylic.
    let (x, y) = (430, 4);
    let lum = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32;
    assert!(
        lum(acrylic.get_pixel(x, y).0) > lum(solid.get_pixel(x, y).0) + 6,
        "glow in the gap: {:?} vs {:?}",
        acrylic.get_pixel(x, y).0,
        solid.get_pixel(x, y).0
    );
    // The middle of the canvas is the renderer's, untouched by the backdrop.
    assert_eq!(acrylic.get_pixel(720, 450), solid.get_pixel(720, 450));
    theme::set_material(Material::Solid);
}
