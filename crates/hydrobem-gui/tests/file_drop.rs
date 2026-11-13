//! Files dropped onto the window: a model (.hbm) opens as a project, results (.hbr) as results,
//! anything else only says so in the status bar.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use hydrobem_gui::app::{App, FileAction, Settings};

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
        .wgpu()
        .build_eframe(|cc| App::new(cc))
}

/// A file as eframe hands it over after a drop.
#[derive(Debug)]
struct Dropped(PathBuf);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn drop_file(h: &mut Harness<'_, App>, path: &Path) {
    h.input_mut().dropped_files.push(Arc::new(Dropped(path.to_path_buf())));
    h.run_steps(4);
}

#[test]
fn dropped_models_and_results_open() {
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().test_answer_unsaved = Some(false);

    // a model opens as the project
    let model = data("cylinder.hbm");
    drop_file(&mut h, &model);
    assert_eq!(h.state().file.as_deref(), Some(model.as_path()), "{}", h.state().status);

    // results saved by a solve open with their project
    h.state_mut().run();
    for _ in 0..4000 {
        h.step();
        if !h.state().is_solving() && h.state().result.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    assert!(h.state().result.is_some(), "solved: {}", h.state().status);
    let dir = root().join("target/file-drop");
    std::fs::create_dir_all(&dir).unwrap();
    let results = dir.join("dropped.hbr");
    assert!(h.state_mut().save_results_to(&results));
    h.state_mut().request(FileAction::New);
    h.run_steps(2);
    assert!(h.state().result.is_none());
    drop_file(&mut h, &results);
    assert!(h.state().result.is_some(), "the dropped results: {}", h.state().status);

    // another file changes nothing
    let file = h.state().file.clone();
    drop_file(&mut h, &root().join("CHANGELOG.md"));
    assert_eq!(h.state().file, file);
    assert!(h.state().status.contains("not a HydroBEM model"), "{}", h.state().status);
}

#[test]
fn gdf_files_dropped_onto_meshes_are_imported() {
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("cylinder.hbm"));
    h.run_steps(4);
    assert_eq!(h.state().project.meshes.len(), 1);
    let gdf = [data("cylinder.gdf"), data("two_truncated_cylinders.gdf")];

    // dropped elsewhere (the viewport) a GDF is not imported
    h.event(egui::Event::PointerMoved(egui::pos2(900.0, 300.0)));
    h.step();
    drop_file(&mut h, &gdf[0]);
    assert_eq!(h.state().project.meshes.len(), 1);

    // onto Meshes in the tree: both files, the last selected
    let row = h.get_by_role_and_label(egui::Role::TreeItem, "Meshes").rect();
    h.event(egui::Event::PointerMoved(row.center()));
    h.step();
    for p in &gdf {
        h.input_mut().dropped_files.push(Arc::new(Dropped(p.clone())));
    }
    h.run_steps(4);
    let names: Vec<&str> = h.state().project.meshes.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names.len(), 3, "{names:?} {}", h.state().status);
    assert!(matches!(h.state().selected, hydrobem_gui::tree::Node::Mesh(2)));
    assert!(h.state().status.contains("Imported 2 meshes"), "{}", h.state().status);
}

#[test]
fn a_mesh_exported_as_hbg_drops_back_onto_meshes() {
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("cylinder.hbm"));
    h.run_steps(4);
    let dir = std::env::temp_dir().join("hydrobem-gui-file-drop");
    std::fs::create_dir_all(&dir).unwrap();
    let hbg = dir.join("barge_export.hbg");
    let _ = std::fs::remove_file(&hbg);
    h.state_mut().export_mesh(0, Some(hbg.clone()));
    assert!(hbg.is_file(), "{}", h.state().status);
    assert!(h.state().status.starts_with("Exported"), "{}", h.state().status);

    let row = h.get_by_role_and_label(egui::Role::TreeItem, "Meshes").rect();
    h.event(egui::Event::PointerMoved(row.center()));
    h.step();
    h.input_mut().dropped_files.push(Arc::new(Dropped(hbg.clone())));
    h.run_steps(4);
    let meshes = &h.state().project.meshes;
    assert_eq!(meshes.len(), 2, "{}", h.state().status);
    assert_eq!(meshes[1].mesh.panels, meshes[0].mesh.panels);
}

#[test]
fn a_cad_file_dropped_is_not_read() {
    // CAD files are converted by cad2bem, a program of its own: the GUI reads its geometry file,
    // never the CAD file; dropped anywhere, the status bar says so
    let dir = std::env::temp_dir().join(format!("hydrobem-gui-drop-cad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let step = dir.join("hull.STEP");
    std::fs::write(&step, "ISO-10303-21;\n").unwrap();
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("cylinder.hbm"));
    h.run_steps(4);
    for at in [None, Some("Meshes")] {
        let p = match at {
            Some(label) => h.get_by_role_and_label(egui::Role::TreeItem, label).rect().center(),
            None => egui::pos2(900.0, 300.0),
        };
        h.event(egui::Event::PointerMoved(p));
        h.step();
        drop_file(&mut h, &step);
        h.run_steps(2);
        assert!(h.state().modal_text().is_none());
        assert_eq!(h.state().project.meshes.len(), 1, "nothing read");
        assert!(h.state().status.contains("cad2bem"), "{}", h.state().status);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn files_dropped_onto_meshes_behind_a_dialog_wait() {
    // a question open (the discretization of a GDF file without SPL file): a mesh file dropped
    // onto Meshes behind it is not taken
    let dir = std::env::temp_dir().join(format!("hydrobem-gui-behind-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let plate = dir.join("plate.gdf");
    std::fs::write(&plate, "plate\n1.0 9.80665\n0 0\n1 0\n0 0 -2\n0 4 -2\n10 4 -2\n10 0 -2\n").unwrap();
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("cylinder.hbm"));
    h.run_steps(4);
    let row = h.get_by_role_and_label(egui::Role::TreeItem, "Meshes").rect();
    h.event(egui::Event::PointerMoved(row.center()));
    h.step();
    drop_file(&mut h, &plate);
    h.run_steps(2);
    assert_eq!(h.state().modal_text().as_deref(), Some("discretization"));
    drop_file(&mut h, &data("two_truncated_cylinders.gdf"));
    h.run_steps(2);
    assert_eq!(h.state().project.meshes.len(), 1, "no mesh imported behind the dialog");
    assert_eq!(h.state().modal_text().as_deref(), Some("discretization"), "the question kept");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_higher_order_gdf_without_spl_asks_for_its_discretization() {
    // two flat-patch GDF files (a 10 m x 4 m plate each, no SPL file) dropped onto Meshes: nothing
    // is read before the discretization is chosen; Cancel reads nothing, Import reads both with it
    let dir = std::env::temp_dir().join(format!("hydrobem-gui-ask-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let plate = "plate\n1.0 9.80665\n0 0\n1 0\n0 0 -2\n0 4 -2\n10 4 -2\n10 0 -2\n";
    let files = [dir.join("a.gdf"), dir.join("b.gdf")];
    for f in &files {
        std::fs::write(f, plate).unwrap();
    }
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("cylinder.hbm"));
    h.run_steps(4);
    let drop_both = |h: &mut Harness<'_, App>| {
        let row = h.get_by_role_and_label(egui::Role::TreeItem, "Meshes").rect();
        h.event(egui::Event::PointerMoved(row.center()));
        h.step();
        for p in &files {
            h.input_mut().dropped_files.push(Arc::new(Dropped(p.clone())));
        }
        h.run_steps(4);
    };
    drop_both(&mut h);
    assert_eq!(h.state().modal_text().as_deref(), Some("discretization"));
    assert_eq!(h.state().project.meshes.len(), 1, "nothing read yet");
    assert!(!h.state().status.contains("Imported"), "{}", h.state().status);
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.state().modal_text().is_none());
    assert_eq!(h.state().project.meshes.len(), 1, "cancelled");

    drop_both(&mut h);
    // a panel size of 1 m, the orders as WAMIT's defaults
    let choice = h.state_mut().gdf_discretization_mut().expect("asked");
    assert_eq!((choice.order, choice.outer, choice.inner), (3, 3, 4));
    choice.by_panel_size = true;
    choice.panel_size = 1.0;
    h.run_steps(2);
    let shots = root().join("target/smoke");
    std::fs::create_dir_all(&shots).unwrap();
    h.render().expect("a frame").save(shots.join("01j-gdf-discretization.png")).unwrap();
    h.get_by_label("Import").click();
    h.run_steps(4);
    assert!(h.state().modal_text().is_none());
    let meshes = &h.state().project.meshes;
    assert_eq!(meshes.len(), 3);
    for m in &meshes[1..] {
        assert_eq!(m.mesh.rows_from, gui_core::mesh::RowsFrom::PanelSize(1.0));
        let row = m.mesh.discretization()[0];
        let mut spans = [row[0], row[1]];
        spans.sort();
        assert_eq!(spans, [5, 11], "floor(L / 1 m) + 1 along 4 m and 10 m");
    }

    // a GDF file with its SPL file beside it is read without the question
    drop_file(&mut h, &data("truncated_cylinder_patches.gdf"));
    h.run_steps(4);
    assert!(h.state().modal_text().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
