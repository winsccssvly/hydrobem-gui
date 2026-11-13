//! `HydroBEM.exe --untitled model.hbm` (`hydrobem_gui.show` of a model): the model opens as a new
//! unsaved project, its file neither kept nor remembered.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use hydrobem_gui::app::{App, Settings};

/// A file of this crate's test data (tests/data/README.md).
fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)
}

#[test]
fn a_model_handed_over_opens_unsaved() {
    let mut h = Harness::builder()
        .with_size([1440.0, 900.0])
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .wgpu()
        .build_eframe(|cc| App::new(cc));
    h.state_mut().settings = Settings::default();
    h.state_mut().open_untitled(&data("cylinder.hbm"));
    h.run_steps(3);
    let app = h.state();
    assert!(app.file.is_none(), "Save asks for a file");
    assert!(app.is_modified(), "closing asks to save");
    assert!(!app.project.bodies.is_empty());
    assert!(app.settings.recent.is_empty(), "not a recent file");
}
