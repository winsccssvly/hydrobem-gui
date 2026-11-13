//! The File menu: Save, Save As and the Export submenu (WAMIT (s), WAMIT (rad/s), NumPy), enabled
//! once solved.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use hydrobem_gui::app::{App, Settings};

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

/// Opens File → Export; whether it is enabled (then its three items are).
fn export_items(h: &mut Harness<'_, App>) -> bool {
    h.get_by_label("File").click();
    h.run_steps(3);
    // the File menu's items (a label holds its shortcut), none of the earlier names
    let labels: Vec<String> = h
        .query_all_by(|n| n.label().is_some_and(|l| l.contains("Save") || l.contains("Export")))
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert_eq!(labels, ["Save Ctrl+S", "Save As… Ctrl+Shift+S", "Export"]);
    if h.get_by_label("Export").accesskit_node().is_disabled() {
        return false;
    }
    h.get_by_label("Export").hover();
    h.run_steps(3);
    for name in ["WAMIT (s)…", "WAMIT (rad/s)…", "NumPy (.npz)…"] {
        assert!(!h.get_by_label(name).accesskit_node().is_disabled(), "{name}");
    }
    // no table export here: the table's own menu has Export CSV
    assert!(h.query_by_label("Excel…").is_none());
    true
}

#[test]
fn export_offers_wamit_and_numpy_once_solved() {
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("cylinder.hbm"));
    h.run_steps(4);
    // no results yet: Export is disabled
    assert!(!export_items(&mut h));
    h.key_press(egui::Key::Escape);
    h.run_steps(2);

    // solved: the WAMIT files and the NumPy archive
    {
        let app = h.state_mut();
        app.project.waves.frequencies.inputs.truncate(2);
        app.project.waves.frequencies.count = 2;
    }
    h.state_mut().run();
    for _ in 0..600 {
        h.run_steps(2);
        if h.state().result.is_some() && !h.state().is_solving() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(h.state().result.is_some(), "solved");
    h.state_mut().selected = hydrobem_gui::tree::Node::Result(hydrobem_gui::tree::ResultNode::AddedMass);
    h.run_steps(4);
    assert!(h.state().output().table.is_some());
    assert!(export_items(&mut h));
    let shots = root().join("target/smoke");
    std::fs::create_dir_all(&shots).unwrap();
    h.render().expect("a frame").save(shots.join("06c-file-export-menu.png")).unwrap();
}

fn solve(h: &mut Harness<'_, App>) {
    h.state_mut().run();
    for _ in 0..600 {
        h.run_steps(2);
        if h.state().result.is_some() && !h.state().is_solving() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("not solved: {}", h.state().status);
}

#[test]
fn save_writes_the_results_once_solved_else_the_model() {
    let dir = root().join("target/file-menu");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (hbm, hbr) = (dir.join("barge.hbm"), dir.join("barge.hbr"));
    std::fs::copy(data("cylinder.hbm"), &hbm).unwrap();

    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&hbm);
    {
        let app = h.state_mut();
        app.project.waves.frequencies.inputs.truncate(2);
        app.project.waves.frequencies.count = 2;
    }
    h.run_steps(2);

    // not run: the model
    assert!(!h.state().saves_results());
    std::fs::remove_file(&hbm).unwrap();
    assert!(h.state_mut().save(false), "{}", h.state().status);
    assert!(hbm.exists() && !hbr.exists());

    // run: the results file beside it, which is now the project's file, saved
    solve(&mut h);
    assert!(h.state().saves_results());
    assert!(h.state_mut().save(false), "{}", h.state().status);
    assert!(hbr.exists(), "{}", h.state().status);
    assert_eq!(h.state().file.as_deref(), Some(hbr.as_path()));
    assert!(!h.state().is_modified());
    let reopened = gui_core::files::open_results(&hbr).expect("the saved results");
    assert_eq!(reopened.result.omegas().len(), 2);

    // changed after the run: the results are out of date, marked so; undone, current again
    assert!(!h.state().results_outdated());
    assert!(h.query_by_label_contains("Results out of date").is_none());
    let density = h.state().project.environment.density;
    h.state_mut().project.environment.density = density + 25.0;
    h.state_mut().selected = hydrobem_gui::tree::Node::Result(hydrobem_gui::tree::ResultNode::AddedMass);
    h.run_steps(4);
    assert!(h.state().results_outdated());
    // in the status bar and above the results' details
    assert_eq!(h.query_all_by_label_contains("Results out of date").count(), 2);
    let shots = root().join("target/smoke");
    std::fs::create_dir_all(&shots).unwrap();
    h.render().expect("a frame").save(shots.join("06d-results-out-of-date.png")).unwrap();
    h.state_mut().project.environment.density = density;
    h.run_steps(2);
    assert!(!h.state().results_outdated() && h.state().saves_results());
    assert!(h.query_by_label_contains("Results out of date").is_none());

    // changed after the run: the results are not of the project, the model is saved
    h.state_mut().project.environment.density = density + 25.0;
    h.run_steps(2);
    assert!(h.state().is_modified() && !h.state().saves_results());
    std::fs::remove_file(&hbm).unwrap();
    let hbr_written = std::fs::metadata(&hbr).unwrap().modified().unwrap();
    assert!(h.state_mut().save(false), "{}", h.state().status);
    assert!(hbm.exists());
    assert_eq!(std::fs::metadata(&hbr).unwrap().modified().unwrap(), hbr_written, "the results file kept");
    assert_eq!(h.state().file.as_deref(), Some(hbm.as_path()));

    // a results file opened: Save writes it again
    h.state_mut().open_results_file(&hbr);
    h.run_steps(2);
    assert!(h.state().saves_results());
    assert_eq!(h.state().file.as_deref(), Some(hbr.as_path()));
}
