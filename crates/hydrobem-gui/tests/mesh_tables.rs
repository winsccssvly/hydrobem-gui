//! The tables of the mesh library: the meshes, a mesh's panels and vertices (or patches and
//! corners), and the panels highlighted in the viewport from the table and picked back.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use hydrobem_gui::app::{App, Settings};
use hydrobem_gui::output::Cell;
use hydrobem_gui::tree::Node;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).expect("the repository").to_path_buf()
}

/// A file of this crate's test data (tests/data/README.md).
fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)
}

fn harness() -> Harness<'static, App> {
    Harness::builder()
        .with_size([1440.0, 900.0])
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_max_steps(64)
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_eframe(|cc| App::new(cc))
}

fn open(h: &mut Harness<'_, App>, file: &str) {
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data(file));
    h.run_steps(4);
}

fn select(h: &mut Harness<'_, App>, node: Node) {
    h.state_mut().selected = node;
    h.run_steps(6);
}

fn value(c: &Cell) -> f64 {
    match c {
        Cell::Int(i) => *i as f64,
        Cell::Num(v) | Cell::Fixed(v, _) => *v,
        Cell::Text(_) => f64::NAN,
    }
}

fn column(h: &Harness<'_, App>, header: &str) -> usize {
    let t = h.state().output().table.as_ref().expect("a table");
    t.columns.iter().position(|c| c.header == header).unwrap_or_else(|| panic!("no column {header}"))
}

#[test]
fn the_library_and_a_panel_mesh_have_their_tables() {
    let mut h = harness();
    open(&mut h, "cylinder.hbm");

    // examples/cylinder: R = 1 m, T = 0.5 m, a quadrant of 16 panels around, 6 down the side and 6
    // across the bottom (the waterplane a regular 64-gon)
    let pi = std::f64::consts::PI;
    let waterplane = 32.0 * (pi / 32.0).sin();

    // the library: one row per mesh with the engine's volume
    select(&mut h, Node::Meshes);
    let t = h.state().output().table.clone().expect("the library table");
    assert_eq!(t.rows.len(), 1);
    let volume = t.columns.iter().position(|c| c.header == "Volume [m³]").unwrap();
    assert!((value(&t.rows[0][volume]) - 0.5 * waterplane).abs() < 1e-9, "{:?}", t.rows[0][volume]);
    assert_eq!(t.rows[0][t.columns.iter().position(|c| c.header == "Records").unwrap()], Cell::Int(192));

    // a mesh: its panels (centroids, normals into the fluid, areas of the engine) and vertices
    select(&mut h, Node::Mesh(0));
    let output = h.state().output().clone();
    let tabs = output.tabs.as_ref().expect("tabs");
    assert_eq!(tabs.names, ["Panels", "Vertices"]);
    let panels = output.table.as_ref().unwrap();
    assert_eq!(panels.rows.len(), 192);
    let (zc, nz, area) = (column(&h, "Zc [m]"), column(&h, "nz"), column(&h, "Area [m²]"));
    let bottom = panels.rows.iter().find(|r| (value(&r[zc]) + 0.5).abs() < 1e-9).expect("a bottom panel");
    assert!((value(&bottom[nz]) + 1.0).abs() < 1e-9, "the bottom's normal points down");
    let total: f64 = panels.rows.iter().map(|r| value(&r[area])).sum();
    // the quarter's wetted area: 16 side panels of chord 2 sin(pi / 64) and height 0.5, the bottom
    // a quarter of the 64-gon
    let wetted = 16.0 * 2.0 * (pi / 64.0).sin() * 0.5 + 0.25 * waterplane;
    assert!((total - wetted).abs() < 1e-9, "the quarter's wetted area (side and bottom): {total}");
    let flags = column(&h, "Flags");
    assert!(panels.rows.iter().all(|r| r[flags] == Cell::Text(String::new())), "a closed, consistent mesh");
    let vertices = &tabs.more[0];
    assert_eq!(vertices.rows.len(), 4 * 192);
    let mesh = &h.state().project.meshes[0].mesh;
    assert_eq!(value(&vertices.rows[4 * 5 + 2][2]), mesh.panels[5][2][0], "panel 6, vertex 3, x as stored");
}

#[test]
fn table_rows_and_viewport_panels_highlight_each_other() {
    let mut h = harness();
    open(&mut h, "cylinder.hbm");
    select(&mut h, Node::Mesh(0));

    // a selected row highlights its panel in every symmetry copy
    h.state_mut().select_table_row(5);
    h.run_steps(3);
    assert_eq!(h.state().viewport.records, vec![5]);

    // on the Vertices tab, a vertex row highlights its panel
    h.get_by_label("Vertices").click();
    h.run_steps(3);
    assert_eq!(h.state().table_tab(), 1);
    h.state_mut().select_table_row(4 * 7 + 2);
    h.run_steps(3);
    assert_eq!(h.state().viewport.records, vec![7]);

    // a click on the mesh in the viewport selects its panel, and the table follows
    let centre = egui::pos2(930.0, 330.0);
    h.event(egui::Event::PointerMoved(centre));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: centre,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(3);
    let picked = h.state().viewport.records.clone();
    assert_eq!(picked.len(), 1, "one panel picked: {}", h.state().status);
    assert!(h.state().status.starts_with(&format!("Panel {}", picked[0] + 1)), "{}", h.state().status);
    // the picked panel stays highlighted with its row selected in the table
    h.run_steps(3);
    assert_eq!(h.state().viewport.records, picked);
    let shots = root().join("target/smoke");
    std::fs::create_dir_all(&shots).unwrap();
    h.render().expect("a frame").save(shots.join("01g-mesh-panel-picked.png")).unwrap();

    // a click on empty space clears it, in the viewport and the table
    let empty = egui::pos2(500.0, 100.0);
    h.event(egui::Event::PointerMoved(empty));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: empty,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(4);
    // still empty frames later: the table's selection went too (it would highlight it again)
    assert!(h.state().viewport.records.is_empty(), "{:?}", h.state().viewport.records);
    assert!(!h.state().status.starts_with("Panel"), "{}", h.state().status);
    h.render().expect("a frame").save(shots.join("01h-mesh-click-away.png")).unwrap();
    // picked again, then away by another node
    h.event(egui::Event::PointerMoved(centre));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: centre,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(3);
    assert_eq!(h.state().viewport.records.len(), 1);

    // another node clears the highlight
    select(&mut h, Node::Bodies);
    assert!(h.state().viewport.records.is_empty());
}

#[test]
fn a_patch_mesh_has_its_discretization_patches_and_corners() {
    let mut h = harness();
    open(&mut h, "truncated_cylinder_patches.hbm");
    select(&mut h, Node::Mesh(0));
    let output = h.state().output().clone();
    assert_eq!(output.tabs.as_ref().unwrap().names, ["Discretization", "Patches", "Corners"]);
    assert_eq!(output.table.as_ref().unwrap().rows.len(), 2, "the discretization per patch");
    let patches = &output.tabs.as_ref().unwrap().more[0];
    assert_eq!(patches.rows.len(), 2);
    assert_eq!(output.tabs.as_ref().unwrap().more[1].rows.len(), 8);
    h.state_mut().select_table_row(1);
    h.run_steps(3);
    assert_eq!(h.state().viewport.records, vec![1]);
}

#[test]
fn a_cad2bem_geometry_file_takes_its_reports_rows_and_checks() {
    // cad2bem's output, made here from the truncated cylinders' patches: a geometry file with patch
    // names and no discretization, and the report beside it
    let mut h = harness();
    open(&mut h, "three_truncated_cylinders_patches.hbm");
    let dir = std::env::temp_dir().join("hydrobem-gui-cad2bem");
    std::fs::create_dir_all(&dir).unwrap();
    let hbg = dir.join("columns.hbg");
    h.state().project.meshes[0].mesh.save_hbg(&hbg).expect("write");
    let mut file: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&hbg).unwrap()).unwrap();
    file.as_object_mut().unwrap().remove("discretization");
    let mut patches = Vec::new();
    for (k, p) in file["patches"].as_array_mut().unwrap().iter_mut().enumerate() {
        let p = p.as_object_mut().unwrap();
        p.remove("discretization");
        let name = format!("face_{}.{}", k / 4 + 3, k % 4 + 1);
        p.insert("name".into(), name.clone().into());
        let warn = k == 2;
        patches.push(serde_json::json!({
            "name": name, "face": k / 4 + 3, "face_type": if k % 2 == 0 { "Plane" } else { "Cylinder" },
            "state": if warn { "warn" } else { "ok" },
            "checks": {"fold": "ok", "corner_angle": if warn { "warn" } else { "ok" }},
            "discretization": [3, 1 + k % 3, 3, 3, 3, 4]
        }));
    }
    std::fs::write(&hbg, file.to_string()).unwrap();
    let report =
        serde_json::json!({"format": "hopatch-report", "version": 1, "summary": {"state": "warn"}, "patches": patches});
    std::fs::write(dir.join("columns.report.json"), report.to_string()).unwrap();

    h.state_mut().apply_import(&hbg);
    h.run_steps(6);
    assert!(h.state().status.contains("cad2bem's report (warn)"), "{}", h.state().status);
    let index = h.state().project.meshes.len() - 1;
    select(&mut h, Node::Mesh(index));
    let t = h.state().output().table.clone().expect("the discretization table");
    let (nv, state, checks) = (column(&h, "nv"), column(&h, "State"), column(&h, "Checks"));
    assert_eq!(value(&t.rows[1][nv]), 2.0, "the report's row");
    assert_eq!(t.rows[2][state].display(), "warn");
    assert_eq!(t.rows[2][checks].display(), "corner_angle warn");
    let shots = root().join("target/smoke");
    std::fs::create_dir_all(&shots).unwrap();
    h.render().expect("a frame").save(shots.join("01h-hopatch-report.png")).unwrap();
}

#[test]
fn a_patch_mesh_from_a_gdf_file_takes_a_panel_size() {
    // the truncated cylinder's patches read from tests/data/truncated_cylinder_patches.gdf: Panel
    // Size in the mesh's Details, 0.25 m typed and committed, makes every patch's discretization
    // from it (the rows saved with the project are coarser)
    let mut h = harness();
    open(&mut h, "truncated_cylinder_gdf.hbm");
    select(&mut h, Node::Mesh(0));
    let before = h.state().project.meshes[0].mesh.discretization();
    // the row's label is painted, not a widget: the field is the empty text input of the Details
    // (at the left), Panel Size being the only empty one there
    let field = h
        .query_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.rect().left() < 420.0 && n.accesskit_node().value().unwrap_or_default().is_empty())
        .expect("the empty Panel Size field")
        .rect();
    let p = field.center();
    // the Details say where the rows come from (the project) and the unknowns they make
    assert!(h.query_all_by_value("as saved with the project").next().is_some());
    h.event(egui::Event::PointerMoved(p));
    h.step();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.event(egui::Event::Text("0.25".into()));
    h.step();
    h.event(egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(4);
    let mesh = h.state().project.meshes[0].mesh.clone();
    assert_eq!(mesh.rows_from, gui_core::mesh::RowsFrom::PanelSize(0.25), "{}", h.state().status);
    let after = mesh.discretization();
    assert_eq!(after.len(), before.len());
    assert_ne!(after, before);
    assert!(after.iter().all(|r| r[2..] == [3, 3, 3, 4]), "{after:?}");
    // the body that uses it takes it
    assert_eq!(h.state().project.bodies[0].mesh.as_ref().unwrap().id, mesh.id);
    assert!(h.state().status.contains("panel size of 0.25 m"), "{}", h.state().status);
    let shots = root().join("target/smoke");
    std::fs::create_dir_all(&shots).unwrap();
    h.render().expect("a frame").save(shots.join("01i-patch-mesh-panel-size.png")).unwrap();
}
