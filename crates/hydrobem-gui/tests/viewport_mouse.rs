//! The viewport by real pointer events: a right drag pans (the camera's target moves, its angles
//! stay) without opening the right-click menu; a right click without a drag opens it; a left drag
//! rotates.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use hydrobem_gui::app::{App, Settings};

fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)
}

fn harness() -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size([1440.0, 900.0])
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_max_steps(64)
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_eframe(|cc| App::new(cc));
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("cylinder.hbm"));
    h.run_steps(6);
    h
}

fn button(h: &mut Harness<'_, App>, pos: egui::Pos2, button: egui::PointerButton, pressed: bool) {
    h.event(egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE });
    h.step();
}

fn drag(h: &mut Harness<'_, App>, with: egui::PointerButton, from: egui::Pos2, by: egui::Vec2) {
    h.event(egui::Event::PointerMoved(from));
    h.step();
    button(h, from, with, true);
    for k in 1..=10 {
        h.event(egui::Event::PointerMoved(from + by * (k as f32 / 10.0)));
        h.step();
    }
    button(h, from + by, with, false);
    h.run_steps(3);
}

/// A point inside the viewport (right of the tree, above the output pane).
const CENTRE: egui::Pos2 = egui::pos2(930.0, 300.0);

#[test]
fn a_right_drag_pans_and_a_right_click_opens_the_menu() {
    let mut h = harness();
    let before = h.state().viewport.camera.clone();
    drag(&mut h, egui::PointerButton::Secondary, CENTRE, egui::vec2(120.0, 60.0));
    let after = h.state().viewport.camera.clone();
    assert_ne!(after.target, before.target, "the right drag pans");
    assert_eq!((after.yaw, after.pitch), (before.yaw, before.pitch), "and does not rotate");
    assert!(h.query_by_label("Fit").is_none(), "no menu after a drag");

    // a right click without a drag: the menu
    button(&mut h, CENTRE, egui::PointerButton::Secondary, true);
    button(&mut h, CENTRE, egui::PointerButton::Secondary, false);
    h.run_steps(3);
    assert!(h.query_by_label("Fit").is_some(), "the menu on a right click");
    h.key_press(egui::Key::Escape);
    h.run_steps(3);

    // a left drag rotates
    let before = h.state().viewport.camera.clone();
    drag(&mut h, egui::PointerButton::Primary, CENTRE, egui::vec2(120.0, 0.0));
    assert_ne!(h.state().viewport.camera.yaw, before.yaw, "the left drag rotates");
}
