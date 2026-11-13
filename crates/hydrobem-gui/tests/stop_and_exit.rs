//! Exit while a solve runs: HydroBEM asks first, stops the solve and exits once it has stopped,
//! without waiting for the engine in the meantime.

use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use hydrobem_gui::app::{App, FileAction, Settings};

/// A file of this crate's test data (tests/data/README.md).
fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)
}

#[test]
fn exit_while_solving_asks_then_stops_and_exits() {
    let mut h = Harness::builder()
        .with_size([1440.0, 900.0])
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_max_steps(64)
        .wgpu()
        .build_eframe(|cc| App::new(cc));
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("three_truncated_cylinders.hbm"));
    h.run_steps(4);
    // enough frequencies to be caught running
    let p = &mut h.state_mut().project;
    // the example solves one frequency: 24 over a range
    (p.waves.frequencies.start, p.waves.frequencies.end) = (0.3, 2.0);
    p.waves.frequencies.count = 24;
    // the progress of the frequencies, not of the field points (which slow each of them)
    p.field_points.enabled = false;
    p.waves.frequencies.rebuild();
    h.state_mut().run();
    for _ in 0..400 {
        h.step();
        if h.state().is_solving() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(h.state().is_solving(), "the solve started");

    // asked first; Cancel goes on solving
    h.state_mut().request(FileAction::Exit);
    h.run_steps(3);
    assert_eq!(h.state().modal_text().as_deref(), Some("stop and exit"));
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.state().is_solving() && !h.state().will_close());

    // Stop and Exit: the solve stops, then the exit goes on: the frequencies changed above, so it
    // asks about them, and Discard closes the window
    h.state_mut().request(FileAction::Exit);
    h.run_steps(3);
    h.get_by_label("Stop and Exit").click();
    for _ in 0..4000 {
        h.step();
        if !h.state().is_solving() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    h.run_steps(3);
    assert!(!h.state().is_solving() && !h.state().will_close());
    assert_eq!(h.state().modal_text().as_deref(), Some("unsaved"));
    h.get_by_label("Discard").click();
    h.run_steps(3);
    assert!(h.state().will_close(), "{}", h.state().status);
}
