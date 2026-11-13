//! While solving: the status bar counts the frequencies done, and the button left of Run lists
//! the progress by worker.

use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use gui_core::hydrobem_engine::RunState;
use hydrobem_gui::app::{App, Settings};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).expect("the repository").to_path_buf()
}

/// A file of this crate's test data (tests/data/README.md).
fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)
}

#[test]
fn each_frequencys_progress_is_listed_beside_run() {
    let mut h = Harness::builder()
        .with_size([1440.0, 900.0])
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_max_steps(64)
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_eframe(|cc| App::new(cc));
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("three_truncated_cylinders.hbm"));
    h.run_steps(4);
    // several frequencies at a time, enough of them to be caught running
    let p = &mut h.state_mut().project;
    p.options.frequency_threads = 3;
    p.options.inner_threads = 1;
    // the example solves one frequency: 24 over a range
    (p.waves.frequencies.start, p.waves.frequencies.end) = (0.3, 2.0);
    p.waves.frequencies.count = 24;
    // the progress of the frequencies, not of the field points (which slow each of them)
    p.field_points.enabled = false;
    p.waves.frequencies.rebuild();
    h.state_mut().run();
    let mut started = false;
    for _ in 0..4000 {
        h.step();
        let f = h.state().frequency_progress();
        // one done and others running
        if f.is_some_and(|f| f.done > 0 && f.frequencies.iter().any(|s| s.state == RunState::Running)) {
            started = true;
            break;
        }
        assert!(h.state().is_solving() || h.state().result.is_none(), "the solve ended before a frequency was seen");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(started, "no frequency reported");
    assert!(!h.state().show_frequencies);
    h.get_by_label("Frequency Progress").click();
    h.run_steps(3);
    assert!(h.state().show_frequencies);
    // by worker: a row per frequency thread, each its frequency with ω and stage, and a percentage
    h.get_by_label("Worker");
    let rows: Vec<String> = h.query_all_by_label_contains("rad/s").filter_map(|n| n.accesskit_node().value()).collect();
    assert!(!rows.is_empty() && rows.len() <= 3, "{rows:?}");
    assert!(rows.iter().all(|r| r.contains(" of 24, ω = ")), "{rows:?}");
    assert!(h.query_all_by_label_contains("%").next().is_some(), "a percentage on a bar");
    assert!(h.query_by_label_contains("Completed ").is_some());
    // the status bar: the frequencies done of all, no stage
    let total = h.state().frequency_progress().map(|f| f.total).unwrap_or_default();
    assert!(h.query_all_by_label_contains(&format!(" / {total}")).next().is_some(), "Frequency done / total");
    assert!(h.query_by_label_contains("assembly").is_none() && h.query_by_label_contains("LU ").is_none());
    let shots = root().join("target/smoke");
    std::fs::create_dir_all(&shots).unwrap();
    h.render().expect("a frame").save(shots.join("04b-frequency-progress.png")).unwrap();

    // Stop: the list goes with the solve
    h.state_mut().run();
    for _ in 0..4000 {
        h.step();
        if !h.state().is_solving() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    h.run_steps(3);
    assert!(!h.state().is_solving() && !h.state().show_frequencies);
    assert!(h.query_by_label("Frequency Progress").is_none());
}

#[test]
fn a_solve_the_engine_refuses_before_any_progress_ends_with_its_message() {
    // the engine's memory check refuses a solve before its first progress report (as, since
    // HydroBEM 0.3.0, an unbuildable free surface): the three columns as one body (2700 panels),
    // 1024 frequencies 1024 at a time, about 156 GiB; the solve ends at once with the message
    let (mut project, _) = gui_core::files::load_project(&data("three_columns.hbm")).unwrap();
    (project.waves.frequencies.start, project.waves.frequencies.end) = (0.3, 2.0);
    project.waves.frequencies.count = 1024;
    project.waves.frequencies.edit_in_table = false;
    project.waves.frequencies.rebuild();
    project.options.frequency_threads = 1024;
    let mut request = gui_core::solve::make_solve_request(&project).unwrap();
    let peak = gui_core::solve::estimate_memory(&mut request).unwrap().peak as u64;
    let free = hydrobem_gui::system::available_memory();
    if free == 0 || peak <= free {
        // a computer with that much free memory would solve it: not a refusal to test here
        eprintln!("skipped: {peak} bytes estimated, {free} free");
        return;
    }
    let started = std::time::Instant::now();
    let e = gui_core::solve::SolveHandle::start(request, || {}).wait().map(|_| ()).unwrap_err();
    assert!(e.message.contains("memory"), "{}", e.message);
    assert!(started.elapsed().as_secs() < 30, "refused before solving");
}
