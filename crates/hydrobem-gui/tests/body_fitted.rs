//! Choosing the Body-Fitted Grid for a body whose section is in parts (three columns of one body)
//! rings the convex hull of the section with inside rings between the columns; the
//! Details warn when Convex Outline is off.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gui_core::field_layout::GridLayout;
use hydrobem_gui::app::{App, Settings};
use hydrobem_gui::tree::Node;

/// A file of this crate's test data (tests/data/README.md).
fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)
}

#[test]
fn a_section_in_parts_takes_the_convex_outline() {
    let mut h = Harness::builder()
        .with_size([1440.0, 900.0])
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_max_steps(64)
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_eframe(|cc| App::new(cc));
    h.state_mut().settings = Settings::default();
    // three truncated cylinders as one body: its section three columns apart
    h.state_mut().open_project(&data("three_columns.hbm"));
    {
        let g = &mut h.state_mut().project.field_points;
        // as a new project's grid: no inside rings
        g.layout = GridLayout::Cartesian;
        g.convex = false;
        g.inside_rings = 0;
        // field points on: rings from 2 to 60 m outside the outline, every 5 degrees
        g.enabled = true;
        g.radial_start = 2.0;
        g.radial_end = 60.0;
        g.radial_count = 20;
        g.angle_count = 72;
    }
    h.state_mut().selected = Node::FieldPoints;
    h.run_steps(6);

    // Input → Body-Fitted Grid from its drop-down
    h.get_by_role_and_label(egui::Role::ComboBox, "Input").click();
    h.run_steps(3);
    h.get_by_label("Body-Fitted Grid").click();
    h.run_steps(6);
    let g = &h.state().project.field_points;
    assert_eq!(g.layout, GridLayout::BodyFitted);
    assert!(g.convex, "the columns are ringed as one");
    assert_eq!(g.inside_rings, 10, "points between the columns");
    let points = g.generate();
    let centre = g.profile.as_ref().expect("the outline").centre;
    let inner =
        points.iter().filter(|p| ((p[0] - centre[0]).powi(2) + (p[1] - centre[1]).powi(2)).sqrt() < 20.0).count();
    assert!(inner > 0, "points between the columns: {} points about {centre:?}", points.len());

    // off again: the Details say why it was on
    h.state_mut().project.field_points.convex = false;
    h.run_steps(4);
    h.get_by_label_contains("separate parts");
}
