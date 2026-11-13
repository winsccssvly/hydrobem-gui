//! The engine objects beyond the basics, each through the GUI's model and back and solved by the
//! engine: tanks, dipole dampers of a body and of the water, control surfaces with their drift,
//! trim, body points with body pressure and velocity, Haskind excitation, pressure and momentum
//! drift, field velocity, a channel's wall reflection.

use std::f64::consts::PI;
use std::sync::Arc;

use gui_core::engine_model::{BuildOptions, build_model, read_model, role_panels};
use gui_core::field_points::PointSource;
use gui_core::files::project_fingerprint;
use gui_core::hydrobem_engine::{PanelRole, QuantityId};
use gui_core::mesh::{Mesh, Symmetry};
use gui_core::project::{
    Body, ControlMethod, ControlSurface, DampingLid, DepthType, Domain, FieldPoints, GeneralizedMode, InteriorMethod,
    MassCalculation, Matrix6, MatrixOrigin, ModeMatrices, ModePiece, PanelSurface, PressureMode, PressureShape,
    Project, SurfaceSource, Tank, TankRef, TankSheet, V3, WaveFrequencies, WaveHeadings,
};
use gui_core::results::SolveResult;
use gui_core::solve::{SolveHandle, make_solve_request};

type Quad = [V3; 4];

/// Normals (q2 - q0) x (q3 - q1) along `n`.
fn orient(q: Quad, n: V3) -> Quad {
    let a = [q[2][0] - q[0][0], q[2][1] - q[0][1], q[2][2] - q[0][2]];
    let b = [q[3][0] - q[1][0], q[3][1] - q[1][1], q[3][2] - q[1][2]];
    let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    if c[0] * n[0] + c[1] * n[1] + c[2] * n[2] > 0.0 { q } else { [q[0], q[3], q[2], q[1]] }
}

/// A quarter (x, y >= 0) vertical cylinder from z0 down to z1, with its bottom; normals out.
fn quarter_cylinder(radius: f64, z0: f64, z1: f64, n_phi: usize, n_z: usize, n_r: usize) -> Vec<Quad> {
    let phi = |j: usize| 0.5 * PI * j as f64 / n_phi as f64;
    let mut out = Vec::new();
    for i in 0..n_z {
        let (za, zb) = (z0 + (z1 - z0) * i as f64 / n_z as f64, z0 + (z1 - z0) * (i + 1) as f64 / n_z as f64);
        for j in 0..n_phi {
            let (p0, p1) = (phi(j), phi(j + 1));
            let q = [
                [radius * p0.cos(), radius * p0.sin(), za],
                [radius * p1.cos(), radius * p1.sin(), za],
                [radius * p1.cos(), radius * p1.sin(), zb],
                [radius * p0.cos(), radius * p0.sin(), zb],
            ];
            let m = 0.5 * (p0 + p1);
            out.push(orient(q, [m.cos(), m.sin(), 0.0]));
        }
    }
    for i in 0..n_r {
        let (r0, r1) = (radius * (1.0 - i as f64 / n_r as f64), radius * (1.0 - (i + 1) as f64 / n_r as f64));
        for j in 0..n_phi {
            let (p0, p1) = (phi(j), phi(j + 1));
            let mut q = [
                [r0 * p0.cos(), r0 * p0.sin(), z1],
                [r0 * p1.cos(), r0 * p1.sin(), z1],
                [r1 * p1.cos(), r1 * p1.sin(), z1],
                [r1 * p0.cos(), r1 * p0.sin(), z1],
            ];
            if i + 1 == n_r {
                q[3] = q[2];
            }
            out.push(orient(q, [0.0, 0.0, -1.0]));
        }
    }
    out
}

/// A quarter annulus at z (a ring), normals up.
fn quarter_annulus(r0: f64, r1: f64, z: f64, n_phi: usize) -> Vec<Quad> {
    (0..n_phi)
        .map(|j| {
            let (p0, p1) = (0.5 * PI * j as f64 / n_phi as f64, 0.5 * PI * (j + 1) as f64 / n_phi as f64);
            orient(
                [
                    [r0 * p0.cos(), r0 * p0.sin(), z],
                    [r1 * p0.cos(), r1 * p0.sin(), z],
                    [r1 * p1.cos(), r1 * p1.sin(), z],
                    [r0 * p1.cos(), r0 * p1.sin(), z],
                ],
                [0.0, 0.0, 1.0],
            )
        })
        .collect()
}

/// A quarter (x, y >= 0) open-top box 0..a, 0..b, -h..0 (the bottom and the two outer sides),
/// normals out of the box.
fn quarter_box(a: f64, b: f64, h: f64, n: (usize, usize, usize)) -> Vec<Quad> {
    let (nx, ny, nz) = n;
    let mut out = Vec::new();
    for i in 0..nx {
        for j in 0..ny {
            let (x0, x1) = (a * i as f64 / nx as f64, a * (i + 1) as f64 / nx as f64);
            let (y0, y1) = (b * j as f64 / ny as f64, b * (j + 1) as f64 / ny as f64);
            out.push(orient([[x0, y0, -h], [x1, y0, -h], [x1, y1, -h], [x0, y1, -h]], [0.0, 0.0, -1.0]));
        }
    }
    for j in 0..ny {
        for k in 0..nz {
            let (y0, y1) = (b * j as f64 / ny as f64, b * (j + 1) as f64 / ny as f64);
            let (z0, z1) = (-h * k as f64 / nz as f64, -h * (k + 1) as f64 / nz as f64);
            out.push(orient([[a, y0, z0], [a, y1, z0], [a, y1, z1], [a, y0, z1]], [1.0, 0.0, 0.0]));
        }
    }
    for i in 0..nx {
        for k in 0..nz {
            let (x0, x1) = (a * i as f64 / nx as f64, a * (i + 1) as f64 / nx as f64);
            let (z0, z1) = (-h * k as f64 / nz as f64, -h * (k + 1) as f64 / nz as f64);
            out.push(orient([[x0, b, z0], [x1, b, z0], [x1, b, z1], [x0, b, z1]], [0.0, 1.0, 0.0]));
        }
    }
    out
}

fn mesh(name: &str, panels: Vec<Quad>) -> Arc<Mesh> {
    Mesh::from_panels(name, Symmetry::XzAndYz, panels)
}

/// A project with one body of `mesh` (displaced mass, inertia from radii of gyration about G).
fn project_with(mesh: Arc<Mesh>, g: V3, radii: V3, omegas: Vec<f64>, headings: WaveHeadings) -> Project {
    let mut p = Project::default();
    p.add_mesh("hull", mesh.clone());
    let i = p.add_body();
    let b = &mut p.bodies[i];
    b.mesh = Some(mesh);
    b.mass_calculation = MassCalculation::FromMesh;
    b.center_of_gravity = g;
    p.environment.density = 1000.0;
    p.waves.frequencies = WaveFrequencies {
        count: omegas.len() as i64,
        inputs: omegas,
        edit_in_table: true,
        ..WaveFrequencies::default()
    };
    p.waves.headings = headings;
    p.update_mesh_properties();
    let m = p.bodies[i].mass;
    p.bodies[i].inertia = [m * radii[0] * radii[0], 0.0, 0.0, m * radii[1] * radii[1], 0.0, m * radii[2] * radii[2]];
    gui_core::engine_model::move_inertia(&mut p.bodies[i], g);
    p
}

/// The project survives the model and back: the same fingerprint.
fn round_trip(p: &Project) -> Project {
    let mut p = p.clone();
    p.update_mesh_properties();
    let p = &p;
    let built = build_model(p, &BuildOptions::default()).unwrap_or_else(|e| panic!("{}", e.message()));
    let (q, warnings) = read_model(&built.model).unwrap_or_else(|e| panic!("{e}"));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(project_fingerprint(&q), project_fingerprint(p), "the model round trip changed the project");
    q
}

fn solve(p: &Project) -> SolveResult {
    let request = make_solve_request(p).unwrap_or_else(|e| panic!("{}", e.message));
    SolveHandle::start(request, || {}).wait().unwrap_or_else(|e| panic!("{}", e.message))
}

fn role_count(p: &Project, role: PanelRole) -> usize {
    let roles = role_panels(p).unwrap_or_else(|e| panic!("{e}"));
    roles.bodies.values().flatten().filter(|(r, _)| *r == role).count()
}

fn heading(h: f64) -> WaveHeadings {
    WaveHeadings { start: h, end: h, count: 1 }
}

#[test]
fn a_tank_with_haskind_excitation_and_pressure_drift() {
    // a box barge 2 x 1 x 1 with a central tank 1.6 x 0.8, fluid 0.5 deep below z = -0.1
    let hull = quarter_box(1.0, 0.5, 1.0, (8, 4, 4));
    let walls: Vec<Quad> = quarter_box(0.8, 0.4, 0.5, (8, 4, 3))
        .into_iter()
        .map(|q| [q[3], q[2], q[1], q[0]].map(|v| [v[0], v[1], v[2] - 0.1]))
        .collect();
    let (nh, nw) = (hull.len() as i64, walls.len() as i64);
    let mut p = project_with(
        mesh("tank_vessel", [hull, walls].concat()),
        [0.0, 0.0, -0.2],
        [0.4, 0.8, 0.8],
        vec![1.0, 2.0],
        heading(0.0),
    );
    let b = &mut p.bodies[0];
    b.mesh_range = Some([1, nh]);
    b.tanks.push(Tank { first_panel: nh + 1, last_panel: nh + nw, ..Tank::default() });
    b.haskind = true;
    p.first_order.motion = true;
    p.second_order.drift_pressure_integration = true;
    let q = round_trip(&p);
    assert_eq!(q.bodies[0].tanks.len(), 1);
    assert!(q.bodies[0].haskind);
    // the viewport draws the engine's panels: the tank walls apart from the hull, all four quarters
    assert_eq!(role_count(&p, PanelRole::Tank), 4 * nw as usize);
    let r = solve(&p);
    // [F, P, D]: one heading pair (the heading with itself), the six coordinates
    assert_eq!(r.dims(QuantityId::DriftPressureIntegration), Some(vec![2, 1, 6]));
    assert_eq!(r.drift_series(QuantityId::DriftPressureIntegration, 0, 0).len(), 2);
    assert!(r.has(QuantityId::Motion));
}

#[test]
fn a_control_surface_with_its_drift_and_the_momentum_drift() {
    let cylinder = quarter_cylinder(1.0, 0.0, -0.5, 8, 4, 4);
    let mut p = project_with(
        mesh("cylinder", cylinder),
        [0.0, 0.0, -0.2],
        [0.6, 0.6, 0.7],
        vec![1.0, 1.6],
        WaveHeadings { start: 0.0, end: 60.0, count: 3 },
    );
    p.bodies[0].control_surface = Some(ControlSurface {
        method: ControlMethod::Automatic,
        radius: 2.0,
        depth: 1.0,
        panel_size: 0.5,
        ..ControlSurface::default()
    });
    p.first_order.motion = true;
    p.second_order.drift_control_surface = true;
    p.second_order.drift_momentum_conservation = true;
    p.second_order.drift_bidirectional = true;
    let q = round_trip(&p);
    assert_eq!(q.bodies[0].control_surface.as_ref().map(|c| c.method), Some(ControlMethod::Automatic));
    assert!(q.second_order.drift_momentum_conservation && q.second_order.drift_bidirectional);
    assert!(role_count(&p, PanelRole::ControlSurface) > 0);
    // drawn on its exact surface (curved patches also in this low-order model): every corner of
    // its outer wall at the radius, not on the chords between the patch corners
    let roles = role_panels(&p).unwrap();
    let wall: Vec<[f64; 3]> = roles.smooth[&0]
        .iter()
        .filter(|(r, _)| *r == PanelRole::ControlSurface)
        .flat_map(|(_, q)| q.corners)
        .filter(|v| v[2] < -0.05 && v[2] > -0.95)
        .collect();
    assert!(wall.len() > 16, "{} corners inside the wall", wall.len());
    for v in &wall {
        let r = (v[0] * v[0] + v[1] * v[1]).sqrt();
        assert!((r - 2.0).abs() < 1e-6, "a corner of the wall at r = {r}");
    }
    let r = solve(&p);
    // bidirectional: every pair of the three headings with the first not after the second
    assert_eq!(r.drift_heading_pairs().len(), 6);
    assert!(r.drift_bidirectional());
    assert_eq!(r.dims(QuantityId::DriftControlSurface), Some(vec![2, 6, 6]));
    assert_eq!(r.dims(QuantityId::DriftMomentumConservation), Some(vec![2, 6, 3]));
    for h in 0..3 {
        let cs = (0..6).flat_map(|c| r.drift_series(QuantityId::DriftControlSurface, h, c));
        let mo = (0..3).flat_map(|c| r.drift_series(QuantityId::DriftMomentumConservation, h, c));
        let values: Vec<f64> = cs.chain(mo).collect();
        assert_eq!(values.len(), 2 * 9);
        assert!(values.iter().all(|v| v.is_finite()), "{values:?}");
    }
    // the matrix of the heading pairs: Hermitian
    let pairs = r.drift_pairs(QuantityId::DriftMomentumConservation, 0, 0);
    assert_eq!(pairs.len(), 3);
    assert!((pairs[0][1] - pairs[1][0].conj()).norm() <= 1e-12 * pairs[0][1].norm().max(1.0));
}

#[test]
fn dipole_dampers_of_a_body_and_of_the_water() {
    // the cylinder with a porous ring around it: of the water (at rest), then of the body (moving)
    let cylinder = quarter_cylinder(1.0, 0.0, -0.5, 8, 3, 3);
    let ring = mesh("ring", quarter_annulus(1.3, 2.0, -0.25, 8));
    let mut p =
        project_with(mesh("cylinder", cylinder), [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![0.8, 1.4], heading(0.0));
    p.add_mesh("ring", ring.clone());
    p.fixed_dampers.push(PanelSurface {
        source: SurfaceSource::ExternalMesh,
        mesh: Some(ring.clone()),
        damping_factor: 0.4,
        ..PanelSurface::default()
    });
    let q = round_trip(&p);
    assert_eq!(q.fixed_dampers.len(), 1);
    assert_eq!(q.fixed_dampers[0].damping_factor, 0.4);
    let water = solve(&p);
    // a damper of the body takes records of the body's mesh (the engine's rule): hull and ring as one
    let (hull, ring_panels) = (quarter_cylinder(1.0, 0.0, -0.5, 8, 3, 3), quarter_annulus(1.3, 2.0, -0.25, 8));
    let (nh, nr) = (hull.len() as i64, ring_panels.len() as i64);
    let mut p2 = project_with(
        mesh("hull_ring", [hull, ring_panels].concat()),
        [0.0, 0.0, -0.2],
        [0.6, 0.6, 0.7],
        vec![0.8, 1.4],
        heading(0.0),
    );
    p2.bodies[0].mesh_range = Some([1, nh]);
    p2.bodies[0].dipole_dampers.push(PanelSurface {
        first_panel: nh + 1,
        last_panel: nh + nr,
        damping_factor: 0.4,
        ..PanelSurface::default()
    });
    let _ = ring;
    let q2 = round_trip(&p2);
    assert_eq!(q2.bodies[0].dipole_dampers.len(), 1);
    let body = solve(&p2);
    // a damper moving with the body is another problem than one at rest
    let (bw, bb) = (
        water.coefficient_series(QuantityId::RadiationDamping, 2, 2),
        body.coefficient_series(QuantityId::RadiationDamping, 2, 2),
    );
    assert!((bw[0] - bb[0]).abs() > 1e-6 * bw[0].abs(), "{bw:?} vs {bb:?}");
}

#[test]
fn a_trimmed_body_is_cut_at_the_free_surface() {
    // a cylinder with 0.4 m freeboard, trimmed by a pitch of 2 deg and sunk 0.05 m
    let hull = quarter_cylinder(1.0, 0.4, -0.6, 8, 5, 2);
    let mut p = project_with(mesh("freeboard", hull), [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![1.0], heading(0.0));
    p.bodies[0].trim = Some([-0.05, 2.0, 0.0]);
    let q = round_trip(&p);
    assert_eq!(q.bodies[0].trim, Some([-0.05, 2.0, 0.0]));
    let r = solve(&p);
    assert!(r.has(QuantityId::AddedMass));
    // the displaced volume is that of the wetted part only (below z = 0, about pi 0.65 m^3)
    let v = r.hydrostatics[0].volume[0];
    assert!(v > 0.6 * PI && v < 0.7 * PI, "{v}");
}

#[test]
fn body_points_with_body_pressure_and_velocity_and_field_velocity() {
    let cylinder = quarter_cylinder(1.0, 0.0, -0.5, 8, 4, 4);
    let mut p = project_with(mesh("cylinder", cylinder), [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![1.0], heading(0.0));
    p.bodies[0].body_points = Some(vec![[1.0, 0.0, -0.25], [0.0, 1.0, -0.25], [0.3, 0.3, -0.5]]);
    p.first_order.body_pressure = true;
    p.first_order.body_velocity = true;
    p.first_order.field_velocity = true;
    p.field_points = FieldPoints {
        enabled: true,
        x_start: 2.0,
        x_end: 3.0,
        x_count: 2,
        y_start: 0.0,
        y_end: 0.0,
        y_count: 1,
        z: -0.5,
        ..FieldPoints::default()
    };
    let q = round_trip(&p);
    assert_eq!(q.bodies[0].body_points.as_ref().map(Vec::len), Some(3));
    let r = solve(&p);
    assert_eq!(r.dims(QuantityId::BodyPointsPressureDiffraction), Some(vec![1, 3, 1]));
    assert_eq!(r.dims(QuantityId::BodyPointsVelocityDiffraction), Some(vec![1, 3, 1, 3]));
    assert_eq!(r.dims(QuantityId::FieldPointsVelocityIncident), Some(vec![1, 2, 1, 3]));
    // the points of the tables: global positions of the body points, values at each
    let points = r.body_output_points();
    assert_eq!(points.len(), 3);
    assert!(points.iter().all(|(b, _)| *b == 0));
    assert_eq!(r.point_values(QuantityId::BodyPointsPressureDiffraction, 0, 0).len(), 3);
    // incident velocity at depth 0.5 in deep water: |u| = omega e^(k z) per unit amplitude
    let u = r.point_vectors(QuantityId::FieldPointsVelocityIncident, 0, 0);
    assert_eq!(u.len(), 2);
    let expected = (-0.5 / p.environment.gravity).exp();
    assert!((u[0][0].norm() - expected).abs() < 1e-3 * expected, "{} vs {expected}", u[0][0].norm());
}

#[test]
fn a_channel_with_partly_reflecting_walls() {
    let hull = quarter_box(1.0, 1.0, 1.0, (4, 4, 4));
    let mut p = project_with(mesh("box", hull), [0.0, 0.0, -0.5], [0.6, 0.6, 0.7], vec![1.0], heading(0.0));
    p.environment.depth_type = DepthType::Finite;
    p.environment.depth = 10.0;
    p.environment.domain = Domain::Channel;
    p.environment.channel_width = 8.0;
    p.environment.channel_reflect = 0.5;
    let q = round_trip(&p);
    assert_eq!(q.environment.channel_reflect, 0.5);
    let half = solve(&p);
    p.environment.channel_reflect = 1.0;
    let full = solve(&p);
    let (a, b) = (
        half.coefficient_series(QuantityId::AddedMass, 2, 2)[0],
        full.coefficient_series(QuantityId::AddedMass, 2, 2)[0],
    );
    assert!((a - b).abs() > 1e-6 * a.abs(), "{a} vs {b}");
}

#[test]
fn switched_off_items_are_disabled_in_the_model() {
    let hull = quarter_box(1.0, 0.5, 1.0, (8, 4, 4));
    let walls: Vec<Quad> = quarter_box(0.8, 0.4, 0.5, (8, 4, 3))
        .into_iter()
        .map(|q| [q[3], q[2], q[1], q[0]].map(|v| [v[0], v[1], v[2] - 0.1]))
        .collect();
    let (nh, nw) = (hull.len() as i64, walls.len() as i64);
    let mut p = project_with(
        mesh("tank_vessel", [hull, walls].concat()),
        [0.0, 0.0, -0.2],
        [0.4, 0.8, 0.8],
        vec![1.0],
        heading(0.0),
    );
    p.bodies[0].mesh_range = Some([1, nh]);
    p.bodies[0].tanks.push(Tank {
        enabled: false,
        name: "aft".into(),
        first_panel: nh + 1,
        last_panel: nh + nw,
        more_ranges: Vec::new(),
        density_ratio: 0.5,
        surface_level: Some(-0.2),
        sheets: Vec::new(),
        free_surface_plates: Vec::new(),
    });
    p.bodies[0].dipole_dampers.push(PanelSurface { enabled: false, damping_factor: 0.3, ..PanelSurface::default() });
    // in the model, disabled (the engine solves as if they were absent)
    let built = build_model(&p, &BuildOptions::default()).unwrap_or_else(|e| panic!("{}", e.message()));
    let body = &p.bodies[0].name;
    for path in [format!("bodies/{body}/tanks/aft"), format!("bodies/{body}/dipole_dampers/1")] {
        let id = built.model.object(&path).unwrap_or_else(|e| panic!("{path}: {}", e.message()));
        assert_eq!(built.model.get(id, "enabled").unwrap().and_then(|v| v.as_bool()), Some(false), "{path}");
    }
    let q = round_trip(&p);
    assert!(!q.bodies[0].tanks[0].enabled && q.bodies[0].tanks[0].surface_level == Some(-0.2));
    assert_eq!(q.bodies[0].tanks[0].name, "aft");
    assert!(!q.bodies[0].dipole_dampers[0].enabled);
    // the body's reference leaves their records out either way (the damper takes record 1)
    let hull = built.model.object(&format!("bodies/{body}")).unwrap();
    assert_eq!(built.model.get(hull, "ranges").unwrap().and_then(|v| v.as_ranges()), Some(vec![[2, nh]]));
    // solved as the body without them: their records without a role
    let without = solve(&p);
    p.bodies[0].tanks.clear();
    p.bodies[0].dipole_dampers.clear();
    p.bodies[0].mesh_range = Some([2, nh]);
    let absent = solve(&p);
    assert_eq!(
        without.coefficient_series(QuantityId::AddedMass, 2, 2),
        absent.coefficient_series(QuantityId::AddedMass, 2, 2)
    );
    let _ = Body::default();
}

#[test]
fn the_added_mass_limits_are_solved_apart_from_the_wave_frequencies() {
    let cylinder = quarter_cylinder(1.0, 0.0, -0.5, 8, 4, 4);
    let mut p =
        project_with(mesh("cylinder", cylinder), [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![1.0, 2.0], heading(0.0));
    p.waves.frequencies.zero_limit = true;
    p.waves.frequencies.infinite_limit = true;
    let q = round_trip(&p);
    assert!(q.waves.frequencies.zero_limit && q.waves.frequencies.infinite_limit);
    assert_eq!(q.waves.frequencies.inputs, vec![1.0, 2.0]);
    let r = solve(&p);
    assert_eq!(r.omegas(), vec![1.0, 2.0]);
    // heave added mass: finite at both limits, and the curve lies between them at these frequencies
    let column = r.radiation_column(3).unwrap();
    let (zero, inf) = (r.added_mass_limit(false, 2, column).unwrap(), r.added_mass_limit(true, 2, column).unwrap());
    assert!(zero.is_finite() && inf.is_finite() && zero > 0.0 && inf > 0.0, "{zero} {inf}");
    // as periods: omega = 0 is an infinite period
    p.waves.frequencies.set_input_type(gui_core::project::WaveInputType::Period);
    let q = round_trip(&p);
    assert!(q.waves.frequencies.zero_limit && q.waves.frequencies.infinite_limit);
}

#[test]
fn pressure_surfaces_with_several_modes_and_their_damping() {
    // a cylinder whose top is a pressure surface (an air chamber), two modes with stiffness
    let hull = quarter_cylinder(1.0, 0.0, -0.5, 8, 4, 4);
    let n = hull.len() as i64;
    // normals down, towards the water (the engine's rule for a pressure surface)
    let lid: Vec<Quad> =
        quarter_annulus(0.0001, 0.6, -0.2, 8).into_iter().map(|q| orient(q, [0.0, 0.0, -1.0])).collect();
    let nl = lid.len() as i64;
    let mut p = project_with(
        mesh("chamber", [hull, lid].concat()),
        [0.0, 0.0, -0.2],
        [0.6, 0.6, 0.7],
        vec![1.0, 2.0],
        heading(0.0),
    );
    p.bodies[0].mesh_range = Some([1, n]);
    p.bodies[0].pressure_surfaces.push(PanelSurface {
        first_panel: n + 1,
        last_panel: n + nl,
        pressure_modes: vec![
            PressureMode { shape: PressureShape::Uniform, stiffness: 100.0, damping: 5.0 },
            PressureMode { shape: PressureShape::SignX, stiffness: 0.0, damping: 0.0 },
        ],
        chamber_coupling: false,
        numeric_convention: true,
        ..PanelSurface::default()
    });
    p.first_order.motion = true;
    let q = round_trip(&p);
    let s = &q.bodies[0].pressure_surfaces[0];
    assert_eq!(s.pressure_modes.len(), 2);
    assert_eq!(s.pressure_modes[1].shape, PressureShape::SignX);
    assert!(!s.chamber_coupling && s.numeric_convention);
    let r = solve(&p);
    // 6 rigid + 2 pressure coordinates, labelled
    let coordinates = r.body_coordinates(0);
    assert_eq!(coordinates.len(), 8, "{coordinates:?}");
    assert_eq!(coordinates[6].1, "Pressure 1");
    assert!(r.has(QuantityId::PressureModeDamping));
}

#[test]
fn a_generalized_mode_with_its_matrices() {
    // a box barge bending in heave: w = x^2 on every record, with a stiffness
    let hull = quarter_box(1.0, 0.5, 0.5, (8, 4, 3));
    let mut p = project_with(mesh("barge", hull), [0.0, 0.0, -0.2], [0.4, 0.8, 0.8], vec![1.0, 2.0], heading(0.0));
    p.bodies[0].generalized_modes.push(GeneralizedMode {
        name: "Bending".into(),
        parity: 2,
        pieces: vec![ModePiece {
            range: None,
            more_ranges: vec![],
            terms: [vec![], vec![], vec![[1.0, 2.0, 0.0, 0.0]]],
        }],
        view_scale: 0.0,
    });
    // the displaced hull: w = x^2 at every corner, no u, v
    let shape = gui_core::engine_model::mode_shape(&p, 0, 0).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(shape.len(), p.bodies[0].mesh.as_ref().unwrap().displayed_panels());
    for (q, d) in &shape {
        for (x, u) in q.iter().zip(d) {
            assert!(u[0].abs() < 1e-9 && u[1].abs() < 1e-9 && (u[2] - x[0] * x[0]).abs() < 1e-9, "{x:?} {u:?}");
        }
    }
    let mut m = ModeMatrices::default();
    m.resize(7);
    m.mass[6 * 7 + 6] = 100.0;
    m.stiffness[6 * 7 + 6] = 5000.0;
    p.bodies[0].mode_matrices = Some(m);
    p.first_order.motion = true;
    let q = round_trip(&p);
    assert_eq!(q.bodies[0].generalized_modes[0].pieces[0].terms[2], vec![[1.0, 2.0, 0.0, 0.0]]);
    assert_eq!(q.bodies[0].mode_matrices.as_ref().unwrap().stiffness[48], 5000.0);
    let r = solve(&p);
    let coordinates = r.body_coordinates(0);
    assert_eq!(coordinates.len(), 7);
    assert_eq!(coordinates[6].1, "Generalized 1");
    // its motion is solved, finite
    let xi = r.fhd_series(QuantityId::Motion, 0, coordinates[6].0);
    assert!(xi.iter().all(|z| z.norm().is_finite()));
}

#[test]
fn tank_sheets_interior_mesh_origins_lids_and_a_control_polygon() {
    // the tank barge: a baffle (dipole) and a porous screen (damper) inside its tank
    let hull = quarter_box(1.0, 0.5, 1.0, (8, 4, 4));
    let walls: Vec<Quad> = quarter_box(0.8, 0.4, 0.5, (8, 4, 3))
        .into_iter()
        .map(|q| [q[3], q[2], q[1], q[0]].map(|v| [v[0], v[1], v[2] - 0.1]))
        .collect();
    // a transverse baffle at x = 0.4 inside the tank, and a screen at x = 0.6
    let baffle = |x: f64| -> Quad { [[x, 0.0, -0.55], [x, 0.3, -0.55], [x, 0.3, -0.3], [x, 0.0, -0.3]] };
    let (nh, nw) = (hull.len() as i64, walls.len() as i64);
    let mut p = project_with(
        mesh("tank_vessel", [hull, walls, vec![baffle(0.4), baffle(0.6)]].concat()),
        [0.0, 0.0, -0.2],
        [0.4, 0.8, 0.8],
        vec![1.0],
        heading(0.0),
    );
    let b = &mut p.bodies[0];
    b.mesh_range = Some([1, nh]);
    b.tanks.push(Tank {
        first_panel: nh + 1,
        last_panel: nh + nw,
        sheets: vec![
            TankSheet { first_panel: nh + nw + 1, last_panel: nh + nw + 1, ..TankSheet::default() },
            TankSheet {
                first_panel: nh + nw + 2,
                last_panel: nh + nw + 2,
                more_ranges: vec![],
                damper: true,
                damping_factor: 0.3,
            },
        ],
        ..Tank::default()
    });
    b.additional_damping =
        Some(Matrix6 { values: (0..36).map(|k| if k % 7 == 0 { 10.0 } else { 0.0 }).collect(), edit_in_table: false });
    b.damping_origin = MatrixOrigin::CenterOfGravity;
    let q = round_trip(&p);
    assert_eq!(q.bodies[0].tanks[0].sheets.len(), 2);
    assert!(q.bodies[0].tanks[0].sheets[1].damper);
    assert_eq!(q.bodies[0].damping_origin, MatrixOrigin::CenterOfGravity);
    let r = solve(&p);
    assert!(r.has(QuantityId::AddedMass));

    // a cylinder whose IRR is a mesh of the library, with a lid placed with it at x = 10
    let cylinder = quarter_cylinder(1.0, 0.0, -0.5, 8, 4, 4);
    let mut p = project_with(mesh("cylinder", cylinder), [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![1.0], heading(0.0));
    let disc = mesh("disc", quarter_annulus(0.0001, 0.99, 0.0, 8));
    p.add_mesh("disc", disc.clone());
    p.bodies[0].interior.method = InteriorMethod::Mesh;
    p.bodies[0].interior.mesh = Some(disc);
    p.bodies[0].position = [10.0, 0.0, 0.0];
    p.lids.push(DampingLid { mesh: None, ..DampingLid::default() });
    // a control surface with a polygon boundary and a partition (control-surface drift, no tanks)
    p.bodies[0].control_surface = Some(ControlSurface {
        method: ControlMethod::Automatic,
        depth: 1.5,
        panel_size: 0.5,
        boundary: Some(vec![[2.5, 0.0], [2.5, 2.5], [0.0, 2.5]]),
        partitions: vec![vec![[1.0, 1.0], [2.5, 2.5]]],
        ..ControlSurface::default()
    });
    p.second_order.drift_control_surface = true;
    p.first_order.motion = true;
    let q = round_trip(&p);
    assert_eq!(q.bodies[0].interior.method, InteriorMethod::Mesh);
    let c = q.bodies[0].control_surface.as_ref().unwrap();
    assert_eq!(c.boundary.as_ref().map(Vec::len), Some(3));
    assert_eq!(c.partitions, vec![vec![[1.0, 1.0], [2.5, 2.5]]]);
    let r = solve(&p);
    assert!(r.has(QuantityId::DriftControlSurface));
    let _ = Body::default();
}

#[test]
fn a_lid_placed_with_its_body() {
    // the OWC chamber of examples/floating_owc_chamber with a damping lid on its chamber: records
    // of the body's mesh, placed with the body; the body moved to x = 10
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let (mut p, _) = gui_core::files::load_project(&root.join("floating_owc_chamber_lid.hbm")).unwrap();
    let name = p.bodies[0].name.clone();
    assert!(p.lids[0].mesh.is_none() && p.lids[0].placed_with.as_deref() == Some(name.as_str()));
    let range = p.lids[0].range;
    assert!(range[0] > 1 && range[1] >= range[0], "{range:?}");
    p.bodies[0].position = [10.0, 0.0, 0.0];
    p.waves.frequencies.inputs.truncate(2);
    p.waves.frequencies.count = 2;
    p.waves.frequencies.edit_in_table = true;
    let q = round_trip(&p);
    assert_eq!(q.lids[0].placed_with.as_deref(), Some(name.as_str()));
    assert_eq!(q.lids[0].range, range);
    let placed = solve(&p);
    assert!(placed.has(QuantityId::LidsElevationDiffraction));
    // the lid points follow the body: the chamber is r < 2 m
    let points = placed.surface_points(QuantityId::Lids, QuantityId::LidsIds);
    assert!(points.iter().all(|(_, x)| (x[0] - 10.0).abs() < 2.0), "{:?}", &points[..2]);
}

#[test]
fn field_points_at_several_depths_and_in_a_tank() {
    let cylinder = quarter_cylinder(1.0, 0.0, -0.5, 8, 4, 4);
    let mut p = project_with(mesh("cylinder", cylinder), [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![1.0], heading(0.0));
    p.field_points = FieldPoints {
        enabled: true,
        x_start: 2.0,
        x_end: 3.0,
        x_count: 3,
        y_start: 0.0,
        y_end: 1.0,
        y_count: 2,
        z: 0.0,
        extra_levels: vec![-0.5, -1.0],
        tank: None,
        list: None,
        ..FieldPoints::default()
    };
    let q = round_trip(&p);
    assert_eq!(q.field_points.extra_levels, vec![-0.5, -1.0]);
    let r = solve(&p);
    assert_eq!(r.field_points.len(), 18);
    // deep water: the incident p/(rho g) decays as e^(k z); at z = 0 it is the elevation, 1
    let a = r.field_amplitude(gui_core::results::FieldKind::Incident, 0, 0, None);
    let k = 1.0 / p.environment.gravity;
    for (pt, v) in r.field_points.iter().zip(&a) {
        assert!((v - (k * pt[2]).exp()).abs() < 1e-3, "{pt:?}: {v}");
    }

    // a grid in the fluid of a tank
    let hull = quarter_box(1.0, 0.5, 1.0, (8, 4, 4));
    let walls: Vec<Quad> = quarter_box(0.8, 0.4, 0.5, (8, 4, 3))
        .into_iter()
        .map(|q| [q[3], q[2], q[1], q[0]].map(|v| [v[0], v[1], v[2] - 0.1]))
        .collect();
    let (nh, nw) = (hull.len() as i64, walls.len() as i64);
    let mut p = project_with(
        mesh("tank_vessel", [hull, walls].concat()),
        [0.0, 0.0, -0.2],
        [0.4, 0.8, 0.8],
        vec![1.0],
        heading(0.0),
    );
    p.bodies[0].mesh_range = Some([1, nh]);
    p.bodies[0].tanks.push(Tank { first_panel: nh + 1, last_panel: nh + nw, ..Tank::default() });
    p.field_points = FieldPoints {
        enabled: true,
        x_start: 0.1,
        x_end: 0.5,
        x_count: 3,
        y_start: 0.1,
        y_end: 0.3,
        y_count: 2,
        z: -0.3,
        extra_levels: vec![],
        tank: Some(TankRef { body: p.bodies[0].name.clone(), tank: 1 }),
        list: None,
        ..FieldPoints::default()
    };
    let q = round_trip(&p);
    assert_eq!(q.field_points.tank, p.field_points.tank);
    // written to the tank, in its body's axes; none in the sea
    let built = build_model(&p, &BuildOptions::default()).unwrap_or_else(|e| panic!("{}", e.message()));
    let t = built.model.object(&format!("bodies/{}/tanks/tank_1", p.bodies[0].name)).unwrap();
    let local = built.model.get(t, "field_points").unwrap().and_then(|v| v.as_reals().map(<[f64]>::to_vec));
    assert_eq!(local.as_ref().map(Vec::len), Some(6 * 3), "{local:?}");
    let first_order = built.model.object("first_order").unwrap();
    assert!(!built.model.item_info(first_order, "field_points").unwrap().is_set);
    let r = solve(&p);
    assert_eq!(r.field_points.len(), 6);
    assert!(r.has(QuantityId::TanksPressureRadiation));
    // the tank's radiation at its points (the fluid of the tank moves with the body), and no waves
    let radiation = r.field_amplitude(gui_core::results::FieldKind::Radiation, 0, 0, None);
    assert_eq!(radiation.len(), 6);
    assert!(radiation.iter().all(|v| v.is_finite()) && radiation.iter().any(|v| *v > 0.0), "{radiation:?}");
    let incident = r.field_amplitude(gui_core::results::FieldKind::Incident, 0, 0, None);
    assert!(incident.iter().all(|v| *v == 0.0), "{incident:?}");
    // points one by one: two in the tank, two outside at another depth each
    let tank = Some(TankRef { body: p.bodies[0].name.clone(), tank: 1 });
    let listed = |point: V3, tank: &Option<TankRef>| gui_core::project::ListedPoint { point, tank: tank.clone() };
    p.field_points.list = Some(vec![
        listed([0.2, 0.1, -0.3], &tank),
        listed([0.4, 0.2, -0.35], &tank),
        listed([3.0, 0.0, 0.0], &None),
        listed([3.0, 1.0, -0.5], &None),
    ]);
    let q = round_trip(&p);
    assert_eq!(q.field_points.list, p.field_points.list);
    let r = solve(&p);
    assert_eq!(r.field_points.len(), 4);
    // the results of each point where it is: the two in the sea first in the engine's, then the tank's
    assert_eq!(
        r.field_sources,
        vec![
            Some(PointSource::Tank { body: 0, tank: 0, index: 0 }),
            Some(PointSource::Tank { body: 0, tank: 0, index: 1 }),
            Some(PointSource::Sea(0)),
            Some(PointSource::Sea(1)),
        ]
    );
    // a model written elsewhere with such points opens as the list, in the model's order: the
    // points in the sea, then each tank's
    let built = build_model(&p, &BuildOptions { gui_state: false, ..BuildOptions::default() }).unwrap();
    let (q, warnings) = read_model(&built.model).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let listed = p.field_points.list.clone().unwrap();
    let reopened = q.field_points.list.clone().expect("a list");
    assert_eq!(reopened.len(), 4);
    for (a, b) in reopened.iter().zip(listed[2..].iter().chain(&listed[..2])) {
        assert_eq!(a.tank, b.tank);
        assert!((0..3).all(|i| (a.point[i] - b.point[i]).abs() < 1e-12), "{a:?} {b:?}");
    }
}

#[test]
fn surfaces_and_wetted_surfaces_of_several_record_ranges() {
    // the tank vessel: tank walls in two ranges, a dipole of two records apart, and records of the
    // hull no role takes
    let hull = quarter_box(1.0, 0.5, 1.0, (8, 4, 4));
    let walls: Vec<Quad> = quarter_box(0.8, 0.4, 0.5, (8, 4, 3))
        .into_iter()
        .map(|q| [q[3], q[2], q[1], q[0]].map(|v| [v[0], v[1], v[2] - 0.1]))
        .collect();
    let (nh, nw) = (hull.len() as i64, walls.len() as i64);
    let mut p = project_with(
        mesh("tank_vessel", [hull, walls].concat()),
        [0.0, 0.0, -0.2],
        [0.4, 0.8, 0.8],
        vec![1.0],
        heading(0.0),
    );
    let b = &mut p.bodies[0];
    b.mesh_range = Some([1, nh]);
    b.mesh_gaps = vec![[5, 6]];
    b.tanks.push(Tank {
        first_panel: nh + 1,
        last_panel: nh + 10,
        more_ranges: vec![[nh + 11, nh + nw]],
        ..Tank::default()
    });
    b.dipoles.push(PanelSurface {
        first_panel: 2,
        last_panel: 2,
        more_ranges: vec![[9, 9]],
        ..PanelSurface::default()
    });
    let built = build_model(&p, &BuildOptions::default()).unwrap_or_else(|e| panic!("{}", e.message()));
    let body = built.model.object(&format!("bodies/{}", p.bodies[0].name)).unwrap();
    let wet = built.model.get(body, "ranges").unwrap().and_then(|v| v.as_ranges());
    assert_eq!(wet, Some(vec![[1, 1], [3, 4], [7, 8], [10, nh]]));
    let q = round_trip(&p);
    assert_eq!(q.bodies[0].mesh_gaps, [[5, 6]]);
    assert_eq!(q.bodies[0].dipoles[0].more_ranges, [[9, 9]]);
    assert!(role_count(&p, PanelRole::Dipole) == 2 * 4, "two dipole records in four copies");
}

#[test]
fn a_higher_order_cylinder_round_trips_and_is_drawn_smooth() {
    // the analytic cylinder R = 1 m, draft 1 m of examples/truncated_cylinder_finite_depth (two
    // patches, its run_higher_order.py): the patch mesh round-trips, is
    // drawn by its tessellation and solves by the higher-order method (its accuracy is HydroBEM's
    // to test)
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let (patches, warnings) = gui_core::files::load_project(&root.join("truncated_cylinder_patches.hbm")).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let mesh = patches.bodies[0].mesh.clone().unwrap();
    assert!(mesh.is_patch());
    assert_eq!(mesh.panels.len(), 2);
    assert_eq!(mesh.discretization().len(), 2);
    let volume = patches.bodies[0].displaced_volume;
    assert!((volume - PI).abs() < 1e-3 * PI, "{volume}");
    round_trip(&patches);
    // the engine's elements: two patches in four copies; drawn by many quadrilaterals per patch,
    // shaded by the surface normals, outlined at the patch boundaries only
    assert_eq!(role_count(&patches, PanelRole::Wetted), 4 * 2);
    let roles = role_panels(&patches).unwrap();
    let smooth = &roles.smooth[&0];
    assert!(smooth.len() > 4 * 2 * 20 && smooth.iter().all(|(r, _)| *r == PanelRole::Wetted));
    let sides = smooth.len() * 4;
    let outlined: usize = smooth.iter().map(|(_, q)| q.edges.iter().filter(|e| **e).count()).sum();
    assert!(outlined > 0 && outlined * 5 < sides, "{outlined} of {sides} sides outlined");
    for (_, q) in smooth {
        for (v, n) in q.corners.iter().zip(&q.normals) {
            // the side wall's normal is radial, the bottom's vertical
            let r = (v[0] * v[0] + v[1] * v[1]).sqrt();
            if v[2] > -0.99 && (r - 1.0).abs() < 1e-6 {
                assert!(n[2].abs() < 1e-6 && ((n[0] * v[0] + n[1] * v[1]) / r).abs() > 0.999, "{v:?} {n:?}");
            }
        }
    }
    let a = solve(&patches);
    assert!(a.coefficient_series(QuantityId::AddedMass, 2, 2).iter().all(|v| v.is_finite()));
}

#[test]
fn field_points_give_velocities_without_the_free_surface_amplitude() {
    let cylinder = quarter_cylinder(1.0, 0.0, -0.5, 8, 4, 4);
    let mut p = project_with(mesh("cylinder", cylinder), [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![1.0], heading(0.0));
    p.field_points = FieldPoints {
        enabled: true,
        x_start: 2.0,
        x_end: 3.0,
        x_count: 2,
        y_start: 0.0,
        y_end: 0.0,
        y_count: 1,
        z: 0.0,
        ..FieldPoints::default()
    };
    p.first_order.field_pressure = false;
    p.first_order.field_velocity = true;
    let q = round_trip(&p);
    assert!(!q.first_order.field_pressure && q.first_order.field_velocity);
    let r = solve(&p);
    assert!(!r.has_field_results(), "no free-surface amplitude");
    assert!(r.has(QuantityId::FieldPointsVelocityIncident));
    // the default writes it
    p.first_order.field_pressure = true;
    assert!(round_trip(&p).first_order.field_pressure);
    assert!(solve(&p).has_field_results());
}

#[test]
fn polar_and_body_fitted_grids_ring_the_body_and_survive_saving() {
    use gui_core::field_layout::GridLayout;
    // a cylinder of radius 1 through the free surface
    let cylinder = quarter_cylinder(1.0, 0.0, -0.5, 8, 4, 4);
    let mut p = project_with(mesh("cylinder", cylinder), [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![1.0], heading(0.0));
    p.field_points = FieldPoints {
        enabled: true,
        layout: GridLayout::Polar,
        radial_start: 1.2,
        radial_end: 3.0,
        radial_count: 4,
        angle_count: 16,
        ..FieldPoints::default()
    };
    let q = round_trip(&p);
    assert_eq!(q.field_points.layout, GridLayout::Polar);
    assert_eq!((q.field_points.radial_count, q.field_points.angle_count), (4, 16));
    assert!(q.field_points.list.is_none(), "the rings, not a list");
    let r = solve(&p);
    assert!(r.has_field_results());
    assert_eq!(r.field_points.len(), 64, "every point outside the cylinder");

    // rings 0.1 to 1 m outside the section: the first ring 0.1 m from the hull all round
    p.field_points.layout = GridLayout::BodyFitted;
    (p.field_points.radial_start, p.field_points.radial_end, p.field_points.radial_count) = (0.1, 1.0, 3);
    p.field_points.angle_count = 24;
    p.update_field_profile();
    let profile = p.field_points.profile.clone().expect("the cylinder's section at z = 0");
    assert!(profile.radii.iter().all(|r| *r > 0.97 && *r < 1.0 + 1e-9), "the octagon-per-quarter section");
    let first = p.field_points.plan_at(0.0, 5.0).unwrap();
    let r0 = first[0].hypot(first[1]);
    assert!(r0 > 1.07 && r0 < 1.1 + 1e-9, "{r0}");
    let q = round_trip(&p);
    assert_eq!(q.field_points.layout, GridLayout::BodyFitted);
    assert_eq!(q.field_points.profile, Some(profile));
    let r = solve(&p);
    assert_eq!(r.field_points.len(), 72);
    assert!(r.has_field_results());
}

#[test]
fn an_analytic_mesh_made_in_the_gui_is_solved_and_reopens_editable() {
    use gui_core::analytic::{AnalyticKind, AnalyticSpec};
    let spec = AnalyticSpec { dimensions: vec![1.0, 0.5], ..AnalyticSpec::of(AnalyticKind::Cylinder) };
    let cylinder = spec.build(None).expect("the engine's cylinder");
    let mut p = project_with(cylinder.clone(), [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![1.0], heading(0.0));
    p.add_mesh("cylinder", cylinder.clone());
    // a new body keeps Detect, which takes the lid patch (the third, in z = 0)
    let previous = None;
    p.bodies[0].follow_analytic_lid(previous, &cylinder);
    assert_eq!(p.bodies[0].interior.method, InteriorMethod::Detect);
    assert_eq!(p.bodies[0].wetted_analytic_lid(), None);
    // a body without an interior surface takes the lid as its Panel Range
    let mut none = p.bodies[0].clone();
    none.interior.method = InteriorMethod::None;
    none.follow_analytic_lid(None, &cylinder);
    let i = &none.interior;
    assert_eq!((i.method, i.first_panel, i.last_panel), (InteriorMethod::PanelRange, 3, 3));
    assert_eq!(none.wetted_analytic_lid(), None);
    // IRR off with the lid in the mesh: the lid would be hull (the Details warn)
    let mut off = p.bodies[0].clone();
    off.interior.method = InteriorMethod::None;
    assert_eq!(off.wetted_analytic_lid(), Some(3));
    off.mesh_range = Some([1, 2]);
    assert_eq!(off.wetted_analytic_lid(), None, "a panel range without the lid");
    let q = round_trip(&p);
    assert_eq!(q.meshes[0].mesh.analytic.as_ref(), Some(&spec), "the shape reopens editable");
    let r = solve(&p);
    assert!(r.has(QuantityId::AddedMass));
    // about pi R^2 T = 1.571 m^3 displaced
    let v = r.hydrostatics[0].volume[0];
    assert!((v - PI * 0.5).abs() < 0.01 * PI * 0.5, "{v}");

    // without the lid Detect stays (it finds none); a wider cylinder reaches its users
    let wider = AnalyticSpec { dimensions: vec![1.5, 0.5], lid: false, ..spec.clone() };
    p.replace_mesh(0, wider.rebuild(&p.meshes[0].mesh).unwrap());
    assert_eq!(p.bodies[0].interior.method, InteriorMethod::Detect);
    assert_eq!(p.bodies[0].mesh.as_ref().and_then(|m| m.analytic.clone()), Some(wider));
    assert!(p.bodies[0].displaced_volume > 1.5 * PI * 0.5);
}

#[test]
fn higher_order_body_outputs_colour_the_patches_of_their_cells() {
    use gui_core::analytic::{AnalyticKind, AnalyticSpec};
    let spec = AnalyticSpec { dimensions: vec![1.0, 0.5], lid: false, ..AnalyticSpec::of(AnalyticKind::Cylinder) };
    let cylinder = spec.build(None).expect("the engine's cylinder");
    let mut p = project_with(cylinder, [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![1.0], heading(0.0));
    p.first_order.body_pressure = true;
    let r = solve(&p);
    let points = r.body_output_points();
    assert!(!points.is_empty());
    assert!(!r.surface.is_empty(), "the tessellated patches");
    // every point colours the faces of its cell, every face goes to one point, near it
    let faces = r.body_point_faces();
    assert_eq!(faces.len(), points.len());
    assert!(faces.iter().all(|f| !f.is_empty()), "every cell has faces");
    assert_eq!(faces.iter().map(Vec::len).sum::<usize>(), r.surface.len());
    // (the points are the centres of uniform cells in the patch parameters: the nearest)
    let distance = |a: V3, b: V3| (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt();
    for (k, f) in faces.iter().enumerate() {
        for q in f {
            let c: V3 = std::array::from_fn(|i| q.iter().map(|v| v[i]).sum::<f64>() / 4.0);
            let nearest = points.iter().map(|(_, p)| distance(*p, c)).fold(f64::INFINITY, f64::min);
            assert!(distance(points[k].1, c) <= nearest + 1e-12, "a face not at its nearest point");
        }
    }
    // the faces cover the wetted surface: 2 pi R T + pi R^2
    let area = |q: &[V3; 4]| {
        let d1: V3 = std::array::from_fn(|i| q[2][i] - q[0][i]);
        let d2: V3 = std::array::from_fn(|i| q[3][i] - q[1][i]);
        let n = [d1[1] * d2[2] - d1[2] * d2[1], d1[2] * d2[0] - d1[0] * d2[2], d1[0] * d2[1] - d1[1] * d2[0]];
        0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
    };
    let total: f64 = r.surface.iter().map(|q| area(&q.quad)).sum();
    let exact = 2.0 * PI * 0.5 + PI;
    assert!((total - exact).abs() < 0.01 * exact, "{total} vs {exact}");
}

#[test]
fn the_spans_of_a_patch_mesh_lie_on_its_surface() {
    // the analytic cylinder R = 1 m, draft 1 m: two patches (side, bottom), a quarter in symmetry
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let (project, _) = gui_core::files::load_project(&root.join("truncated_cylinder_patches.hbm")).unwrap();
    let mesh = project.bodies[0].mesh.clone().unwrap();
    let rows = mesh.discretization();
    let lines = mesh.span_lines().unwrap();
    // per patch nu - 1 curves across v and nv - 1 across u, each of 4 x (spans crossed) segments
    // (8 to 64), in every symmetry copy
    let segments = |across: i64| (4 * across).clamp(8, 64) as usize;
    let per_copy: usize = rows
        .iter()
        .map(|r| (r[0] - 1).max(0) as usize * segments(r[1]) + (r[1] - 1).max(0) as usize * segments(r[0]))
        .sum();
    assert!(per_copy > 0, "{rows:?}");
    assert_eq!(lines.len(), per_copy * mesh.displayed_panels() / mesh.panels.len());
    // every point on the hull: the side at r = 1, the bottom at z = -1 within r <= 1
    for p in lines.iter().flatten() {
        let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
        let side = (r - 1.0).abs() < 1e-6 && p[2] <= 1e-9 && p[2] >= -1.0 - 1e-9;
        let bottom = (p[2] + 1.0).abs() < 1e-6 && r <= 1.0 + 1e-6;
        assert!(side || bottom, "{p:?}");
    }
    // finer spans give more lines; a low-order mesh has none
    let finer = mesh.with_discretization(0, 0, rows[0][0] + 2).unwrap();
    assert!(finer.span_lines().unwrap().len() > lines.len());
    let (panels, _) = gui_core::files::load_project(&root.join("cylinder.hbm")).unwrap();
    assert!(panels.bodies[0].mesh.as_ref().unwrap().span_lines().unwrap().is_empty());
}

#[test]
fn a_wave_amplitude_scales_what_the_waves_drive() {
    // the cylinder of examples/cylinder: RAO, field points; with the pressure drift
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let (mut p, _) = gui_core::files::load_project(&root.join("cylinder.hbm")).unwrap();
    p.second_order.drift_pressure_integration = true;
    let unit = solve(&p);
    assert_eq!(unit.wave_amplitude, 1.0);
    let a = 2.5;
    let r = unit.with_wave_amplitude(a);
    let close = |x: f64, y: f64| (x - y).abs() <= 1e-9 * y.abs().max(1.0);
    // wave loads and motions: A times
    for q in [QuantityId::ExcitationDiffraction, QuantityId::ExcitationFroudeKrylov, QuantityId::Motion] {
        for (x, y) in r.fhd_series(q, 0, 0).iter().zip(unit.fhd_series(q, 0, 0)) {
            assert!(close(x.norm(), a * y.norm()), "{q:?}");
        }
    }
    // the mean drift: A² times
    for (x, y) in r.drift_series(QuantityId::DriftPressureIntegration, 0, 0).iter().zip(unit.drift_series(
        QuantityId::DriftPressureIntegration,
        0,
        0,
    )) {
        assert!(close(*x, a * a * y), "drift {x} {y}");
    }
    // added mass and the radiation per unit motion: as they are
    assert_eq!(r.coefficient_series(QuantityId::AddedMass, 0, 0), unit.coefficient_series(QuantityId::AddedMass, 0, 0));
    use gui_core::results::FieldKind;
    assert_eq!(r.field_complex(FieldKind::Radiation, 1, 0, None), unit.field_complex(FieldKind::Radiation, 1, 0, None));
    // the free surface: incident, excitation and the Total (incident + scattering + radiation x
    // motion, the motions scaled) A times
    for kind in [FieldKind::Incident, FieldKind::Excitation, FieldKind::Total] {
        let (x, y) = (r.field_complex(kind, 1, 0, None), unit.field_complex(kind, 1, 0, None));
        assert!(!y.is_empty());
        for (x, y) in x.iter().zip(&y) {
            assert!((x - y * a).norm() <= 1e-9 * y.norm().max(1.0), "{kind:?}");
        }
    }
    // the units: per metre of wave amplitude at 1 m, of the waves otherwise
    assert_eq!(unit.wave_unit("m/m"), "m/m");
    assert_eq!(r.wave_unit("m/m"), "m");
    assert_eq!(r.wave_unit("rad/m"), "rad");
    assert_eq!(r.wave_unit("N·m/m²"), "N·m");
    assert_eq!(r.wave_unit("m³/s per m per rad"), "m³/s per rad");
}

#[test]
fn a_patch_mesh_the_engine_cannot_build_still_shows_its_surfaces() {
    // the analytic cylinder's two patches (side, bottom); the bottom a pressure surface, drawn by
    // the GUI as when the engine refuses the problem: pink, and not also as hull
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let (mut p, _) = gui_core::files::load_project(&root.join("truncated_cylinder_patches.hbm")).unwrap();
    let draw = |p: &Project| {
        gui_core::scene::build_scene(
            p,
            &gui_core::scene::SceneInputs {
                focus: gui_core::scene::Focus::All,
                engine: None,
                overlay: None,
                smoothing: true,
                markers: None,
                body_overlay: None,
                free_surface: false,
                normals: false,
                spans: true,
                mode_shape: None,
                animation: None,
            },
        )
    };
    let has_pressure = |s: &gui_core::scene::Scene| s.legend.iter().any(|(n, _)| *n == "Pressure Surface");
    let hull = draw(&p);
    assert!(!has_pressure(&hull));
    p.bodies[0].pressure_surfaces.push(PanelSurface {
        first_panel: 2,
        last_panel: 2,
        pressure_modes: vec![PressureMode::default()],
        ..PanelSurface::default()
    });
    let with = draw(&p);
    assert!(has_pressure(&with), "{:?}", with.legend);
    // the bottom is drawn pink in place of the hull's colour
    let pink = gui_core::scene::PRESSURE.map(|c| f32::from(c) / 255.0);
    let colored =
        |s: &gui_core::scene::Scene| s.opaque.iter().chain(&s.transparent).filter(|v| v.color[..3] == pink).count();
    let count = |s: &gui_core::scene::Scene| s.opaque.len() + s.transparent.len();
    assert!(colored(&with) > 0 && colored(&hull) == 0);
    assert!(count(&with) - colored(&with) < count(&hull), "the bottom also drawn as hull");
}

#[test]
fn a_mesh_shown_alone_can_show_its_normals() {
    // the analytic cylinder R = 1 m, draft 1 m: two patches (side, bottom), symmetric about
    // x = 0 and y = 0: eight arrows, each three strokes; the bottom's point down, into the water
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let (p, _) = gui_core::files::load_project(&root.join("truncated_cylinder_patches.hbm")).unwrap();
    let draw = |normals: bool| {
        gui_core::scene::build_scene(
            &p,
            &gui_core::scene::SceneInputs {
                focus: gui_core::scene::Focus::Mesh(0),
                engine: None,
                overlay: None,
                smoothing: true,
                markers: None,
                body_overlay: None,
                free_surface: false,
                normals,
                spans: true,
                mode_shape: None,
                animation: None,
            },
        )
    };
    let (off, on) = (draw(false), draw(true));
    let red = gui_core::scene::NORMAL.map(|c| f32::from(c) / 255.0);
    let arrows: Vec<&[gui_core::scene::LineVertex]> = on.edges.chunks(2).filter(|l| l[0].color[..3] == red).collect();
    assert_eq!(arrows.len(), 8 * 3);
    assert_eq!(on.edges.len() - off.edges.len(), 8 * 3 * 2);
    assert!(on.legend.iter().any(|(n, _)| *n == "Normals") && !off.legend.iter().any(|(n, _)| *n == "Normals"));
    // the shafts (every third stroke) of the bottom: from z = -1 downwards; of the side: level
    let shafts: Vec<_> = arrows.iter().step_by(3).collect();
    let bottom: Vec<_> = shafts.iter().filter(|l| (l[0].position[2] + 1.0).abs() < 1e-4).collect();
    assert_eq!(bottom.len(), 4);
    assert!(bottom.iter().all(|l| l[1].position[2] < l[0].position[2]), "the bottom's normals point up");
}

#[test]
fn a_patch_mesh_takes_its_discretization_from_a_panel_size() {
    // one flat patch of 10 m x 4 m in a higher-order GDF file: at a panel size of 1 m the engine
    // gives floor(L / s) + 1 spans along each side (11 and 5), orders 3, Gauss 3 and 4
    let dir = std::env::temp_dir().join(format!("hydrobem-gui-panel-size-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plate.gdf");
    let gdf = |x: f64| format!("plate\n1.0 9.80665\n0 0\n1 0\n0 0 -2\n0 4 -2\n{x} 4 -2\n{x} 0 -2\n");
    std::fs::write(&path, gdf(10.0)).unwrap();
    let mesh = Mesh::load(&path).unwrap();
    assert!(mesh.is_patch() && mesh.rows_from == gui_core::mesh::RowsFrom::Default);
    // one patch of 4 x 4 spans of order 3: 6 x 6 coefficients
    assert_eq!(mesh.unknowns(), Some(36));
    let sized = mesh.with_panel_size(1.0, None).unwrap();
    let row = sized.discretization()[0];
    let mut spans = [row[0], row[1]];
    spans.sort();
    assert_eq!((spans, &row[2..]), ([5, 11], &[3, 3, 3, 4][..]));
    assert_eq!(sized.panels, mesh.panels);
    assert_eq!(sized.rows_from, gui_core::mesh::RowsFrom::PanelSize(1.0));
    // 11 x 5 spans of order 3: 13 x 7 coefficients
    assert_eq!(sized.unknowns(), Some(91));
    // an edit of the table forgets the size
    assert_eq!(sized.with_discretization(0, 0, 2).unwrap().rows_from, gui_core::mesh::RowsFrom::Edited);
    // refused: not a positive size, a file that changed, a mesh not read from such a file
    assert!(mesh.with_panel_size(0.0, None).is_err());
    std::fs::write(&path, gdf(12.0)).unwrap();
    assert!(mesh.with_panel_size(1.0, None).unwrap_err().contains("changed"));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let (panels, _) = gui_core::files::load_project(&root.join("cylinder.hbm")).unwrap();
    assert!(panels.meshes[0].mesh.with_panel_size(1.0, Some(&root)).is_err());
    // a relative source (a shipped model) is beside the project
    let (gdf_model, _) = gui_core::files::load_project(&root.join("truncated_cylinder_gdf.hbm")).unwrap();
    let mesh = &gdf_model.meshes[0].mesh;
    assert_eq!(mesh.rows_from, gui_core::mesh::RowsFrom::Project);
    assert!(std::path::Path::new(&mesh.source_file).is_relative());
    assert!(mesh.with_panel_size(2.0, None).is_err());
    let sized = mesh.with_panel_size(2.0, Some(&root)).unwrap();
    assert_eq!(sized.source_file, mesh.source_file);
    // and so does the object that is saved: the relative path, not the one the reader made
    let saved = sized.patch.as_ref().unwrap().object.get("source_file").and_then(|v| v.as_text());
    assert_eq!(saved, Some(mesh.source_file.as_str()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn normal_arrows_sit_at_the_middle_of_each_patch_and_of_each_copy() {
    let red = gui_core::scene::NORMAL.map(|c| f32::from(c) / 255.0);
    let draw = |p: &Project| {
        gui_core::scene::build_scene(
            p,
            &gui_core::scene::SceneInputs {
                focus: gui_core::scene::Focus::Mesh(0),
                engine: None,
                overlay: None,
                smoothing: true,
                markers: None,
                body_overlay: None,
                free_surface: false,
                normals: true,
                spans: true,
                mode_shape: None,
                animation: None,
            },
        )
    };
    // the shafts: every third stroke of the arrows, from the arrow's foot
    let shafts = |s: &gui_core::scene::Scene| -> Vec<[[f32; 3]; 2]> {
        let arrows: Vec<_> = s.edges.chunks(2).filter(|l| l[0].color[..3] == red).collect();
        arrows.iter().step_by(3).map(|l| [l[0].position, l[1].position]).collect()
    };
    // the cylinder's side (a curved patch, a quarter turn per copy): its arrow at mid-angle and
    // mid-depth of each quarter, not on an edge of the patch
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let (cyl, _) = gui_core::files::load_project(&root.join("truncated_cylinder_patches.hbm")).unwrap();
    let side: Vec<_> = shafts(&draw(&cyl)).into_iter().filter(|s| (s[0][2] + 1.0).abs() > 1e-3).collect();
    assert_eq!(side.len(), 4);
    for [foot, tip] in side {
        let angle = f64::from(foot[1].atan2(foot[0]).to_degrees()).rem_euclid(90.0);
        assert!((angle - 45.0).abs() < 5.0, "the arrow at {angle} deg within its quarter");
        assert!((f64::from(foot[2]) + 0.5).abs() < 0.05, "at depth {}", foot[2]);
        // outwards, into the water
        assert!(tip[0].hypot(tip[1]) > foot[0].hypot(foot[1]));
    }
    // one plate patch with symmetry about x = 0 and y = 0: an arrow for each of the four copies
    let dir = std::env::temp_dir().join(format!("hydrobem-gui-normals-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plate.gdf");
    std::fs::write(&path, "plate\n1.0 9.80665\n1 1\n1 0\n0 0 -2\n0 4 -2\n4 4 -2\n4 0 -2\n").unwrap();
    let mut plate = Project::default();
    plate.add_mesh("plate", Mesh::load(&path).unwrap());
    let arrows = shafts(&draw(&plate));
    assert_eq!(arrows.len(), 4, "{arrows:?}");
    // each at the middle of its copy: (±2, ±2)
    for [foot, _] in &arrows {
        assert!((foot[0].abs() - 2.0).abs() < 0.2 && (foot[1].abs() - 2.0).abs() < 0.2, "{foot:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_higher_order_gdf_without_spl_is_read_with_the_discretization_chosen() {
    use gui_core::mesh::{GdfDiscretization, RowsFrom, needs_discretization};
    let dir = std::env::temp_dir().join(format!("hydrobem-gui-choice-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plate.gdf");
    std::fs::write(&path, "plate\n1.0 9.80665\n0 0\n1 0\n0 0 -2\n0 4 -2\n10 4 -2\n10 0 -2\n").unwrap();
    // asked: a higher-order GDF without SPL file; not a low-order one, nor one with its SPL file
    assert!(needs_discretization(&path));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    assert!(!needs_discretization(&root.join("cylinder.gdf")));
    assert!(!needs_discretization(&root.join("truncated_cylinder_patches.gdf")));
    // spans per patch, with orders of its own
    let spans =
        GdfDiscretization { by_panel_size: false, nu: 2, nv: 3, order: 4, outer: 4, inner: 5, ..Default::default() };
    let mesh = Mesh::load_gdf_with(&path, &spans).unwrap();
    assert_eq!(mesh.discretization(), vec![[2, 3, 4, 4, 4, 5]]);
    assert_eq!(mesh.rows_from, RowsFrom::Spans(2, 3));
    // a panel size, with the orders chosen
    let sized = GdfDiscretization { panel_size: 2.0, order: 4, ..Default::default() };
    let mesh = Mesh::load_gdf_with(&path, &sized).unwrap();
    let row = mesh.discretization()[0];
    let mut n = [row[0], row[1]];
    n.sort();
    assert_eq!((n, row[2], row[3]), ([3, 6], 4, 4), "floor(4/2)+1, floor(10/2)+1");
    assert_eq!(mesh.rows_from, RowsFrom::PanelSize(2.0));
    assert!(Mesh::load_gdf_with(&path, &GdfDiscretization { panel_size: 0.0, ..Default::default() }).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn qtfs_of_a_floating_cylinder() {
    // the quadratic QTF by pressure integration, the force of the second-order potential and
    // Newman's approximation of a floating quarter cylinder at three frequencies
    let cylinder = quarter_cylinder(1.0, 0.0, -0.5, 8, 4, 4);
    let mut p =
        project_with(mesh("cylinder", cylinder), [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![0.8, 1.0, 1.2], heading(0.0));
    p.first_order.motion = true;
    let s = &mut p.second_order;
    s.drift_pressure_integration = true;
    s.qtf_quadratic_pressure_integration = true;
    s.qtf_potential_direct = true;
    s.qtf_newman = true;
    s.qtf_newman_average = gui_core::project::NewmanAverage::Arithmetic;
    s.free_surface_panel_size = Some(0.5);
    s.quadrature_annuli = 1;
    let q = round_trip(&p);
    assert_eq!(q.second_order, p.second_order);
    let r = solve(&p);
    assert_eq!(r.qtf_heading_pairs(), vec![[0.0, 0.0]]);
    let surge = r.coordinate_row(0, 0).expect("surge");
    for quantity in [
        QuantityId::QtfQuadraticPressureIntegrationDifference,
        QuantityId::QtfQuadraticPressureIntegrationSum,
        QuantityId::QtfPotentialDirectDifference,
        QuantityId::QtfPotentialDirectSum,
        QuantityId::QtfNewmanPressureIntegrationDifference,
    ] {
        let m = r.qtf_matrix(quantity, 0, surge);
        assert_eq!((m.len(), m.first().map(Vec::len)), (3, Some(3)), "{quantity:?}");
        assert!(m.iter().flatten().all(|z| z.re.is_finite() && z.im.is_finite()), "{quantity:?}");
    }
    // the GUI reads the [F, F, P, D] layout: the difference quadratic QTF of a frequency with itself
    // is the pressure drift read from its own [F, P, D] layout
    let qtf = r.qtf_matrix(QuantityId::QtfQuadraticPressureIntegrationDifference, 0, surge);
    let drift = r.drift_series(QuantityId::DriftPressureIntegration, 0, surge);
    for (a, d) in drift.iter().enumerate() {
        assert!((qtf[a][a].re - d).abs() <= 1e-6 * d.abs().max(1.0), "{} {d}", qtf[a][a]);
    }
    // Newman's diagonal is the drift too, and its off-diagonal the arithmetic mean of the two
    let newman = r.qtf_matrix(QuantityId::QtfNewmanPressureIntegrationDifference, 0, surge);
    assert!((newman[0][2].re - 0.5 * (drift[0] + drift[2])).abs() <= 1e-6 * drift[0].abs().max(1.0));
    // the total: the quadratic part plus the potential part, element by element; none without a part
    let potential = r.qtf_matrix(QuantityId::QtfPotentialDirectDifference, 0, surge);
    let total = r.qtf_sum_of(
        &[QuantityId::QtfQuadraticPressureIntegrationDifference, QuantityId::QtfPotentialDirectDifference],
        0,
        surge,
    );
    for a in 0..3 {
        for b in 0..3 {
            assert!((total[a][b] - (qtf[a][b] + potential[a][b])).norm() <= 1e-12 * total[a][b].norm().max(1.0));
        }
    }
    let missing = [QuantityId::QtfQuadraticControlSurfaceDifference, QuantityId::QtfPotentialDirectDifference];
    assert!(r.qtf_sum_of(&missing, 0, surge).is_empty());
}

#[test]
fn a_body_of_a_free_surface_plate_alone() {
    // a floating mat of zero draft: a 2 x 2 m plate in z = 0 (a quarter, normals down, into the
    // water), the body's only surface, heaving
    let n = 4;
    let mut quads = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (x0, x1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let (y0, y1) = (j as f64 / n as f64, (j + 1) as f64 / n as f64);
            quads.push(orient([[x0, y0, 0.0], [x1, y0, 0.0], [x1, y1, 0.0], [x0, y1, 0.0]], [0.0, 0.0, -1.0]));
        }
    }
    let plate = mesh("mat", quads);
    let mut p = Project::default();
    p.add_mesh("mat", plate.clone());
    let i = p.add_body();
    let b = &mut p.bodies[i];
    b.modes = [false, false, true, false, false, false];
    b.free_surface_plates.push(PanelSurface {
        source: SurfaceSource::ExternalMesh,
        mesh: Some(plate),
        ..PanelSurface::default()
    });
    p.first_order.diffraction = false;
    p.waves.frequencies = WaveFrequencies { start: 0.5, end: 2.0, count: 3, ..WaveFrequencies::default() };
    p.waves.frequencies.rebuild();
    let q = round_trip(&p);
    assert_eq!(q.bodies[0].free_surface_plates.len(), 1);
    assert_eq!(q.bodies[0].free_surface_plates[0].source, SurfaceSource::ExternalMesh);
    // the engine's panels of the plate role
    assert!(role_count(&p, PanelRole::FreeSurfacePlate) > 0, "the plate's panels in the viewport");
    let r = solve(&p);
    let heave = r.coordinate_row(0, 2).expect("heave");
    let column = r.radiation_column(heave + 1).expect("the heave radiation");
    let a33 = r.coefficient_series(QuantityId::AddedMass, heave, column);
    assert_eq!(a33.len(), 3);
    assert!(a33.iter().all(|v| v.is_finite() && *v > 0.0), "{a33:?}");
}

#[test]
fn the_haskind_excitation_is_read_by_its_radiation_columns() {
    // excitation.haskind is [F, H, K]: its last axis the radiated modes (here surge, heave and
    // pitch), read for a coordinate row through its radiation column
    let cylinder = quarter_cylinder(1.0, 0.0, -0.5, 8, 4, 4);
    let mut p =
        project_with(mesh("cylinder", cylinder), [0.0, 0.0, -0.2], [0.6, 0.6, 0.7], vec![1.0, 1.5], heading(0.0));
    p.bodies[0].modes = [true, false, true, false, true, false];
    let r = solve(&p);
    assert_eq!(r.dims(QuantityId::ExcitationHaskind), Some(vec![2, 1, 3]));
    for row in [0, 2, 4] {
        let haskind = r.haskind_series(0, row);
        let diffraction = r.fhd_series(QuantityId::ExcitationDiffraction, 0, row);
        assert_eq!(haskind.len(), 2, "row {row}");
        // the same force by two methods: the right column lands on the right row (surge, heave and
        // pitch differ by far more than the methods do)
        for (h, d) in haskind.iter().zip(&diffraction) {
            assert!((h - d).norm() <= 0.1 * d.norm(), "row {row}: Haskind {h}, diffraction {d}");
        }
    }
    // a coordinate not radiated has no Haskind force
    assert!(r.haskind_series(0, 1).is_empty());
}

#[test]
fn generated_and_tank_plates_round_trip_and_follow_their_body() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    // examples/trimmed_moonpool_cover: the plate the engine generates in the trimmed moonpool;
    // moved, it round-trips with its point and with the body whose waterline it covers
    let (mut p, _) = gui_core::files::load_project(&root.join("trimmed_moonpool_cover.hbm")).unwrap();
    one_frequency(&mut p);
    p.bodies[0].free_surface_plates[0].inside = [0.25, -0.5];
    let q = round_trip(&p);
    let plate = &q.bodies[0].free_surface_plates[0];
    assert_eq!((plate.source, plate.inside, plate.body.as_deref()), (SurfaceSource::Generated, [0.25, -0.5], None));
    let solved = solve(&p);
    assert!(solved.has(QuantityId::AddedMass));
    // a second body with a plate over the first one's waterline: renamed and deleted with it
    let hull = p.bodies[0].name.clone();
    let i = p.add_body();
    p.bodies[i].free_surface_plates.push(PanelSurface {
        source: SurfaceSource::Generated,
        body: Some(hull.clone()),
        ..PanelSurface::default()
    });
    p.rename_body(&hull, "tube");
    p.bodies[0].name = "tube".into();
    assert_eq!(p.bodies[i].free_surface_plates[0].body.as_deref(), Some("tube"));
    p.forget_body("tube");
    assert_eq!(p.bodies[i].free_surface_plates[0].body, None);

    // examples/fish_tank_cylinders/run_covered_tank.py: the tank's plate, a mesh of its own, and
    // the same plate as records of the body's mesh
    let (mut p, _) = gui_core::files::load_project(&root.join("fish_tank_covered.hbm")).unwrap();
    one_frequency(&mut p);
    let q = round_trip(&p);
    assert_eq!(q.bodies[0].tanks[0].free_surface_plates.len(), 1);
    assert!(role_count(&p, PanelRole::FreeSurfacePlate) > 0, "the tank's plate drawn by the engine");
    let solved = solve(&p);
    assert!(solved.has(QuantityId::AddedMass));
}

/// The first frequency alone (the second order and the control surfaces off): enough to show the
/// engine takes the model.
fn one_frequency(p: &mut Project) {
    p.waves.frequencies.inputs.truncate(1);
    p.waves.frequencies.count = 1;
    p.waves.frequencies.edit_in_table = true;
    p.second_order = Default::default();
    for b in &mut p.bodies {
        b.control_surface = None;
    }
}
