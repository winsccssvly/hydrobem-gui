//! Model files (.hbm) and results files (.hbr): round trips, unsaved-change fingerprints, every
//! test model opened and solved, files the GUI refuses.

use std::path::{Path, PathBuf};

use gui_core::files::{
    export_npz, export_wamit, load_project, open_results, project_fingerprint, save_project, save_results,
};
use gui_core::hydrobem_engine::{Model, QuantityId};
use gui_core::project::{InteriorMethod, MassCalculation, Project, SurfaceSource};
use gui_core::results::FieldKind;
use gui_core::solve::{SolveHandle, make_solve_request};

/// The test files of this crate (tests/data/README.md).
fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hydrobem-gui-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// The models of tests/data, sorted.
fn name_is(file: &Path, stem: &str) -> bool {
    file.file_stem().is_some_and(|s| s == stem)
}

fn test_models() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(data())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "hbm"))
        .collect();
    files.sort();
    files
}

#[test]
fn every_test_model_opens_without_warnings_and_solves() {
    let names = test_models();
    assert!(names.len() >= 13, "{names:?}");
    for file in names {
        let (mut project, warnings) = load_project(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        assert!(warnings.is_empty(), "{}: {warnings:?}", file.display());
        assert!(!project.bodies.is_empty());
        // every body a hull that displaces water, but a body that is a free-surface plate alone (the
        // dock: zero draft, no wetted mesh)
        let plate = |b: &gui_core::project::Body| b.mesh.is_none() && !b.free_surface_plates.is_empty();
        let dry = project.bodies.iter().find(|b| !plate(b) && (b.mesh.is_none() || b.displaced_volume <= 0.0));
        assert!(dry.is_none(), "{}: {:?}", file.display(), dry.map(|b| (&b.name, b.displaced_volume)));
        // the large higher-order models take minutes: opened only
        if name_is(&file, "three_truncated_cylinders_patches") {
            continue;
        }
        // one frequency is enough to show that the engine accepts the model and the GUI reads the
        // layout it promises
        project.waves.frequencies.inputs.truncate(1);
        project.waves.frequencies.count = 1;
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let result = SolveHandle::start(make_solve_request(&project).unwrap(), || {})
            .wait()
            .unwrap_or_else(|e| panic!("{name}: {}", e.message));
        assert!(result.has(QuantityId::AddedMass), "{name}");
        let coordinates = match name.as_str() {
            "two_truncated_cylinders.hbm" => 12,
            "three_truncated_cylinders.hbm" => 18,
            "floating_owc_chamber.hbm" => 7, // six rigid ones and the chamber's pressure mode
            _ => 6,
        };
        assert_eq!(result.coordinates, coordinates, "{name}");
        assert_eq!(result.has(QuantityId::Motion), project.first_order.motion, "{name}");
        assert_eq!(result.has(QuantityId::Lids), !project.lids.is_empty(), "{name}");
        assert_eq!(!result.field_points.is_empty(), project.field_points.enabled, "{name}");
    }
}

#[test]
fn no_test_model_holds_a_machine_path() {
    // the models are in the public repository: their meshes name their files alone
    let models = test_models();
    assert!(!models.is_empty());
    for path in models {
        let bytes = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        // a drive (C:\), a verbatim prefix (\\?\) or a user folder
        let drive = text.as_bytes().windows(3).any(|w| w[0].is_ascii_alphabetic() && w[1] == b':' && w[2] == b'\\');
        assert!(!drive && !text.contains(r"\\?\") && !text.contains("Users"), "{} holds a path", path.display());
    }
}

#[test]
fn the_test_models_keep_what_they_were_written_with() {
    // examples/cylinder: the displaced mass (rho V, the 64-gon prism R = 1 m, T = 0.5 m), its radii
    // of gyration as inertia about R, the interior surface detected; the tests' grid and motions
    let (cylinder, _) = load_project(&data().join("cylinder.hbm")).unwrap();
    let b = &cylinder.bodies[0];
    assert_eq!(b.name, "cylinder");
    assert_eq!(b.mass_calculation, MassCalculation::FromMesh);
    let volume = 16.0 * (std::f64::consts::PI / 32.0).sin();
    assert!((b.mass - 1000.0 * volume).abs() < 1e-6, "the displaced mass, rho V");
    // about G: m (0.6^2, 0.6^2, 0.7^2); about R, 0.2 m above G, m 0.2^2 more in roll and pitch
    assert!((b.inertia[0] - b.mass * (0.36 + 0.04)).abs() < 1e-6 && (b.inertia[5] - b.mass * 0.49).abs() < 1e-6);
    assert_eq!(b.interior.method, InteriorMethod::Detect);
    assert!(cylinder.field_points.enabled && cylinder.field_points.x_count == 25);
    assert!(cylinder.first_order.motion);
    // the mesh was built from its vertices, not read: no file, no machine path in the model
    assert_eq!(b.mesh.as_ref().unwrap().source_file, "");

    // examples/floating_owc_chamber: the chamber a pressure surface on records of the body's mesh
    let (owc, _) = load_project(&data().join("floating_owc_chamber.hbm")).unwrap();
    let surfaces = &owc.bodies[0].pressure_surfaces;
    assert!(surfaces.len() == 1 && surfaces[0].mesh.is_none() && surfaces[0].pressure_modes.len() == 1);
    assert!(surfaces[0].chamber_coupling);
    let (lid, _) = load_project(&data().join("floating_owc_chamber_lid.hbm")).unwrap();
    assert_eq!(lid.lids.len(), 1);
    assert_eq!(lid.lids[0].damping_factor, 0.1);
    // examples/cylinder_with_heave_plate/run_thin_plate.py: the plate a dipole sheet
    let (plate, _) = load_project(&data().join("cylinder_with_heave_plate.hbm")).unwrap();
    assert_eq!(plate.bodies[0].dipoles.len(), 1);
    // examples/dock and examples/fish_tank_cylinders: a free-surface plate, a tank
    let (dock, _) = load_project(&data().join("dock.hbm")).unwrap();
    assert_eq!(dock.bodies[0].free_surface_plates.len(), 1);
    let (tank, _) = load_project(&data().join("fish_tank_cylinders.hbm")).unwrap();
    assert_eq!(tank.bodies[0].tanks.len(), 1);
    // examples/trimmed_moonpool_cover: a plate the engine generates in the trimmed hull's moonpool
    let (moonpool, _) = load_project(&data().join("trimmed_moonpool_cover.hbm")).unwrap();
    let plate = &moonpool.bodies[0].free_surface_plates[0];
    assert!(moonpool.bodies[0].trim.is_some());
    assert_eq!((plate.source, plate.inside, plate.body.as_deref()), (SurfaceSource::Generated, [0.0, 0.0], None));
    // examples/fish_tank_cylinders/run_covered_tank.py: the tank half covered, a plate of the tank
    // with a mesh of its own
    let (covered, _) = load_project(&data().join("fish_tank_covered.hbm")).unwrap();
    let plates = &covered.bodies[0].tanks[0].free_surface_plates;
    assert!(plates.len() == 1 && plates[0].source == SurfaceSource::ExternalMesh && plates[0].mesh.is_some());
}

#[test]
fn save_open_save_is_stable() {
    let (project, _) = load_project(&data().join("two_truncated_cylinders.hbm")).unwrap();
    let first = scratch("stable-1.hbm");
    save_project(&project, &first).unwrap();
    let (reopened, warnings) = load_project(&first).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(project_fingerprint(&project), project_fingerprint(&reopened));
    // saving over an existing file replaces it
    save_project(&reopened, &first).unwrap();
    assert_eq!(project_fingerprint(&load_project(&first).unwrap().0), project_fingerprint(&project));
}

#[test]
fn a_suppressed_body_and_lid_are_disabled_in_the_model() {
    let (mut project, _) = load_project(&data().join("floating_owc_chamber_lid.hbm")).unwrap();
    project.add_body();
    project.bodies[1].mesh = project.bodies[0].mesh.clone();
    project.bodies[1].position = [6.0, 0.0, 0.0];
    project.bodies[1].suppressed = true;
    project.bodies[1].interior.method = InteriorMethod::Waterline;
    project.lids[0].suppressed = true;
    project.update_mesh_properties();
    let file = scratch("suppressed.hbm");
    save_project(&project, &file).unwrap();
    // the model holds both, disabled
    let m = Model::load(&file).unwrap();
    let bodies = m.children(m.root(), "bodies").unwrap().to_vec();
    let lids = m.children(m.root(), "lids").unwrap().to_vec();
    assert_eq!((bodies.len(), lids.len()), (2, 1));
    for id in [bodies[1], lids[0]] {
        assert_eq!(m.get(id, "enabled").unwrap().and_then(|v| v.as_bool()), Some(false));
    }
    let (reopened, _) = load_project(&file).unwrap();
    assert_eq!(reopened.bodies.len(), 2);
    assert!(reopened.bodies[1].suppressed && reopened.bodies[1].position == [6.0, 0.0, 0.0]);
    assert_eq!(reopened.bodies[1].interior.method, InteriorMethod::Waterline);
    assert!(reopened.lids[0].suppressed && reopened.lids[0].range == project.lids[0].range);
    assert_eq!(project_fingerprint(&project), project_fingerprint(&reopened));
}

#[test]
fn the_fingerprint_follows_every_input() {
    let (project, _) = load_project(&data().join("cylinder.hbm")).unwrap();
    let base = project_fingerprint(&project);
    type Change = Box<dyn Fn(&mut Project)>;
    let changes: Vec<Change> = vec![
        Box::new(|p| p.bodies[0].position[0] = 1.0),
        Box::new(|p| p.bodies[0].visible = false),
        Box::new(|p| p.environment.density = 1025.0),
        Box::new(|p| p.options.inner_threads = 2),
        Box::new(|p| p.field_points.z = -1.0),
        Box::new(|p| p.waves.headings.count = 2),
        Box::new(|p| p.waves.frequencies.inputs[0] = 0.41),
        Box::new(|p| p.bodies[0].additional_damping = Some(Default::default())),
        Box::new(|p| p.bodies[0].mesh = None),
    ];
    for (i, change) in changes.iter().enumerate() {
        let mut p = project.clone();
        change(&mut p);
        assert_ne!(project_fingerprint(&p), base, "change {i}");
    }
}

#[test]
fn a_model_without_a_mass_opens_as_from_mesh_with_a_warning() {
    let (project, _) = load_project(&data().join("cylinder.hbm")).unwrap();
    let file = scratch("no-mass.hbm");
    save_project(&project, &file).unwrap();
    let mut m = Model::load(&file).unwrap();
    let b = m.object("bodies/cylinder").unwrap();
    m.reset_item(b, "mass").unwrap();
    // and no GUI state, as a model written by Python
    let tool = m.object("tool/hydrobem_gui").unwrap();
    m.remove(tool).unwrap();
    let python = scratch("python.hbm");
    m.save(&python, true).unwrap();
    let (opened, warnings) = load_project(&python).unwrap();
    assert_eq!(opened.bodies[0].mass_calculation, MassCalculation::FromMesh);
    assert!(warnings.iter().any(|w| w.contains("no mass")), "{warnings:?}");
    // the grid is rebuilt from the points
    assert_eq!(opened.field_points, project.field_points);
}

#[test]
fn inputs_the_gui_cannot_show_are_refused_with_their_path() {
    let (project, _) = load_project(&data().join("cylinder.hbm")).unwrap();
    let file = scratch("fixed-damper-ranges.hbm");
    save_project(&project, &file).unwrap();
    let mut m = Model::load(&file).unwrap();
    // a fixed damper on some records of a mesh
    let root = m.root();
    let d = m.add(root, "fixed_dampers", None, "", None).unwrap();
    m.set_surface(d, "mesh", "ranges", "cylinder", &[[1, 2]]).unwrap();
    m.save(&file, true).unwrap();
    let e = load_project(&file).expect_err("refused");
    assert!(e.contains("fixed_dampers") && e.contains("not supported"), "{e}");
    assert!(load_project(&data().join("cylinder.gdf")).is_err());
}

#[test]
fn results_round_trip_through_a_results_file() {
    let (mut project, _) = load_project(&data().join("cylinder.hbm")).unwrap();
    project.waves.frequencies.inputs.truncate(2);
    project.waves.frequencies.count = 2;
    let result = SolveHandle::start(make_solve_request(&project).unwrap(), || {}).wait().unwrap();
    let file = scratch("run.hbr");
    save_results(&result, &file).unwrap();
    let opened = open_results(&file).unwrap();
    assert!(opened.warnings.is_empty(), "{:?}", opened.warnings);
    assert_eq!(opened.project.bodies[0].name, "cylinder");
    assert_eq!(opened.result.omegas(), result.omegas());
    assert_eq!(opened.result.field_points, result.field_points);
    assert_eq!(opened.result.bodies, result.bodies);
    let (a, b) = (
        result.field_amplitude(FieldKind::Total, 1, 0, None),
        opened.result.field_amplitude(FieldKind::Total, 1, 0, None),
    );
    assert_eq!(a, b);
    assert_eq!(
        result.coefficient_series(QuantityId::AddedMass, 2, 2),
        opened.result.coefficient_series(QuantityId::AddedMass, 2, 2)
    );
    // a results file opens as a project too (its inputs), and a model file is not results
    assert_eq!(load_project(&file).unwrap().0.bodies.len(), 1);
    assert!(open_results(&data().join("cylinder.hbm")).is_err());
}

#[test]
fn results_export_as_wamit_files_over_the_frequency() {
    let (mut project, _) = load_project(&data().join("cylinder.hbm")).unwrap();
    project.waves.frequencies.inputs.truncate(2);
    project.waves.frequencies.count = 2;
    let result = SolveHandle::start(make_solve_request(&project).unwrap(), || {}).wait().unwrap();
    let out = scratch("wamit").join("cylinder.out");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    export_wamit(&result, &out, true).unwrap();
    // the summary and the numeric files of the stem; again over them
    assert!(out.is_file());
    let added = std::fs::read_to_string(out.with_extension("1")).unwrap();
    export_wamit(&result, &out, true).unwrap();
    // the first column is the frequency in rad/s (the first two of the model), not the period
    let omegas = result.omegas();
    let first: Vec<f64> =
        added.lines().filter_map(|l| l.split_whitespace().next()?.parse::<f64>().ok()).filter(|w| *w > 0.0).collect();
    assert!(!first.is_empty(), "{added}");
    for w in &first {
        assert!(omegas.iter().any(|o| (o - w).abs() < 1e-4 * o.max(1.0)), "{w} of {omegas:?}");
    }
}

#[test]
fn results_export_as_wamit_files_over_the_period() {
    let (mut project, _) = load_project(&data().join("cylinder.hbm")).unwrap();
    project.waves.frequencies.inputs.truncate(2);
    project.waves.frequencies.count = 2;
    let result = SolveHandle::start(make_solve_request(&project).unwrap(), || {}).wait().unwrap();
    let out = scratch("wamit-period").join("cylinder.out");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    export_wamit(&result, &out, false).unwrap();
    let added = std::fs::read_to_string(out.with_extension("1")).unwrap();
    // the first column is the period 2 pi / omega
    let periods: Vec<f64> = result.omegas().iter().map(|w| 2.0 * std::f64::consts::PI / w).collect();
    let first: Vec<f64> =
        added.lines().filter_map(|l| l.split_whitespace().next()?.parse::<f64>().ok()).filter(|t| *t > 0.0).collect();
    assert!(!first.is_empty(), "{added}");
    for t in &first {
        assert!(periods.iter().any(|p| (p - t).abs() < 1e-4 * p), "{t} of {periods:?}");
    }
}

/// The members of a ZIP archive of stored members: name and bytes, from the local headers.
fn zip_members(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]) as usize;
    let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
    let mut members = Vec::new();
    let mut i = 0;
    while u32_at(i) == 0x0403_4b50 {
        assert_eq!(u16_at(i + 8), 0, "stored");
        let (size, name_len, extra) = (u32_at(i + 18), u16_at(i + 26), u16_at(i + 28));
        let name = String::from_utf8(bytes[i + 30..i + 30 + name_len].to_vec()).unwrap();
        let start = i + 30 + name_len + extra;
        members.push((name, bytes[start..start + size].to_vec()));
        i = start + size;
    }
    members
}

#[test]
fn results_export_as_a_numpy_archive() {
    let (mut project, _) = load_project(&data().join("cylinder.hbm")).unwrap();
    project.waves.frequencies.inputs.truncate(2);
    project.waves.frequencies.count = 2;
    let result = SolveHandle::start(make_solve_request(&project).unwrap(), || {}).wait().unwrap();
    let out = scratch("npz").join("cylinder.npz");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    export_npz(&result, &out).unwrap();
    export_npz(&result, &out).unwrap(); // again over it
    let members = zip_members(&std::fs::read(&out).unwrap());
    let r = &result.results;
    // every quantity, in the engine's type and shape, then the bodies, the motion equation, the
    // bodies' placement and the meta data
    let mut names: Vec<String> = r.available().iter().map(|q| format!("{}.npy", q.name())).collect();
    for extra in [
        "bodies",
        "motion_equation_inertia",
        "motion_equation_stiffness",
        "motion_equation_damping",
        "body_position",
        "body_yaw",
        "body_rotation_center",
        "body_trim",
        "meta",
    ] {
        names.push(format!("{extra}.npy"));
    }
    assert_eq!(members.iter().map(|m| m.0.clone()).collect::<Vec<_>>(), names);
    for (q, (_, npy)) in r.available().into_iter().zip(&members) {
        let shape = r.shape(q).unwrap();
        let header_len = u16::from_le_bytes([npy[8], npy[9]]) as usize;
        let header = std::str::from_utf8(&npy[10..10 + header_len]).unwrap();
        let descr = gui_core::npz::descr(shape.element_type).unwrap();
        assert!(header.contains(&format!("'descr': '{descr}'")), "{header}");
        let dims: Vec<String> = shape.dims.iter().map(|d| d.to_string()).collect();
        let dims = if dims.len() == 1 { format!("({},)", dims[0]) } else { format!("({})", dims.join(", ")) };
        assert!(header.contains(&format!("'shape': {dims}")), "{} {header}", q.name());
        assert_eq!(&npy[10 + header_len..], r.bytes(q).unwrap(), "{}", q.name());
    }
    assert_eq!(r.shape(QuantityId::AddedMass).unwrap().dims, [2, 6, 6]);
    // the meta data: a JSON text (UTF-32) with the quantities' axes
    let meta = &members.last().unwrap().1;
    let header_len = u16::from_le_bytes([meta[8], meta[9]]) as usize;
    let text: String = meta[10 + header_len..]
        .chunks(4)
        .map(|c| char::from_u32(u32::from_le_bytes(c.try_into().unwrap())).unwrap())
        .filter(|c| *c != '\0')
        .collect();
    let meta: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(meta["version"], 1);
    assert_eq!(meta["quantities"]["added_mass"]["dims"], QuantityId::AddedMass.dims());
    assert!(!QuantityId::AddedMass.dims().is_empty());
    assert_eq!(meta["motion_equation_ready"], true);
    assert!(meta["second_order_motions"].as_str().unwrap().contains("motion_equation"));
    assert_eq!(meta["model"]["body_yaw"]["unit"], "deg");

    // the motion equation of the model solved: 6 x 6, its mass the body's
    let member = |name: &str| -> Vec<f64> {
        let npy = &members.iter().find(|m| m.0 == format!("{name}.npy")).unwrap().1;
        let header_len = u16::from_le_bytes([npy[8], npy[9]]) as usize;
        let header = std::str::from_utf8(&npy[10..10 + header_len]).unwrap();
        assert!(header.contains("'descr': '<f8'"), "{name}: {header}");
        npy[10 + header_len..].chunks(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect()
    };
    let inertia = member("motion_equation_inertia");
    assert_eq!(inertia.len(), 36);
    let mass = r.f64s(QuantityId::HydrostaticsMass).unwrap()[0];
    for k in 0..3 {
        assert!((inertia[k * 7] - mass).abs() <= 1e-9 * mass, "{} of {mass}", inertia[k * 7]);
    }
    // the stiffness holds the hydrostatic one (rho g and weight terms) of the results
    let stiffness = member("motion_equation_stiffness");
    let hydrostatic = r.f64s(QuantityId::HydrostaticsStiffness).unwrap();
    for (a, b) in stiffness.iter().zip(hydrostatic) {
        assert!((a - b).abs() <= 1e-9 * b.abs().max(1.0), "{a} {b}");
    }
    // the placement of the body as solved
    let body = &project.bodies[0];
    assert_eq!(member("body_position"), body.position);
    assert_eq!(member("body_yaw"), [body.yaw]);
    assert_eq!(member("body_rotation_center"), body.reference);
    assert_eq!(member("body_trim"), [0.0; 3]);
}

/// The motion equation and placement keys of the NumPy export of a model solved at one frequency.
fn npz_model_keys(model: &str) -> (Vec<(String, Vec<u8>)>, Project) {
    let (mut project, _) = load_project(&data().join(model)).unwrap();
    project.waves.frequencies.inputs.truncate(1);
    project.waves.frequencies.count = 1;
    project.waves.frequencies.edit_in_table = true;
    project.second_order = Default::default();
    project.field_points.enabled = false;
    for b in &mut project.bodies {
        b.control_surface = None;
    }
    let result = SolveHandle::start(make_solve_request(&project).unwrap(), || {}).wait().unwrap();
    let out = scratch("npz-model").join(model).with_extension("npz");
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    export_npz(&result, &out).unwrap();
    (zip_members(&std::fs::read(&out).unwrap()), project)
}

fn npz_reals(members: &[(String, Vec<u8>)], name: &str) -> Option<Vec<f64>> {
    let npy = &members.iter().find(|m| m.0 == format!("{name}.npy"))?.1;
    let header_len = u16::from_le_bytes([npy[8], npy[9]]) as usize;
    Some(npy[10 + header_len..].chunks(8).map(|c| f64::from_le_bytes(c.try_into().unwrap())).collect())
}

#[test]
fn the_numpy_archive_has_every_body_placed_and_the_trim() {
    // three bodies: 18 x 18, each placed as solved
    let (members, project) = npz_model_keys("three_truncated_cylinders.hbm");
    assert_eq!(npz_reals(&members, "motion_equation_inertia").unwrap().len(), 18 * 18);
    let position = npz_reals(&members, "body_position").unwrap();
    let expected: Vec<f64> = project.bodies.iter().flat_map(|b| b.position).collect();
    assert_eq!(position, expected);
    assert!(position.iter().any(|v| *v != 0.0));
    // a trimmed hull: its trim
    let (members, project) = npz_model_keys("trimmed_moonpool_cover.hbm");
    let trim = npz_reals(&members, "body_trim").unwrap();
    assert_eq!(trim, project.bodies[0].trim.expect("a trim"));
    // a pressure mode: D = 7, the motion equation of the rigid coordinates only (6 x 6)
    let (members, _) = npz_model_keys("floating_owc_chamber.hbm");
    assert_eq!(npz_reals(&members, "motion_equation_inertia").unwrap().len(), 36);
}

/// The Python package's `hb.save_npz` (`python/hydrobem_gui/npz.py`) writes the GUI's archive: the
/// texts of the GUI's `meta` (the keys' axes, units and descriptions, the note on second-order
/// motions, the format and version, the run's fields) are those of the Python module.
#[test]
fn npz_python_texts_match() {
    let (members, _) = npz_model_keys("cylinder.hbm");
    let npy = &members.iter().find(|m| m.0 == "meta.npy").unwrap().1;
    let header_len = u16::from_le_bytes([npy[8], npy[9]]) as usize;
    let text: String = npy[10 + header_len..]
        .chunks(4)
        .map(|c| char::from_u32(u32::from_le_bytes(c.try_into().unwrap())).unwrap())
        .filter(|c| *c != '\0')
        .collect();
    let meta: serde_json::Value = serde_json::from_str(&text).unwrap();
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../python/hydrobem_gui/npz.py");
    let source = std::fs::read_to_string(&file).unwrap().replace("\r\n", "\n");
    // the Python texts joined where a literal continues on the next line ("..."\n    "...")
    let mut python = String::new();
    let mut rest = source.as_str();
    while let Some(i) = rest.find("\"\n") {
        let after = rest[i + 2..].trim_start_matches(' ');
        python.push_str(&rest[..i]);
        rest = match after.strip_prefix('"') {
            Some(next) => next,
            None => {
                python.push_str("\"\n");
                &rest[i + 2..]
            }
        };
    }
    python.push_str(rest);
    let quoted = |s: &str| python.contains(&format!("\"{s}\""));
    let mut texts = vec![meta["format"].as_str().unwrap(), meta["second_order_motions"].as_str().unwrap()];
    for (key, v) in meta["model"].as_object().unwrap() {
        texts.push(key);
        let dims = v["dims"].as_str().unwrap();
        // the motion equation's axes are one constant of the module
        assert!(quoted(dims) || dims.starts_with("rigid coordinate"), "{key}: {dims}");
        texts.extend([v["unit"].as_str().unwrap(), v["description"].as_str().unwrap()]);
    }
    texts.push(meta["model"]["motion_equation_inertia"]["dims"].as_str().unwrap());
    for t in texts {
        assert!(quoted(t), "npz.py lacks the text \"{t}\"");
    }
    for key in meta["run"].as_object().unwrap().keys().chain(meta.as_object().unwrap().keys()) {
        assert!(quoted(key), "npz.py lacks the key \"{key}\"");
    }
    assert!(python.contains(&format!("VERSION = {}", gui_core::files::NPZ_VERSION)));
}
