//! The wave amplitude of the result views: the values the waves drive (wave loads, motions) are
//! shown for it, the others (added mass) are not; it is the app's for the session, not saved.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use hydrobem_gui::app::{App, Settings};
use hydrobem_gui::output::Cell;
use hydrobem_gui::tree::{Node, ResultNode};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).expect("the repository").to_path_buf()
}

/// A file of this crate's test data (tests/data/README.md).
fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)
}

/// The numbers of a column of the node's table.
fn column(h: &Harness<'_, App>, k: usize) -> Vec<f64> {
    let table = h.state().output().table.clone().expect("a table");
    table.rows.iter().filter_map(|r| if let Cell::Num(v) = r[k] { Some(v) } else { None }).collect()
}

#[test]
fn results_are_shown_for_the_wave_amplitude() {
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
    h.run_steps(4);
    h.state_mut().run();
    for _ in 0..4000 {
        h.step();
        if !h.state().is_solving() && h.state().result.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    assert!(h.state().result.is_some(), "solved: {}", h.state().status);
    assert_eq!(h.state().wave_amplitude, 1.0, "per metre of wave amplitude at first");

    let at = |h: &mut Harness<'_, App>, node: ResultNode| {
        h.state_mut().selected = Node::Result(node);
        h.run_steps(6);
    };
    // per metre: the magnitudes (column 2) of the excitation and the motions, and the added mass
    at(&mut h, ResultNode::Excitation);
    assert_eq!(h.query_all_by_label("Waves").count(), 1, "the Details' section of the wave amplitude");
    let force = column(&h, 2);
    at(&mut h, ResultNode::Motion);
    let motion = column(&h, 2);
    assert!(h.state().output().table.as_ref().unwrap().columns[2].header.contains("[m/m]"));
    at(&mut h, ResultNode::AddedMass);
    assert_eq!(h.query_all_by_label("Waves").count(), 0, "the added mass does not depend on the waves");
    let added = column(&h, 2);

    // for waves of 2 m (an edit of the field)
    h.state_mut().wave_amplitude = 2.0;
    at(&mut h, ResultNode::Excitation);
    let doubled: Vec<f64> = force.iter().map(|v| 2.0 * v).collect();
    assert_eq!(column(&h, 2), doubled);
    assert!(h.state().output().title.ends_with("A = 2 m"), "{}", h.state().output().title);
    at(&mut h, ResultNode::Motion);
    assert_eq!(column(&h, 2), motion.iter().map(|v| 2.0 * v).collect::<Vec<_>>());
    let header = &h.state().output().table.as_ref().unwrap().columns[2].header;
    assert!(header.contains("[m]") && !header.contains("m/m"), "{header}");
    let shots = root().join("target/smoke");
    std::fs::create_dir_all(&shots).unwrap();
    h.render().expect("a frame").save(shots.join("30-motions-wave-amplitude.png")).unwrap();
    at(&mut h, ResultNode::AddedMass);
    assert_eq!(column(&h, 2), added);

    // the animation follows it: Motions animated, paused, at the real size (Display Scale 1, the
    // default): the highest point of the scene (the incident wave's crest, the moved hull)
    // rises with the wave amplitude
    let top = |h: &mut Harness<'_, App>, a: f64| -> f32 {
        h.state_mut().wave_amplitude = a;
        h.state_mut().selected = Node::Result(ResultNode::Motion);
        h.state_mut().selection.animate = true;
        h.state_mut().selection.paused = true;
        assert_eq!(h.state().selection.display_scale, 1.0);
        h.run_steps(6);
        let s = &h.state().viewport.scene;
        s.transparent.iter().chain(&s.opaque).map(|v| v.position[2]).fold(f32::MIN, f32::max)
    };
    let (one, two) = (top(&mut h, 1.0), top(&mut h, 2.0));
    h.render().expect("a frame").save(root().join("target/smoke/30b-motions-animated-real-size.png")).unwrap();
    assert!(two > one + 0.5, "the crest at 1 m {one}, at 2 m {two}");
}
