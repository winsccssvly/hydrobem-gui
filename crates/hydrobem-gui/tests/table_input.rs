//! The output table driven by real pointer and keyboard events: a click, a drag and the row and
//! column headers select, Ctrl+C copies the selection.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use hydrobem_gui::app::{App, Settings};
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
        // frames of a real display, so that two clicks make a double click
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_eframe(|cc| App::new(cc))
}

fn button(h: &mut Harness<'_, App>, pos: egui::Pos2, pressed: bool) {
    h.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
}

fn move_to(h: &mut Harness<'_, App>, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
    h.step();
}

/// Ctrl+C as winit delivers it, and the text the app copied.
fn copy(h: &mut Harness<'_, App>) -> String {
    h.event(egui::Event::Copy);
    h.step();
    let copied = h
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        })
        .unwrap_or_default();
    h.step();
    copied
}

#[test]
fn the_table_selects_by_mouse_and_copies() {
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("cylinder.hbm"));
    h.state_mut().selected = Node::Mesh(0);
    h.run_steps(6);
    let table = h.state().output().table.clone().expect("the mesh geometry table");

    // the table of the default layout at 1440 x 900: a 38 px header, 24 px rows
    h.get_by_label("Geometry (mesh coordinates, symmetry expanded)");
    let (x0, top) = (420.0, 635.0);
    let row_y = |r: usize| top + 38.0 + 24.0 * r as f32 + 12.0;
    let first_value_x = x0 + 80.0;

    // a click on a cell selects it
    let p = egui::pos2(first_value_x, row_y(1));
    move_to(&mut h, p);
    button(&mut h, p, true);
    button(&mut h, p, false);
    let one = copy(&mut h);
    assert!(!one.is_empty(), "a clicked cell is copied");
    assert!(!one.contains('\t') && !one.contains('\n'), "one cell: {one:?}");
    assert_eq!(one, table.rows[1][1].display(), "the cell of row 2, column 2");

    // a drag selects a block of cells
    let q = egui::pos2(first_value_x, row_y(0));
    move_to(&mut h, q);
    button(&mut h, q, true);
    for r in 1..=3 {
        move_to(&mut h, egui::pos2(first_value_x + 10.0 * r as f32, row_y(r)));
    }
    button(&mut h, egui::pos2(first_value_x + 30.0, row_y(3)), false);
    let block = copy(&mut h);
    assert_eq!(block.split("\r\n").count(), 4, "four rows dragged: {block:?}");

    // the row number selects the whole row
    let n = egui::pos2(x0 + 12.0, row_y(2));
    move_to(&mut h, n);
    button(&mut h, n, true);
    button(&mut h, n, false);
    let row = copy(&mut h);
    assert_eq!(row.split('\t').count(), table.columns.len(), "the whole row: {row:?}");

    // a column header selects the whole column
    let c = egui::pos2(first_value_x, top + 19.0);
    move_to(&mut h, c);
    button(&mut h, c, true);
    button(&mut h, c, false);
    let column = copy(&mut h);
    assert_eq!(column.split("\r\n").count(), table.rows.len(), "the whole column");
}

#[test]
fn an_editable_table_takes_a_paste_and_a_typed_value() {
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("cylinder.hbm"));
    {
        let f = &mut h.state_mut().project.waves.frequencies;
        f.edit_in_table = true;
        f.count = 4;
        f.resize();
    }
    h.state_mut().selected = Node::Frequencies;
    h.run_steps(6);
    let table = h.state().output().table.clone().expect("the frequency table");
    let column = table.columns.iter().position(|c| c.editable).expect("an editable column");
    let x = 420.0 + table.columns[..column].len() as f32 * 60.0 + 40.0;
    let (x, top) = (x.max(500.0), 635.0);
    let row_y = |r: usize| top + 38.0 + 24.0 * r as f32 + 12.0;

    // a click on the first value, then Excel's two lines pasted down from it
    let p = egui::pos2(x, row_y(0));
    move_to(&mut h, p);
    button(&mut h, p, true);
    button(&mut h, p, false);
    h.event(egui::Event::Paste("0.25\r\n0.5\r\n".into()));
    h.run_steps(3);
    let inputs = h.state().project.waves.frequencies.inputs.clone();
    assert_eq!(&inputs[..2], &[0.25, 0.5], "pasted: {inputs:?} (column {column} at x {x})");

    // a double click edits the third value, Enter commits it
    h.run_steps(60);
    let q = egui::pos2(x, row_y(2));
    move_to(&mut h, q);
    for _ in 0..2 {
        button(&mut h, q, true);
        button(&mut h, q, false);
    }
    h.run_steps(2);
    h.event(egui::Event::Key {
        key: egui::Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    });
    h.event(egui::Event::Text("1.75".into()));
    h.step();
    h.event(egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
    assert_eq!(h.state().project.waves.frequencies.inputs[2], 1.75, "{:?}", h.state().project.waves.frequencies.inputs);
}

#[test]
fn the_right_click_menu_copies_the_selection() {
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("cylinder.hbm"));
    h.state_mut().selected = Node::Mesh(0);
    h.run_steps(6);
    let table = h.state().output().table.clone().expect("the mesh geometry table");
    let p = egui::pos2(500.0, 635.0 + 38.0 + 12.0);
    move_to(&mut h, p);
    button(&mut h, p, true);
    button(&mut h, p, false);
    h.event(egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Secondary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.step();
    h.event(egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Secondary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(3);
    h.get_by_label("Copy with Headers").click();
    h.step();
    let copied = h
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert_eq!(copied, format!("{}\r\n{}", table.columns[1].header, table.rows[0][1].display()));
}

#[test]
fn shift_and_a_left_drag_pans_the_viewport() {
    let mut h = harness();
    h.state_mut().settings = Settings::default();
    h.state_mut().open_project(&data("cylinder.hbm"));
    h.state_mut().selected = Node::Bodies;
    h.run_steps(6);
    let drag = |h: &mut Harness<'_, App>, modifiers: egui::Modifiers| {
        let (from, to) = (egui::pos2(900.0, 300.0), egui::pos2(960.0, 340.0));
        h.event(egui::Event::PointerMoved(from));
        h.event(egui::Event::PointerButton {
            pos: from,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        });
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.step();
        for t in 1..=4 {
            h.event(egui::Event::PointerMoved(from + (to - from) * (t as f32 / 4.0)));
            h.step();
        }
        h.event(egui::Event::PointerButton {
            pos: to,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        });
        h.step();
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.step();
    };
    let before = h.state().viewport.camera.clone();
    drag(&mut h, egui::Modifiers::SHIFT);
    let panned = h.state().viewport.camera.clone();
    assert_ne!(panned.target, before.target, "Shift + drag moves the target");
    assert_eq!((panned.yaw, panned.pitch), (before.yaw, before.pitch), "without a rotation");
    drag(&mut h, egui::Modifiers::NONE);
    let rotated = h.state().viewport.camera.clone();
    assert_ne!((rotated.yaw, rotated.pitch), (panned.yaw, panned.pitch), "a plain drag rotates");
    assert_eq!(rotated.target, panned.target);
}

#[test]
fn a_table_row_and_a_graph_point_pick_each_other() {
    let mut h = harness();
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
    h.state_mut().selected = Node::Result(hydrobem_gui::tree::ResultNode::AddedMass);
    h.run_steps(6);
    let p = egui::pos2(500.0, 635.0 + 38.0 + 24.0 * 2.0 + 12.0);
    move_to(&mut h, p);
    button(&mut h, p, true);
    button(&mut h, p, false);
    h.run_steps(4);
    let image = h.render().expect("a frame");
    let dir = root().join("target/smoke");
    std::fs::create_dir_all(&dir).unwrap();
    image.save(dir.join("29-graph-point-from-table.png")).unwrap();

    // the point at 1.2 rad/s clicked on the graph selects row 5
    let q = egui::pos2(1266.0, 771.0);
    move_to(&mut h, q);
    button(&mut h, q, true);
    button(&mut h, q, false);
    h.run_steps(4);
    h.render().expect("a frame").save(dir.join("29b-table-row-from-graph.png")).unwrap();

    // a click on the graph away from its points chooses nothing: the point and the row are released
    let away = egui::pos2(1100.0, 660.0);
    move_to(&mut h, away);
    button(&mut h, away, true);
    button(&mut h, away, false);
    h.run_steps(4);
    h.render().expect("a frame").save(dir.join("29c-graph-click-away.png")).unwrap();
}
