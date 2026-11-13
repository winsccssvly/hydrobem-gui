//! The GUI smoke test: the real application driven through egui_kittest and drawn by wgpu, with
//! screenshots in target/smoke/ for review (AGENTS.md). Needs a GPU adapter (any D3D12/Vulkan, or
//! the WARP software adapter).

use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gui_core::hydrobem_engine::PanelRole;
use gui_core::mesh::Symmetry;
use gui_core::project::{ControlMethod, ControlSurface, PanelSurface};
use gui_core::results::FieldKind;
use hydrobem_gui::app::{App, FileAction, Settings};
use hydrobem_gui::output::{Cell, PlotSpec};
use hydrobem_gui::tree::{BodyPart, Node, ResultNode};
use hydrobem_gui::viewport::camera::View;

/// The free-surface plane's cells (two triangles each).
const FREE_SURFACE: usize = gui_core::scene::FREE_SURFACE_CELLS * gui_core::scene::FREE_SURFACE_CELLS;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).expect("the repository").to_path_buf()
}

/// A file of this crate's test data (tests/data/README.md).
fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)
}

fn shots() -> PathBuf {
    let dir = root().join("target/smoke");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn harness() -> Harness<'static, App> {
    Harness::builder()
        .with_size([1440.0, 900.0])
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_max_steps(64)
        .wgpu()
        .build_eframe(|cc| App::new(cc))
}

fn settle(h: &mut Harness<'_, App>) {
    h.run_steps(6);
}

fn select(h: &mut Harness<'_, App>, node: Node) {
    h.state_mut().selected = node;
    settle(h);
}

fn shot(h: &mut Harness<'_, App>, name: &str) {
    let image = h.render().expect("a rendered frame");
    image.save(shots().join(format!("{name}.png"))).expect("a screenshot");
}

fn solve(h: &mut Harness<'_, App>) {
    // Run starts two frames later: wait for the new result, not the last one
    h.state_mut().result = None;
    h.state_mut().run();
    let mut started = false;
    for _ in 0..4000 {
        h.step();
        started |= h.state().is_solving();
        // done, or ended without a result (a failure: the assert below says why)
        if !h.state().is_solving() && (h.state().result.is_some() || started) {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert!(h.state().result.is_some(), "the solve finished: {} {:?}", h.state().status, h.state().modal_text());
    settle(h);
}

fn table_rows(h: &Harness<'_, App>) -> usize {
    h.state().output().table.as_ref().map_or(0, |t| t.rows.len())
}

#[test]
fn smoke() {
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    // the first item of the tree is selected at the start, and again for a project opened
    assert_eq!(h.state().selected, Node::Meshes);
    h.state_mut().selected = Node::Environment;
    let app = h.state_mut();
    app.open_project(&data("cylinder.hbm"));
    assert!(app.modal_text().is_none(), "{:?}", app.modal_text());
    settle(&mut h);
    assert_eq!(h.state().selected, Node::Meshes);

    // 01: body details with its mesh; the viewport shows the body alone
    select(&mut h, Node::Body(0));
    h.get_by_label("Excitation Method");
    h.get_by_label("Inertia Properties");
    // the body alone with the free surface (one cuboid), without the field plane
    assert_eq!(h.state().viewport.scene.transparent.len(), 768 * 6 + FREE_SURFACE * 6);
    shot(&mut h, "01-body-details");

    // 01c: the mesh library: the mesh alone, its geometry and its panels
    assert_eq!(h.state().project.meshes.len(), 1);
    select(&mut h, Node::Mesh(0));
    h.get_by_label("Geometry (mesh coordinates, symmetry expanded)");
    assert_eq!(table_rows(&h), 192);
    shot(&mut h, "01c-mesh-details");
    select(&mut h, Node::Meshes);
    assert!(!h.state().viewport.scene.has_content, "the library shows no mesh until one is chosen");
    assert_eq!(table_rows(&h), 1, "the library table: a row per mesh");
    shot(&mut h, "01f-meshes-table");

    // 01d: Bodies shows everything (the body and the field-point plane)
    select(&mut h, Node::Bodies);
    assert!(h.state().viewport.scene.transparent.len() > 768 * 6);
    shot(&mut h, "01d-bodies");

    // 01e: a panel range of the body mesh and body children added by right-click
    {
        let b = &mut h.state_mut().project.bodies[0];
        // without the interior surface the model detects (drawn with the hull)
        b.interior.method = gui_core::project::InteriorMethod::None;
        b.mesh_range = Some([1, 54]);
        b.dipoles.push(Default::default());
        b.dipoles.push(Default::default());
        b.additional_damping = Some(Default::default());
    }
    select(&mut h, Node::BodyPart(0, BodyPart::Dipole(1)));
    h.get_by_label("Dipole 2");
    select(&mut h, Node::Body(0));
    // records 2-54 wetted (record 1 is the dipoles' default range), and the two dipoles on record 1,
    // each in four symmetry copies
    assert_eq!(h.state().viewport.scene.transparent.len(), (53 + 2) * 4 * 6 + FREE_SURFACE * 6);
    shot(&mut h, "01e-body-range-and-parts");
    {
        let b = &mut h.state_mut().project.bodies[0];
        b.mesh_range = None;
        b.dipoles.clear();
        b.additional_damping = None;
    }
    h.state_mut().project.update_mesh_properties();

    // 01b: the graded interior surface the engine builds, in the viewport
    h.state_mut().project.bodies[0].interior.method = gui_core::project::InteriorMethod::Graded;
    settle(&mut h);
    shot(&mut h, "01b-irr-graded");
    h.state_mut().project.bodies[0].interior.method = gui_core::project::InteriorMethod::None;

    // the frequency table, tabbed output
    select(&mut h, Node::Frequencies);
    assert_eq!(table_rows(&h), 9);
    let t = h.state().output().table.clone().unwrap();
    assert_eq!(t.columns.len(), 5);
    assert!(matches!(t.rows[0][3], Cell::Num(k) if (k - 0.4 * 0.4 / 9.80665).abs() < 1e-9));

    // solve: the results summary, the added mass, the excitation
    solve(&mut h);
    assert!(matches!(h.state().selected, Node::Result(ResultNode::Summary)));
    assert!(table_rows(&h) > 10, "the available quantities");
    shot(&mut h, "02-results-summary");

    select(&mut h, Node::Result(ResultNode::Hydrostatics));
    {
        // GM of the cylinder R = 1 m, T = 0.5 m, G at -0.2: KB = -0.25, BM = I / V = R^2 / (4 T) = 0.5,
        // GM = 0.45
        let r = h.state().result.clone().unwrap();
        let s = &r.hydrostatics[0];
        let gm = s.stiffness[21] / (s.stiffness[14] / s.waterplane_area * s.volume[0]);
        // the engine integrates the waterplane from the panels: within 1 % of the exact value
        assert!((gm - 0.45).abs() < 0.01 * 0.45, "{gm}");
    }
    assert_eq!(table_rows(&h), 6);
    shot(&mut h, "02b-hydrostatics");

    select(&mut h, Node::Result(ResultNode::AddedMass));
    assert_eq!(table_rows(&h), 9);
    assert!(h.state().output().plot.is_some());
    shot(&mut h, "03-added-mass");

    select(&mut h, Node::Result(ResultNode::Excitation));
    assert_eq!(h.state().output().table.as_ref().unwrap().columns.len(), 6);
    shot(&mut h, "04-excitation");
    // the same force by the Haskind relation, read through the radiation columns
    select(&mut h, Node::Result(ResultNode::Haskind));
    let t = h.state().output().table.clone().expect("the Haskind table");
    assert_eq!((t.columns.len(), t.rows.len()), (6, 9));
    assert!(h.state().output().title.starts_with("Excitation · Haskind"), "{}", h.state().output().title);
    shot(&mut h, "04b-excitation-haskind");

    // the free-surface total: the heatmap and its table
    select(&mut h, Node::Result(ResultNode::Field(FieldKind::Total)));
    let points = h.state().result.as_ref().unwrap().field_points.len();
    assert_eq!(table_rows(&h), points);
    assert!(h.state().viewport.scene.color_scale.is_some(), "a heatmap with its colour bar");
    shot(&mut h, "05-free-surface-total");

    // a picked row marks its point
    h.state_mut().viewport.highlight = Some(10);
    settle(&mut h);
    shot(&mut h, "05b-field-point-picked");

    // 05c: the total animated (paused at a quarter period): the surface raised by its elevation,
    // the body moved by its motion, coloured about zero
    {
        let s = &mut h.state_mut().selection;
        s.animate = true;
        s.paused = true;
        // 2 rad/s: a wave length of about 15 m over the 12 m grid
        s.frequency = 8;
    }
    h.state_mut().animation_phase = std::f64::consts::FRAC_PI_2;
    settle(&mut h);
    let scale = h.state().viewport.scene.color_scale.clone().expect("the animated field's colour bar");
    assert!(scale.label.starts_with("Elevation") && (scale.min + scale.max).abs() < 1e-9, "{scale:?}");
    h.get_by_label("Radiating Bodies");
    // the player at the bottom of the viewport, paused at the phase set
    let player = h.state().viewport.player.expect("the player bar");
    assert!(player.paused && (player.phase - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    shot(&mut h, "05c-total-animated");
    // Move Bodies off: the same surface, the body kept still
    let response = |h: &Harness<'_, App>| {
        let app = h.state();
        let node = ResultNode::Field(FieldKind::Total);
        hydrobem_gui::results_view::animation_response(node, app.result.as_ref().unwrap(), &app.selection, &app.project)
            .expect("the animated Total")
    };
    assert!(!response(&h).bodies.is_empty(), "the body moves by its motion");
    h.state_mut().selection.still_bodies = true;
    settle(&mut h);
    let still = response(&h);
    assert!(still.bodies.is_empty() && still.field.is_some(), "the body still, the surface animated");
    h.state_mut().selection.still_bodies = false;

    // 05d: the motions animated with the incident wave
    select(&mut h, Node::Result(ResultNode::Motion));
    assert!(h.state().viewport.scene.legend.iter().any(|(n, _)| *n == "Incident Wave"));
    shot(&mut h, "05d-motions-animated");
    h.state_mut().selection.animate = false;
    settle(&mut h);

    // 05e: Clear Results in the top bar forgets the solve; the result views say so; Run solves again
    let kept = h.state().result.clone();
    h.get_by_label("Clear Results").click();
    settle(&mut h);
    assert!(h.state().result.is_none());
    assert!(matches!(h.state().selected, Node::Result(ResultNode::Summary)));
    shot(&mut h, "05e-results-cleared");
    h.state_mut().result = kept;
    select(&mut h, Node::Result(ResultNode::Field(FieldKind::Total)));

    // tabs layout
    h.state_mut().settings.output_layout = hydrobem_gui::output::OutputLayout::Tabs;
    select(&mut h, Node::Result(ResultNode::AddedMass));
    shot(&mut h, "06-frequencies-tabbed");
    h.state_mut().settings.output_layout = hydrobem_gui::output::OutputLayout::Split;

    // results file round trip
    let dir = std::env::temp_dir().join(format!("hydrobem-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let hbr = dir.join("run.hbr");
    assert!(h.state_mut().save_results_to(&hbr));
    h.state_mut().request(FileAction::New);
    settle(&mut h);
    assert!(h.state().result.is_none() && h.state().project.bodies.is_empty());
    h.state_mut().open_results_file(&hbr);
    settle(&mut h);
    assert!(h.state().result.is_some());
    shot(&mut h, "06b-opened-results");

    // project round trip with an unsaved-change prompt answered by Save
    let hbm = dir.join("saved.hbm");
    h.state_mut().open_project(&data("two_truncated_cylinders.hbm"));
    h.state_mut().project.bodies[0].position[0] = 0.5;
    settle(&mut h);
    assert!(h.state().is_modified());
    assert!(h.state_mut().save_to(&hbm));
    assert!(!h.state().is_modified());
    h.state_mut().project.bodies[1].visible = false;
    settle(&mut h);
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::Open(Some(hbm.clone())));
    settle(&mut h);
    assert_eq!(h.state().project.bodies[0].position[0], 0.5);
    assert!(h.state().project.bodies[1].visible, "the change was discarded");
    assert_eq!(h.state().settings.recent[0], std::path::absolute(&hbm).unwrap());
    select(&mut h, Node::Body(1));
    shot(&mut h, "07-reopened-project");

    // models with their own layers
    for (file, name, node) in [
        ("cylinder_with_heave_plate.hbm", "08-example-heave-plate-dipole", Node::BodyPart(0, BodyPart::Dipole(0))),
        ("cylinder_wall.hbm", "09-example-cylinder-wall", Node::Environment),
        ("cylinder_channel.hbm", "10-example-cylinder-channel", Node::Options),
        ("floating_owc_chamber_lid.hbm", "11-example-owc-chamber-lid", Node::Lid(0)),
        ("floating_owc_chamber.hbm", "12-example-owc-chamber", Node::BodyPart(0, BodyPart::Pressure(0))),
        ("trimmed_moonpool_cover.hbm", "12b-example-generated-plate", Node::BodyPart(0, BodyPart::Plate(0))),
        ("fish_tank_covered.hbm", "12c-example-tank-plate", Node::BodyPart(0, BodyPart::Tank(0))),
    ] {
        h.state_mut().test_answer_unsaved = Some(false);
        h.state_mut().request(FileAction::Open(Some(data(file))));
        settle(&mut h);
        assert!(h.state().modal_text().is_none(), "{file}: {:?}", h.state().modal_text());
        select(&mut h, node);
        shot(&mut h, name);
        if file == "trimmed_moonpool_cover.hbm" {
            h.get_by_label_contains("The engine makes the plate");
            assert!(
                h.state().viewport.scene.legend.iter().any(|(n, _)| *n == "Free Surface Plate"),
                "the engine's generated plate drawn"
            );
        }
        if file == "fish_tank_covered.hbm" {
            h.get_by_label("Tank 1 · Free Surface Plate 1");
        }
    }
    // 10b: the channel with a field-point grid wider than it: the walls keep the water column's
    // height, the free surface reaches the walls around the grid's plane
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::Open(Some(data("cylinder_channel.hbm"))));
    settle(&mut h);
    {
        let g = &mut h.state_mut().project.field_points;
        g.enabled = true;
        g.list = None;
        (g.x_start, g.x_end, g.x_count, g.y_start, g.y_end, g.y_count, g.z) = (-10.0, 10.0, 41, -10.0, 10.0, 41, 0.0);
    }
    select(&mut h, Node::FieldPoints);
    shot(&mut h, "10b-channel-with-field-points");

    // 10c, 10d: rings of field points about the cylinder, Polar and Body-Fitted:
    // the heatmap of the Total starts at the first ring, next to the hull
    for (file, layout, name) in [
        ("cylinder.hbm", gui_core::field_layout::GridLayout::Polar, "10c-polar-grid-total"),
        ("cylinder.hbm", gui_core::field_layout::GridLayout::BodyFitted, "10d-body-fitted-grid-total"),
    ] {
        h.state_mut().test_answer_unsaved = Some(false);
        h.state_mut().request(FileAction::Open(Some(data(file))));
        settle(&mut h);
        {
            let p = &mut h.state_mut().project;
            let g = &mut p.field_points;
            (g.enabled, g.list, g.layout, g.z) = (true, None, layout, 0.0);
            (g.radial_count, g.angle_start, g.angle_end, g.angle_count) = (16, 0.0, 360.0, 72);
            if layout == gui_core::field_layout::GridLayout::Polar {
                assert!(p.fit_polar_grid(), "the cylinder's section");
            } else {
                p.update_field_profile();
                let r = p.field_points.profile.as_ref().expect("the cylinder's section").max_radius();
                (p.field_points.radial_start, p.field_points.radial_end) = (0.1 * r, 2.0 * r);
            }
        }
        select(&mut h, Node::FieldPoints);
        assert_eq!(h.state().project.field_points.generate().len(), 16 * 72);
        shot(&mut h, &format!("{name}-details"));
        solve(&mut h);
        select(&mut h, Node::Result(ResultNode::Field(FieldKind::Total)));
        assert!(h.state().viewport.scene.color_scale.is_some(), "{file}: the heatmap");
        let bounds = h.state().viewport.scene.bounds;
        h.state_mut().viewport.camera.set_view(View::Top, bounds);
        settle(&mut h);
        shot(&mut h, name);
    }

    // 10e-10g: the three cylinders of examples/three_truncated_cylinders on a triangle: rings about
    // their sections (each ray to its outermost column: a star), about their convex outline, and the
    // Total
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::Open(Some(data("three_truncated_cylinders.hbm"))));
    settle(&mut h);
    assert!(h.state().project.field_points.convex);
    for (convex, name) in [(false, "10e-three-cylinders-section-rings"), (true, "10f-three-cylinders-convex-rings")] {
        h.state_mut().project.field_points.convex = convex;
        select(&mut h, Node::FieldPoints);
        let bounds = h.state().viewport.scene.bounds;
        h.state_mut().viewport.camera.set_view(View::Top, bounds);
        settle(&mut h);
        shot(&mut h, name);
    }
    let s = gui_core::field_points::select_field_points(&h.state().project);
    // the inside rings reach between the columns; their points in the columns are left out
    assert_eq!(s.generated, 30 * 72);
    assert!(s.inside_bodies > 0 && s.rejected == 0, "{} in the columns", s.inside_bodies);
    assert!(s.points.iter().any(|p| p[0].hypot(p[1]) < 20.0), "points between the columns");
    solve(&mut h);
    select(&mut h, Node::Result(ResultNode::Field(FieldKind::Total)));
    assert!(h.state().viewport.scene.color_scale.is_some());
    let bounds = h.state().viewport.scene.bounds;
    h.state_mut().viewport.camera.set_view(View::Top, bounds);
    settle(&mut h);
    shot(&mut h, "10g-three-cylinders-convex-rings-total");

    // 10g2: the three truncated cylinders as analytic patches (higher order): the rings about the
    // three columns
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::Open(Some(data("three_truncated_cylinders_patches.hbm"))));
    settle(&mut h);
    assert!(h.state().modal_text().is_none(), "{:?}", h.state().modal_text());
    assert!(h.state().project.meshes[0].mesh.is_patch());
    select(&mut h, Node::FieldPoints);
    let g = &h.state().project.field_points;
    assert!(g.convex && g.profile.is_some());
    shot(&mut h, "10g2-patches-rings");
    // 10g3: the patch mesh alone, with the spans of its discretization (nu x nv per patch)
    select(&mut h, Node::Mesh(0));
    assert!(h.state().viewport.scene.legend.iter().any(|(name, _)| *name == "Spans (nu x nv)"));
    shot(&mut h, "10g3-patch-spans");
    // View → Show Spans off: the patches alone; on again
    h.state_mut().settings.show_spans = false;
    h.run_steps(3);
    assert!(!h.state().viewport.scene.legend.iter().any(|(name, _)| *name == "Spans (nu x nv)"));
    shot(&mut h, "10g3b-patches-without-spans");
    h.state_mut().settings.show_spans = true;
    h.run_steps(3);

    // 10h: an analytic mesh made in the GUI: a new cylinder, then a moonpool cylinder with its lid,
    // the shape in its Details; a body that takes it keeps Detect, which takes the lid (in z = 0)
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::New);
    settle(&mut h);
    h.state_mut().new_analytic_mesh();
    settle(&mut h);
    assert!(matches!(h.state().selected, Node::Mesh(0)));
    h.get_by_label("Analytic Shape");
    {
        let p = &mut h.state_mut().project;
        let spec = gui_core::analytic::AnalyticSpec::of(gui_core::analytic::AnalyticKind::MoonpoolCylinder);
        let mesh = spec.rebuild(&p.meshes[0].mesh).expect("the moonpool cylinder");
        p.replace_mesh(0, mesh.clone());
        let b = p.add_body();
        hydrobem_gui::details::assign_mesh(p, hydrobem_gui::details::MeshTarget::Body(b), Some(mesh));
        assert_eq!(p.bodies[b].interior.method, gui_core::project::InteriorMethod::Detect);
        assert_eq!(p.bodies[b].wetted_analytic_lid(), None, "Detect takes the moonpool's lid");
    }
    select(&mut h, Node::Mesh(0));
    shot(&mut h, "10h-analytic-moonpool-cylinder");
    // IRR off on the body while the mesh keeps its lid: the lid would be hull, the Details warn
    h.state_mut().project.bodies[0].interior.method = gui_core::project::InteriorMethod::None;
    select(&mut h, Node::Body(0));
    h.get_by_label_contains("solved as hull at z = 0");
    shot(&mut h, "10i-analytic-lid-left-wetted");
    // lid and pressure-surface elevations, the added mass limits
    for (file, node, name) in [
        ("floating_owc_chamber_lid.hbm", ResultNode::LidElevation, "21-lid-elevation"),
        ("floating_owc_chamber.hbm", ResultNode::SurfaceElevation, "22-pressure-surface-elevation"),
    ] {
        h.state_mut().test_answer_unsaved = Some(false);
        h.state_mut().request(FileAction::Open(Some(data(file))));
        settle(&mut h);
        // in deep water: the open tube's zero-frequency heave has a net volume flux, its added
        // mass not defined in finite depth (the engine refuses A(0) there)
        h.state_mut().project.environment.depth_type = gui_core::project::DepthType::Infinite;
        {
            let f = &mut h.state_mut().project.waves.frequencies;
            f.inputs.truncate(3);
            f.count = 3;
            f.edit_in_table = true;
            f.zero_limit = true;
            f.infinite_limit = true;
        }
        settle(&mut h);
        solve(&mut h);
        select(&mut h, Node::Result(node));
        assert!(table_rows(&h) > 10, "{file}: {}", table_rows(&h));
        assert!(h.state().viewport.scene.color_scale.is_some(), "{file}: coloured");
        shot(&mut h, name);
    }
    // the limits in Added Mass: two rows after the frequencies, two points on the graph, the
    // matrices of the two bodies as tabs
    select(&mut h, Node::Result(ResultNode::AddedMass));
    {
        let s = h.state_mut().selection_mut();
        (s.force_dof, s.motion_dof) = (2, 2);
    }
    settle(&mut h);
    let out = h.state().output().clone();
    let t = out.table.clone().expect("the added mass table");
    assert_eq!(t.rows.len(), 3 + 2, "three frequencies, A(0) and A(∞)");
    assert!(
        matches!(&t.rows[3][0], Cell::Text(n) if n == "A(0)") && matches!(&t.rows[4][0], Cell::Text(n) if n == "A(∞)")
    );
    assert!(t.rows[3..].iter().all(|r| matches!(r[2], Cell::Num(v) if v.is_finite())));
    match &out.plot {
        Some(PlotSpec::Single { x, marks, .. }) => {
            assert_eq!(marks.len(), 2);
            assert_eq!(marks[0].x, 0.0);
            assert!(marks[1].x > x.iter().copied().fold(f64::MIN, f64::max), "ω = ∞ right of every frequency");
        }
        other => panic!("one graph, not {other:?}"),
    }
    let tabs = out.tabs.clone().expect("the limit matrices as tabs");
    assert_eq!(tabs.names, ["Added Mass", "ω = 0 Matrix", "ω = ∞ Matrix"]);
    assert!(tabs.more.iter().all(|m| m.rows.len() == 6));
    shot(&mut h, "23-added-mass-limits");

    // the OWC chamber with two pressure modes: their Details, then the computed pneumatic damping
    {
        let s = &mut h.state_mut().project.bodies[0].pressure_surfaces[0];
        s.pressure_modes = vec![
            gui_core::project::PressureMode { stiffness: 50.0, ..Default::default() },
            gui_core::project::PressureMode { shape: gui_core::project::PressureShape::SignX, ..Default::default() },
        ];
    }
    select(&mut h, Node::BodyPart(0, BodyPart::Pressure(0)));
    shot(&mut h, "24-pressure-modes");
    solve(&mut h);
    select(&mut h, Node::Result(ResultNode::PressureModeDamping));
    let t = h.state().output().table.clone().expect("the pressure-mode damping");
    assert!(
        t.columns.iter().any(|c| c.header.contains("Pressure 2")),
        "{:?}",
        t.columns.iter().map(|c| &c.header).collect::<Vec<_>>()
    );
    shot(&mut h, "25-pressure-mode-damping");
    // a generalized mode and the mode matrices (Details and tables)
    {
        let b = &mut h.state_mut().project.bodies[0];
        b.generalized_modes.push(gui_core::project::GeneralizedMode {
            name: "Bending".into(),
            pieces: vec![gui_core::project::ModePiece {
                range: None,
                more_ranges: vec![],
                terms: [vec![], vec![], vec![[1.0, 2.0, 0.0, 0.0]]],
            }],
            view_scale: 0.05,
            ..Default::default()
        });
        let mut m = gui_core::project::ModeMatrices::default();
        m.resize(7);
        m.stiffness[48] = 5000.0;
        m.shown = 2;
        b.mode_matrices = Some(m);
    }
    select(&mut h, Node::BodyPart(0, BodyPart::Generalized(0)));
    assert_eq!(table_rows(&h), 1);
    assert!(h.state().viewport.scene.legend.iter().any(|(n, _)| *n == "Displaced Mode"));
    shot(&mut h, "26-generalized-mode");
    select(&mut h, Node::BodyPart(0, BodyPart::ModeMatrices));
    assert_eq!(table_rows(&h), 7);
    shot(&mut h, "27-mode-matrices");

    // a coupling between the two cylinders, the second then suppressed: still solved
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::Open(Some(data("two_truncated_cylinders.hbm"))));
    settle(&mut h);
    {
        let p = &mut h.state_mut().project;
        let other = p.bodies[1].name.clone();
        let mut k = gui_core::project::Matrix6 { edit_in_table: true, ..Default::default() };
        k.set(2, 2, -1.0e5);
        p.bodies[0].couplings.push(gui_core::project::Coupling { body: other, stiffness: k, ..Default::default() });
    }
    select(&mut h, Node::BodyPart(0, BodyPart::Coupling(0)));
    assert_eq!(table_rows(&h), 6);
    shot(&mut h, "28-coupling");
    h.state_mut().project.bodies[1].suppressed = true;
    solve(&mut h);
    let r = h.state().result.as_ref().expect("solved without the suppressed cylinder");
    assert_eq!((r.bodies.len(), r.model_bodies.len()), (1, 2));
    select(&mut h, Node::Body(1));
    shot(&mut h, "28b-suppressed-body-solved");

    // the body pressure coloured on the panels themselves (every symmetry copy)
    h.state_mut().project.bodies[1].suppressed = false;
    h.state_mut().project.first_order.body_pressure = true;
    solve(&mut h);
    select(&mut h, Node::Result(ResultNode::BodyPressure));
    let r = h.state().result.as_ref().unwrap();
    assert!(r.body_point_panels().iter().all(Option::is_some), "every point on its panel");
    shot(&mut h, "28c-body-pressure-on-panels");

    // a higher-order cylinder: its patches drawn by their tessellation, their discretization
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::Open(Some(data("truncated_cylinder_patches.hbm"))));
    settle(&mut h);
    assert!(h.state().modal_text().is_none(), "{:?}", h.state().modal_text());
    select(&mut h, Node::Body(0));
    shot(&mut h, "29-higher-order-body");
    select(&mut h, Node::Mesh(0));
    assert_eq!(table_rows(&h), 2);
    shot(&mut h, "29b-patch-discretization");
    solve(&mut h);
    select(&mut h, Node::Result(ResultNode::AddedMass));
    shot(&mut h, "29c-higher-order-added-mass");
    // the body pressure on the patches: each output cell coloured on the tessellated surface
    h.state_mut().project.first_order.body_pressure = true;
    solve(&mut h);
    select(&mut h, Node::Result(ResultNode::BodyPressure));
    {
        let r = h.state().result.as_ref().unwrap();
        assert!(
            !r.surface.is_empty() && r.body_point_faces().iter().all(|f| !f.is_empty()),
            "every cell on the patches"
        );
    }
    shot(&mut h, "29d-higher-order-body-pressure");
    // animated: Re{p e^(iwt)} on the same faces, a fixed scale about zero
    {
        let s = &mut h.state_mut().selection;
        s.animate = true;
        s.paused = true;
    }
    h.state_mut().animation_phase = 1.0;
    settle(&mut h);
    let scale = h.state().viewport.scene.color_scale.clone().expect("the animated pressure's colour bar");
    assert!(scale.label.starts_with("p/ρg (t)") && (scale.min + scale.max).abs() < 1e-9, "{scale:?}");
    shot(&mut h, "29e-higher-order-body-pressure-animated");
    h.state_mut().selection.animate = false;
    settle(&mut h);

    // field points one by one, at two depths, solved and shown as squares
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::Open(Some(data("cylinder.hbm"))));
    settle(&mut h);
    {
        let g = &mut h.state_mut().project.field_points;
        let mut list = Vec::new();
        for k in 0..12 {
            let a = k as f64 * std::f64::consts::PI / 6.0;
            for (r, z) in [(2.5, 0.0), (3.5, -0.5)] {
                list.push(gui_core::project::ListedPoint { point: [r * a.cos(), r * a.sin(), z], tank: None });
            }
        }
        g.list = Some(list);
    }
    select(&mut h, Node::FieldPoints);
    assert_eq!(table_rows(&h), 24);
    shot(&mut h, "30-field-point-list");
    solve(&mut h);
    select(&mut h, Node::Result(ResultNode::Field(FieldKind::Total)));
    assert!(h.state().viewport.scene.color_scale.is_some());
    shot(&mut h, "30b-field-point-list-solved");

    // the wall: everything drawn on the fluid side
    h.state_mut().request(FileAction::Open(Some(data("cylinder_wall.hbm"))));
    settle(&mut h);
    let scene = h.state().viewport.scene.clone();
    let extent = (scene.bounds.1[0] - scene.bounds.0[0]).max(1.0) as f32;
    assert!(scene.transparent.iter().all(|v| v.position[1] >= -0.006 * extent - 1e-3));

    // the other engine items: a control surface, body points, a water damper, the drift and point
    // outputs; a tank and a body damper in Details
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::Open(Some(data("cylinder.hbm"))));
    settle(&mut h);
    {
        let p = &mut h.state_mut().project;
        let b = &mut p.bodies[0];
        b.control_surface = Some(ControlSurface {
            method: ControlMethod::Automatic,
            radius: 2.5,
            depth: 1.5,
            ..ControlSurface::default()
        });
        b.body_points = Some(vec![[1.0, 0.0, -0.3], [0.0, 1.0, -0.3], [0.7, 0.7, -0.6]]);
        b.tanks.push(Default::default());
        b.dipole_dampers.push(PanelSurface { damping_factor: 0.2, ..PanelSurface::default() });
        b.trim = Some([0.0, 1.0, 0.0]);
        let a = &mut p.first_order;
        a.motion = true;
        a.field_velocity = true;
        a.body_pressure = true;
        a.body_velocity = true;
        let s = &mut p.second_order;
        s.drift_pressure_integration = true;
        s.drift_control_surface = true;
        s.drift_momentum_conservation = true;
        s.drift_bidirectional = true;
        s.qtf_quadratic_pressure_integration = true;
        s.qtf_newman = true;
    }
    select(&mut h, Node::BodyPart(0, BodyPart::Tank(0)));
    h.get_by_label("Tank 1");
    shot(&mut h, "13-tank-details");
    select(&mut h, Node::BodyPart(0, BodyPart::Damper(0)));
    shot(&mut h, "13b-body-dipole-damper");
    {
        let b = &mut h.state_mut().project.bodies[0];
        b.tanks.clear();
        b.dipole_dampers.clear();
        b.trim = None;
    }
    select(&mut h, Node::BodyPart(0, BodyPart::ControlSurface));
    let roles = h.state().derived_roles().expect("the engine builds the problem");
    assert!(roles.bodies[&0].iter().any(|(r, _)| *r == PanelRole::ControlSurface));
    shot(&mut h, "14-control-surface");
    select(&mut h, Node::BodyPart(0, BodyPart::BodyPoints));
    assert_eq!(table_rows(&h), 3);
    shot(&mut h, "15-body-points");
    select(&mut h, Node::FirstOrder);
    assert!(h.query_all_by_label("First Order").count() >= 2, "the tree row and the section");
    shot(&mut h, "16-first-order");
    select(&mut h, Node::SecondOrder);
    assert!(h.query_all_by_label("Drift").count() >= 1);
    shot(&mut h, "16b-second-order");
    solve(&mut h);
    select(&mut h, Node::Result(ResultNode::Drift));
    let t = h.state().output().table.clone().expect("the drift table");
    assert_eq!(t.columns.len(), 2 + 6);
    assert!(t.rows.iter().all(|r| r[2..].iter().all(|c| matches!(c, Cell::Num(v) if v.is_finite()))));
    shot(&mut h, "17-mean-drift");
    // the heading pairs: the fourth method the solve computed (pressure, control surface, momentum, pairs)
    h.state_mut().selection_mut().drift_method = 3;
    settle(&mut h);
    assert!(h.state().output().title.contains("Heading Pairs"), "{}", h.state().output().title);
    shot(&mut h, "17b-drift-heading-pairs");
    h.state_mut().selection_mut().drift_method = 0;
    select(&mut h, Node::Result(ResultNode::Kochin));
    let t = h.state().output().table.clone().expect("the Kochin table");
    assert!(t.rows.len() > 8);
    shot(&mut h, "17c-kochin");
    // the QTF: the matrix of the frequencies, every value finite
    select(&mut h, Node::Result(ResultNode::Qtf));
    let t = h.state().output().table.clone().expect("the QTF table");
    let f = t.rows.len();
    assert!(f > 1 && t.columns.len() == f + 1, "{f} rows, {} columns", t.columns.len());
    assert!(t.rows.iter().all(|r| r[1..].iter().all(|c| matches!(c, Cell::Num(v) if v.is_finite()))));
    assert!(h.state().output().title.contains("Quadratic, Pressure Integration"), "{}", h.state().output().title);
    // the matrix as a heatmap, f x f cells
    match h.state().output().plot.clone() {
        Some(PlotSpec::Heatmap { x, y, values, .. }) => {
            assert!(x.len() == f && y.len() == f && values.len() == f && values.iter().all(|r| r.len() == f))
        }
        other => panic!("a heatmap, not {other:?}"),
    }
    shot(&mut h, "17d-qtf");
    h.state_mut().selection_mut().qtf_row_graph = true;
    settle(&mut h);
    assert!(matches!(h.state().output().plot, Some(PlotSpec::Paired { .. })));
    shot(&mut h, "17d2-qtf-row");
    h.state_mut().selection_mut().qtf_row_graph = false;
    h.state_mut().selection_mut().qtf_sum = true;
    settle(&mut h);
    assert!(h.state().output().title.contains("Sum"), "{}", h.state().output().title);
    shot(&mut h, "17e-qtf-sum");
    // Newman's approximation: the second method the solve computed
    h.state_mut().selection_mut().qtf_method = 1;
    settle(&mut h);
    assert!(h.state().output().title.contains("Newman"), "{}", h.state().output().title);
    shot(&mut h, "17f-qtf-newman");
    h.state_mut().selection_mut().qtf_method = 0;
    select(&mut h, Node::Result(ResultNode::BodyPressure));
    assert_eq!(table_rows(&h), 3);
    let scale = h.state().viewport.scene.color_scale.clone().expect("the hull coloured by the pressure");
    assert!(scale.label.contains("p/ρg") && scale.max > scale.min);
    shot(&mut h, "18-body-pressure");
    select(&mut h, Node::Result(ResultNode::FieldVelocity));
    // the first quantity is the incident velocity at the field points, coloured on their grid
    assert!(table_rows(&h) > 3);
    assert!(h.state().viewport.scene.color_scale.is_some());
    shot(&mut h, "19-field-velocity");
    let body = h.state().output().table.is_some();
    assert!(body);
    select(&mut h, Node::Result(ResultNode::BodyVelocity));
    h.state_mut().selection_mut().point_quantity = 0; // Diffraction
    h.state_mut().selection_mut().velocity_component = 3;
    settle(&mut h);
    assert_eq!(table_rows(&h), 3);
    shot(&mut h, "19b-body-velocity");
    // without body points the body outputs are at the panels: the whole hull coloured
    h.state_mut().project.bodies[0].body_points = None;
    settle(&mut h);
    solve(&mut h);
    select(&mut h, Node::Result(ResultNode::BodyPressure));
    assert!(table_rows(&h) > 100, "{}", table_rows(&h));
    shot(&mut h, "18b-body-pressure-panels");

    // trim: a barge 4 x 1.5 m with 0.4 m freeboard (an open box, quarter mesh), cut by the engine at
    // the free surface: level, then sunk 0.1 m, pitched 5 deg and rolled 3 deg
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::New);
    settle(&mut h);
    let hull =
        gui_core::mesh::Mesh::from_panels("freeboard_barge", Symmetry::XzAndYz, freeboard_box(2.0, 0.75, 1.0, 0.4));
    {
        let p = &mut h.state_mut().project;
        p.add_mesh("freeboard_barge", hull.clone());
        let i = p.add_body();
        p.bodies[i].mesh = Some(hull);
        p.bodies[i].trim = Some([0.0, 0.0, 0.0]);
    }
    for (trim, name) in [([0.0, 0.0, 0.0], "20-trim-level"), ([-0.1, 5.0, 3.0], "20b-trim-pitch-roll")] {
        h.state_mut().project.bodies[0].trim = Some(trim);
        select(&mut h, Node::Body(0));
        let roles = h.state().derived_roles().expect("the engine trims the hull");
        // every wetted panel lies at or below the free surface after the cut
        assert!(roles.bodies[&0].iter().all(|(_, q)| q.iter().all(|v| v[2] <= 1e-9)));
        let bounds = h.state().viewport.scene.bounds;
        h.state_mut().viewport.camera.set_view(View::Iso, bounds);
        settle(&mut h);
        shot(&mut h, name);
    }
    let bounds = h.state().viewport.scene.bounds;
    h.state_mut().viewport.camera.set_view(View::Front, bounds);
    settle(&mut h);
    shot(&mut h, "20c-trim-front");
    h.state_mut().viewport.camera.set_view(View::Right, bounds);
    settle(&mut h);
    shot(&mut h, "20d-trim-right");
}

/// A quarter (x, y >= 0) open box from z = freeboard down to z = freeboard - height; normals out.
fn freeboard_box(a: f64, b: f64, height: f64, freeboard: f64) -> Vec<[[f64; 3]; 4]> {
    let (nx, ny, nz) = (8usize, 3usize, 5usize);
    let x = |i: usize| a * i as f64 / nx as f64;
    let y = |j: usize| b * j as f64 / ny as f64;
    let z = |k: usize| freeboard - height * k as f64 / nz as f64;
    let bottom = freeboard - height;
    let mut out = Vec::new();
    for i in 0..nx {
        for j in 0..ny {
            // seen from below: normal -z
            out.push([
                [x(i), y(j), bottom],
                [x(i), y(j + 1), bottom],
                [x(i + 1), y(j + 1), bottom],
                [x(i + 1), y(j), bottom],
            ]);
        }
    }
    for j in 0..ny {
        for k in 0..nz {
            out.push([[a, y(j), z(k + 1)], [a, y(j + 1), z(k + 1)], [a, y(j + 1), z(k)], [a, y(j), z(k)]]);
        }
    }
    for i in 0..nx {
        for k in 0..nz {
            out.push([[x(i), b, z(k + 1)], [x(i), b, z(k)], [x(i + 1), b, z(k)], [x(i + 1), b, z(k + 1)]]);
        }
    }
    out
}

/// The QTF settings of Outputs, the potential QTF's free surface among them (below the first
/// order and the mean drift in Details: found by their labels in a tall window).
#[test]
fn qtf_settings_in_outputs() {
    let mut h = Harness::builder()
        .with_size([1440.0, 3000.0])
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_max_steps(64)
        .wgpu()
        .build_eframe(|cc| App::new(cc));
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::Open(Some(data("cylinder.hbm"))));
    settle(&mut h);
    {
        let s = &mut h.state_mut().project.second_order;
        s.qtf_quadratic_pressure_integration = true;
        s.qtf_potential_direct = true;
        s.quadrature_annuli = 2;
        s.free_surface_panel_size = Some(0.5);
    }
    select(&mut h, Node::SecondOrder);
    // the sections and the drop-down rows (check and number rows carry no accessible label)
    for label in ["QTF", "Newman Average", "Free Surface (Potential QTF)", "Free Surface Method"] {
        h.get_by_label(label);
    }
    // without the potential QTF its free surface is not shown
    h.state_mut().project.second_order.qtf_potential_direct = false;
    settle(&mut h);
    assert!(h.query_by_label("Free Surface (Potential QTF)").is_none());

    // a free-surface plate of the body: its node and Details (a library mesh, chosen below)
    h.state_mut().project.bodies[0].free_surface_plates.push(Default::default());
    select(&mut h, Node::BodyPart(0, BodyPart::Plate(0)));
    h.get_by_label("Free Surface Plate 1");
    shot(&mut h, "16c-free-surface-plate");
}

/// The total QTF: a quadratic part plus a potential part, the GUI's sum of the engine's two (the
/// methods in order: the quadratic part, the potential part, their total).
#[test]
fn qtf_total_is_the_sum_of_its_parts() {
    let mut h = harness();
    h.state_mut().test_answer_unsaved = Some(false);
    h.state_mut().request(FileAction::Open(Some(data("cylinder.hbm"))));
    settle(&mut h);
    {
        let p = &mut h.state_mut().project;
        p.waves.frequencies.inputs.truncate(3);
        p.waves.frequencies.count = 3;
        p.field_points.enabled = false;
        p.bodies[0].interior.method = gui_core::project::InteriorMethod::None;
        let s = &mut p.second_order;
        s.qtf_quadratic_pressure_integration = true;
        s.qtf_potential_direct = true;
        s.qtf_sum = false;
        s.quadrature_annuli = 1;
        s.free_surface_panel_size = Some(0.5);
    }
    solve(&mut h);
    select(&mut h, Node::Result(ResultNode::Qtf));
    // the real part of each matrix, linear in its parts
    let mut real = |method: usize| {
        let s = h.state_mut().selection_mut();
        (s.qtf_method, s.qtf_part) = (method, 1);
        settle(&mut h);
        let title = h.state().output().title.clone();
        let t = h.state().output().table.clone().expect("the QTF table");
        let values: Vec<f64> = t
            .rows
            .iter()
            .flat_map(|r| r[1..].iter().map(|c| if let Cell::Num(v) = c { *v } else { f64::NAN }))
            .collect();
        (title, values)
    };
    let (quadratic, potential) = (real(0).1, real(1).1);
    let (title, total) = real(2);
    assert!(title.contains("Total, Pressure Integration + Direct"), "{title}");
    assert_eq!(total.len(), 9);
    for ((t, q), p) in total.iter().zip(&quadratic).zip(&potential) {
        assert!((t - (q + p)).abs() <= 1e-9 * (q.abs() + p.abs()).max(1.0), "{t} vs {q} + {p}");
    }
    h.state_mut().selection_mut().qtf_part = 0;
    settle(&mut h);
    shot(&mut h, "17g-qtf-total");
}
