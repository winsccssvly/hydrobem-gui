//! Solves through the engine: the box, motions, several bodies, pressure surfaces, walls, a
//! channel, lids, interior previews, warnings, located errors and cancellation.

use std::sync::Arc;

use gui_core::engine_model::{BuildOptions, build_model, interior_preview, read_model};
use gui_core::hydrobem_engine::{QuantityId, Stage, Warning, wavenumber};
use gui_core::mesh::{Mesh, Symmetry};
use gui_core::project::{
    Body, Coupling, DampingLid, DepthType, Domain, Environment, FieldPoints, Formulation, InteriorMethod,
    InteriorSurface, MassCalculation, Matrix6, Project, SurfaceSource, V3,
};
use gui_core::results::{FieldKind, SolveResult, location_text};
use gui_core::solve::{SolveError, SolveEvent, SolveHandle, estimate_memory, make_solve_request, without_memory_check};

fn p(x: f64, y: f64, z: f64) -> V3 {
    [x, y, z]
}

/// An open-top box, 2 x 2 x 1 m, the wetted surface only, normals into the fluid.
fn box_mesh() -> Arc<Mesh> {
    Mesh::from_panels(
        "box",
        Symmetry::None,
        vec![
            [p(-1., -1., -1.), p(-1., 1., -1.), p(1., 1., -1.), p(1., -1., -1.)],
            [p(1., -1., -1.), p(1., 1., -1.), p(1., 1., 0.), p(1., -1., 0.)],
            [p(-1., -1., 0.), p(-1., 1., 0.), p(-1., 1., -1.), p(-1., -1., -1.)],
            [p(-1., 1., -1.), p(-1., 1., 0.), p(1., 1., 0.), p(1., 1., -1.)],
            [p(-1., -1., 0.), p(-1., -1., -1.), p(1., -1., -1.), p(1., -1., 0.)],
        ],
    )
}

/// The quarter (x, y >= 0) of the box mirrored about x = 0 and y = 0, with its waterplane lid as
/// record 4 (z = 0, normal up).
fn quarter_box_with_lid() -> Arc<Mesh> {
    Mesh::from_panels(
        "quarter",
        Symmetry::XzAndYz,
        vec![
            [p(0., 0., -1.), p(0., 1., -1.), p(1., 1., -1.), p(1., 0., -1.)],
            [p(1., 0., -1.), p(1., 1., -1.), p(1., 1., 0.), p(1., 0., 0.)],
            [p(0., 1., -1.), p(0., 1., 0.), p(1., 1., 0.), p(1., 1., -1.)],
            [p(0., 0., 0.), p(1., 0., 0.), p(1., 1., 0.), p(0., 1., 0.)],
        ],
    )
}

/// One horizontal 1 x 1 m panel at z with its normal down.
fn plate(z: f64, xc: f64) -> Arc<Mesh> {
    Mesh::from_panels(
        "plate",
        Symmetry::None,
        vec![[p(xc - 0.5, -0.5, z), p(xc - 0.5, 0.5, z), p(xc + 0.5, 0.5, z), p(xc + 0.5, -0.5, z)]],
    )
}

/// A radiation problem at one frequency with boxes at the given x.
fn boxes_at(xs: &[f64]) -> Project {
    let mut project = Project::default();
    for x in xs {
        let i = project.add_body();
        project.bodies[i].mesh = Some(box_mesh());
        project.bodies[i].position = [*x, 0.0, 0.0];
    }
    project.environment.density = 1000.0;
    project.first_order.diffraction = false;
    project.waves.frequencies.inputs = vec![1.0];
    project.waves.frequencies.count = 1;
    project
}

fn box_project() -> Project {
    let mut project = Project::default();
    let i = project.add_body();
    project.bodies[i].name = "Hull".into();
    project.bodies[i].mesh = Some(box_mesh());
    project.environment.density = 1000.0;
    project.waves.frequencies.inputs = vec![0.5, 1.0];
    project.field_points = FieldPoints {
        enabled: true,
        x_start: 3.0,
        x_end: 4.0,
        x_count: 2,
        y_start: 0.0,
        y_end: 0.0,
        y_count: 1,
        z: 0.0,
        ..FieldPoints::default()
    };
    project.update_mesh_properties();
    project
}

fn solve(project: &Project) -> Result<SolveResult, SolveError> {
    SolveHandle::start(make_solve_request(project)?, || {}).wait()
}

fn added_mass(result: &SolveResult, row: usize, id: usize) -> f64 {
    result.radiation_column(id).map(|c| result.coefficient_series(QuantityId::AddedMass, row, c)[0]).unwrap_or(f64::NAN)
}

fn close(a: f64, b: f64, relative: f64) -> bool {
    (a - b).abs() <= relative * a.abs().max(b.abs())
}

#[test]
fn the_memory_estimate_grows_with_frequency_threads() {
    let mut project = box_project();
    let one = estimate_memory(&mut make_solve_request(&project).unwrap()).unwrap();
    assert!(one.statics > 0 && one.per_frequency > 0);
    project.options.frequency_threads = 2;
    let two = estimate_memory(&mut make_solve_request(&project).unwrap()).unwrap();
    assert_eq!(two.peak, one.peak + one.per_frequency);
}

#[test]
fn the_wave_number_is_the_deep_water_dispersion() {
    let k = wavenumber(1.0, f64::INFINITY, 9.80665).unwrap();
    assert!((k - 1.0 / 9.80665).abs() < 1e-12);
}

#[test]
fn a_panic_of_the_solve_ends_it_with_an_error() {
    // a panic inside the solve (here of the progress report) stands for an engine bug
    let once = std::sync::atomic::AtomicBool::new(false);
    let request = make_solve_request(&box_project()).unwrap();
    let mut handle = SolveHandle::start(request, move || {
        if !once.swap(true, std::sync::atomic::Ordering::SeqCst) {
            panic!("a test panic");
        }
    });
    let outcome = loop {
        std::thread::sleep(std::time::Duration::from_millis(5));
        if let Some(SolveEvent::Finished(outcome)) =
            handle.poll().into_iter().find(|e| matches!(e, SolveEvent::Finished(_)))
        {
            break *outcome;
        }
    };
    let error = outcome.err().expect("an error");
    assert!(error.message.contains("a test panic"), "{}", error.message);
    assert!(!error.cancelled);
}

#[test]
fn the_box_solves() {
    let request = make_solve_request(&box_project()).unwrap();
    let mut handle = SolveHandle::start(request, || {});
    let (mut done, mut total) = (0, 0);
    let result = loop {
        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut finished = None;
        for event in handle.poll() {
            match event {
                SolveEvent::Progress(p) if p.stage == Stage::FrequencyDone => {
                    (done, total) = (p.frequencies_done, p.frequencies_total)
                }
                SolveEvent::Progress(_) => {}
                SolveEvent::Finished(outcome) => finished = Some(*outcome),
            }
        }
        if let Some(outcome) = finished {
            break outcome.unwrap();
        }
    };
    assert_eq!((done, total), (2, 2));
    assert!(!result.route.is_empty());
    assert_eq!(result.bodies.len(), 1);
    assert_eq!(result.coordinates, 6);
    assert_eq!(result.omegas(), vec![0.5, 1.0]);
    assert_eq!(result.headings(), vec![0.0]);
    assert_eq!(result.radiation_mode_ids().len(), 6);
    assert_eq!(result.dims(QuantityId::AddedMass), Some(vec![2, 6, 6]));
    assert_eq!(result.dims(QuantityId::ExcitationDiffraction), Some(vec![2, 1, 6]));
    assert!(result.has(QuantityId::ExcitationFroudeKrylov) && result.has(QuantityId::ExcitationScattering));
    assert!(!result.has(QuantityId::Motion));
    assert_eq!(result.field_points.len(), 2);
    assert_eq!(result.dims(QuantityId::FieldPointsPressureRadiation), Some(vec![2, 2, 6]));
    assert_eq!(result.dims(QuantityId::FieldPointsPressureIncident), Some(vec![2, 2, 1]));

    let heave = result.coefficient_series(QuantityId::AddedMass, 2, result.radiation_column(3).unwrap());
    assert!(heave.len() == 2 && heave[0] > 0.0 && heave[0].is_finite());
    let incident = result.field_amplitude(FieldKind::Incident, 0, 0, None);
    assert!((incident[0] - 1.0).abs() < 1e-3, "{incident:?}"); // a unit-amplitude incident wave
    // Excitation is the engine's fixed-body elevation: incident + scattering.
    let inc = result.results.complex32s(QuantityId::FieldPointsPressureIncident).unwrap();
    let sca = result.results.complex32s(QuantityId::FieldPointsPressureScattering).unwrap();
    let excitation = result.field_amplitude(FieldKind::Excitation, 1, 0, None);
    for (i, e) in excitation.iter().enumerate() {
        let sum = (inc[2 + i] + sca[2 + i]).norm() as f64;
        assert!((e - sum).abs() <= 1e-5 * sum, "{e} vs {sum}");
    }
    // the hydrostatics computed with the solve, the bodies on D and the meta data
    assert_eq!(result.hydrostatics.len(), 1);
    assert!(close(result.hydrostatics[0].volume[2], 4.0, 1e-9));
    assert!(close(result.hydrostatics[0].waterplane_area, 4.0, 1e-9));
    assert_eq!(result.bodies[0].name, "Hull");
    assert!(result.engine_seconds > 0.0);
    assert_eq!(result.generated_field_points, 2);
}

#[test]
fn a_rao_solve_has_motions() {
    let mut project = box_project();
    project.first_order.motion = true;
    let b = &mut project.bodies[0];
    b.mass_calculation = MassCalculation::Manual;
    b.mass = 4000.0;
    b.center_of_gravity = [0.0, 0.0, -0.5];
    b.inertia = [4000.0 * 5.0 / 12.0, 0.0, 0.0, 4000.0 * 5.0 / 12.0, 0.0, 4000.0 * 8.0 / 12.0];
    let result = solve(&project).unwrap();
    assert_eq!(result.dims(QuantityId::Motion), Some(vec![2, 1, 6]));
    assert!(result.fhd_series(QuantityId::Motion, 0, 2)[0].norm() > 0.0);
    // Total = excitation + the radiation of the motions
    let total = result.field_amplitude(FieldKind::Total, 0, 0, None);
    let excitation = result.field_amplitude(FieldKind::Excitation, 0, 0, None);
    assert!(total.iter().zip(&excitation).any(|(t, e)| (t - e).abs() > 1e-6));
}

#[test]
fn validation_errors_carry_the_engine_message() {
    let mut project = box_project();
    project.bodies[0].pitch = 5.0;
    assert!(make_solve_request(&project).is_err());
    let mut project = box_project();
    project.environment.density = -1.0;
    let e = make_solve_request(&project).err().expect("rejected");
    assert!(e.message.contains("density"), "{}", e.message);
}

/// Mirror-image boxes: equal self terms and reciprocal cross terms, which only hold when every body
/// is read at its body-major D offset (0 and 6).
#[test]
fn two_bodies_use_body_major_offsets() {
    let result = solve(&boxes_at(&[-3.0, 3.0])).unwrap();
    assert_eq!(result.coordinates, 12);
    assert_eq!(result.bodies[1].coordinate_offset, 6);
    assert_eq!(result.coordinate_row(1, 2), Some(8));
    let (s0, s1) = (added_mass(&result, 2, 3), added_mass(&result, 8, 9));
    assert!(s0 > 0.0 && close(s0, s1, 1e-4), "{s0} vs {s1}");
    let (c01, c10) = (added_mass(&result, 2, 9), added_mass(&result, 8, 3));
    assert!(c01 != 0.0 && close(c01, c10, 1e-4), "{c01} vs {c10}");
}

/// A suppressed body is in the model, disabled: the solve is the one without it, the warnings and
/// errors count it, and it opens again suppressed with its mesh.
#[test]
fn a_suppressed_body_is_disabled_in_the_model() {
    let alone = solve(&boxes_at(&[-3.0])).unwrap();
    let mut project = boxes_at(&[-3.0, 3.0]);
    project.bodies[1].suppressed = true;
    let built = build_model(&project, &BuildOptions::default()).unwrap();
    assert_eq!(built.body_names, [project.bodies[0].name.clone()]);
    assert_eq!(built.model_bodies.len(), 2);
    let b = built.model.object(&format!("bodies/{}", project.bodies[1].name)).unwrap();
    assert_eq!(built.model.get(b, "enabled").unwrap().and_then(|v| v.as_bool()), Some(false));
    let (read, _) = read_model(&built.model).unwrap();
    assert!(!read.bodies[0].suppressed && read.bodies[1].suppressed && read.bodies[1].mesh.is_some());
    let without = solve(&project).unwrap();
    assert_eq!(without.coordinates, 6);
    assert_eq!(without.model_bodies.len(), 2);
    assert_eq!(added_mass(&without, 2, 3), added_mass(&alone, 2, 3));
}

/// A coupling names the other body: it follows a rename, goes with a deleted body, is left out
/// with a suppressed one, and a spring between the heaves changes the motions.
#[test]
fn a_coupling_between_two_bodies() {
    let mut project = boxes_at(&[-3.0, 3.0]);
    project.first_order.diffraction = true;
    project.first_order.motion = true;
    for b in &mut project.bodies {
        b.mass_calculation = MassCalculation::Manual;
        b.mass = 4000.0;
        b.center_of_gravity = [0.0, 0.0, -0.5];
        b.inertia = [4000.0 * 5.0 / 12.0, 0.0, 0.0, 4000.0 * 5.0 / 12.0, 0.0, 4000.0 * 8.0 / 12.0];
    }
    let free = solve(&project).unwrap();
    let mut k = Matrix6::default();
    k.set(2, 2, -2.0e4);
    let other = project.bodies[1].name.clone();
    project.bodies[0].couplings.push(Coupling { body: other.clone(), stiffness: k, ..Coupling::default() });
    let built = build_model(&project, &BuildOptions::default()).unwrap();
    let (read, _) = read_model(&built.model).unwrap();
    assert_eq!(read.bodies[0].couplings.len(), 1);
    assert_eq!(read.bodies[0].couplings[0].body, other);
    assert_eq!(read.bodies[0].couplings[0].stiffness.get(2, 2), -2.0e4);
    let coupled = solve(&project).unwrap();
    let heave = |r: &SolveResult| r.fhd_series(QuantityId::Motion, 0, 2)[0].norm();
    assert!((heave(&coupled) - heave(&free)).abs() > 1e-3 * heave(&free), "{} {}", heave(&coupled), heave(&free));

    project.rename_body(&other, "Partner");
    project.bodies[1].name = "Partner".into();
    assert_eq!(project.bodies[0].couplings[0].body, "Partner");
    assert!(solve(&project).is_ok());
    // a suppressed body takes its couplings with it
    project.bodies[1].suppressed = true;
    assert!(solve(&project).is_ok());
    project.forget_body("Partner");
    assert!(project.bodies[0].couplings.is_empty());
}

/// A pressure mode of the first body is its seventh coordinate; the second body starts at 7.
#[test]
fn a_pressure_coordinate_follows_its_body() {
    let mut project = boxes_at(&[0.0, 4.0]);
    project.bodies[0].pressure_surfaces = vec![gui_core::project::PanelSurface {
        source: SurfaceSource::ExternalMesh,
        mesh: Some(plate(-0.2, 0.0)),
        ..Default::default()
    }];
    let result = solve(&project).unwrap();
    assert_eq!(result.coordinates, 13);
    assert_eq!(result.bodies[0].pressure_modes, 1);
    assert_eq!(result.bodies[1].coordinate_offset, 7);
    assert_eq!(result.coordinate_row(1, 2), Some(9));
    assert!(result.radiation_column(7).is_some());
    let (c01, c10) = (added_mass(&result, 2, 10), added_mass(&result, 9, 3));
    assert!(c01 != 0.0 && close(c01, c10, 1e-2), "{c01} vs {c10}");
    for wrong in [8, 10] {
        assert!(!close(c01, added_mass(&result, wrong, 3), 1e-2));
    }
}

/// Method of images: a wall at x = 0 is the open sea with the mirrored body moving in phase.
#[test]
fn an_x_wall_matches_its_mirror_image() {
    let mut wall = boxes_at(&[3.0]);
    wall.environment.domain = Domain::XWall;
    let walled = solve(&wall).unwrap();
    let pair = solve(&boxes_at(&[3.0, -3.0])).unwrap();
    let with_wall = added_mass(&walled, 2, 3);
    let imaged = added_mass(&pair, 2, 3) + added_mass(&pair, 2, 9);
    assert!(close(with_wall, imaged, 1e-3), "{with_wall} vs {imaged}");
    assert!(!close(with_wall, added_mass(&pair, 2, 3), 1e-3));
}

#[test]
fn a_channel_solves_in_finite_depth() {
    let mut open = boxes_at(&[0.0]);
    let mut channel = boxes_at(&[0.0]);
    for p in [&mut open, &mut channel] {
        p.environment.depth_type = DepthType::Finite;
        p.environment.depth = 10.0;
    }
    channel.environment.domain = Domain::Channel;
    channel.environment.channel_width = 6.0;
    let a = added_mass(&solve(&channel).unwrap(), 2, 3);
    let b = added_mass(&solve(&open).unwrap(), 2, 3);
    assert!(a.is_finite() && a > 0.0 && !close(a, b, 1e-3), "{a} vs {b}");
}

#[test]
fn a_damping_lid_changes_the_added_mass() {
    let mut project = boxes_at(&[0.0]);
    project.first_order.diffraction_formulation = Formulation::Total;
    let i = project.add_lid();
    project.lids[i] = DampingLid { mesh: Some(plate(0.0, 2.5)), damping_factor: 0.5, ..project.lids[i].clone() };
    let result = solve(&project).unwrap();
    assert_eq!(result.coordinates, 6);
    assert!(result.has(QuantityId::Lids));
    let mut plain = boxes_at(&[0.0]);
    plain.first_order.diffraction_formulation = Formulation::Total;
    let (with, without) = (added_mass(&result, 2, 3), added_mass(&solve(&plain).unwrap(), 2, 3));
    assert!(with.is_finite() && !close(with, without, 1e-6));
}

/// The engine's generated interior surface: the 2 x 2 m waterplane of the box, at z = 0 with upward
/// normals, where the body is placed.
#[test]
fn the_interior_preview_is_the_engine_waterplane() {
    for method in [InteriorMethod::Projected, InteriorMethod::Waterline, InteriorMethod::Graded] {
        let mut body = Body { mesh: Some(box_mesh()), position: [5.0, 0.0, 0.0], ..Body::default() };
        body.interior.method = method;
        let panels = interior_preview(&body, &Environment::default()).unwrap();
        assert!(!panels.is_empty());
        let (mut area, mut sum_x) = (0.0, 0.0);
        for q in &panels {
            let n = cross(sub(q[2], q[0]), sub(q[3], q[1]));
            area += norm(n) / 2.0;
            assert!(n[2] >= 0.0, "{method:?}: normals point up");
            for v in q {
                assert!(v[2].abs() < 1e-9);
                assert!(v[0] >= 4.0 - 1e-9 && v[0] <= 6.0 + 1e-9 && v[1].abs() <= 1.0 + 1e-9);
                sum_x += v[0];
            }
        }
        assert!(close(area, 4.0, 1e-9), "{method:?}: {area}");
        assert!(close(sum_x / (4.0 * panels.len() as f64), 5.0, 0.2));
        body.roll = 5.0;
        assert!(interior_preview(&body, &Environment::default()).is_err());
    }
}

/// A selected range is a stored record shown in every symmetry copy.
#[test]
fn an_interior_panel_range_covers_every_symmetry_copy() {
    let mut body = Body { mesh: Some(quarter_box_with_lid()), ..Body::default() };
    body.interior =
        InteriorSurface { method: InteriorMethod::PanelRange, first_panel: 4, last_panel: 4, ..Default::default() };
    let panels = interior_preview(&body, &Environment::default()).unwrap();
    assert_eq!(panels.len(), 4);
    let area: f64 = panels.iter().map(|q| norm(cross(sub(q[2], q[0]), sub(q[3], q[1]))) / 2.0).sum();
    assert!(close(area, 4.0, 1e-12), "{area}");
    let fallback = body.mesh.as_ref().unwrap().expanded_range(4, 4);
    assert_eq!(fallback.len(), 4);
}

/// 2 m panels against a 6.9 m wave (omega = 3 rad/s) are coarser than a sixth of the wavelength.
#[test]
fn engine_warnings_reach_the_result() {
    let mut project = boxes_at(&[0.0]);
    project.waves.frequencies.inputs = vec![3.0];
    let result = solve(&project).unwrap();
    let coarse = result.diagnostics.iter().find(|d| d.warning() == Some(Warning::CoarseMesh)).expect("coarse mesh");
    assert_eq!(coarse.location.body, Some(0));
    assert!(coarse.location.element.is_some_and(|r| r >= 1));
    assert!(!coarse.message.is_empty());
    assert!(location_text(&coarse.location, &["Hull".into()]).starts_with("body 'Hull', panel "));
}

/// A rejected input names where it applies: a body beyond the wall x = 0.
#[test]
fn errors_carry_their_location() {
    let mut project = boxes_at(&[0.0, -3.0]);
    project.environment.domain = Domain::XWall;
    let e = match make_solve_request(&project) {
        Err(e) => e,
        Ok(request) => SolveHandle::start(request, || {}).wait().err().expect("rejected"),
    };
    let l = e.location.expect(&e.message);
    assert!(matches!(l.body, Some(0 | 1)), "{l:?}");
}

/// A cancel during the solve stops it.
#[test]
fn a_cancel_is_reported() {
    let mut project = box_project();
    project.waves.frequencies.inputs = vec![0.4, 0.6, 0.8, 1.0, 1.2, 1.4];
    let handle = SolveHandle::start(make_solve_request(&project).unwrap(), || {});
    handle.cancel();
    let e = handle.wait().err().expect("cancelled");
    assert!(e.cancelled, "{}", e.message);
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

#[test]
fn solving_anyway_turns_the_engines_memory_check_off() {
    // the user's Solve after the GUI's memory warning: the request's model has the engine's check
    // off (the project keeps the default), and it solves
    let project = box_project();
    let mut request = make_solve_request(&project).unwrap();
    let o = request.model.object("options").unwrap();
    assert_eq!(request.model.get(o, "memory_check").unwrap().and_then(|v| v.as_bool()), Some(true));
    without_memory_check(&mut request).unwrap();
    assert_eq!(request.model.get(o, "memory_check").unwrap().and_then(|v| v.as_bool()), Some(false));
    SolveHandle::start(request, || {}).wait().unwrap();
    let again = make_solve_request(&project).unwrap();
    let o = again.model.object("options").unwrap();
    assert_eq!(again.model.get(o, "memory_check").unwrap().and_then(|v| v.as_bool()), Some(true));
}
