use super::*;
use crate::engine_model::tests_support::cylinder_project;

#[test]
fn a_trimmed_hull_is_cut_at_the_free_surface() {
    // pitch 90 deg turns +x down; the elevation lifts it
    let p = trim_point([0.5, 90.0, 0.0], [1.0, 0.0, 0.0]);
    assert!((p[0]).abs() < 1e-12 && (p[2] + 0.5).abs() < 1e-12, "{p:?}");
    let square = [[0.0, 0.0, -1.0], [1.0, 0.0, -1.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]];
    let part = above_water(&square);
    assert_eq!(part.len(), 4);
    assert!(part.iter().all(|v| v[2] >= 0.0));
    assert!(above_water(&square.map(|v| [v[0], v[1], v[2] - 2.0])).is_empty());
}

#[test]
fn the_jet_palette_runs_blue_to_red() {
    assert_eq!(heat_color(0.0), [0, 0, 127]);
    assert_eq!(heat_color(1.0), [127, 0, 0]);
    assert_eq!(heat_color(0.5)[1], 255);
}

#[test]
fn a_body_is_drawn_with_its_outlines() {
    let p = cylinder_project();
    let s = build_scene(
        &p,
        &SceneInputs {
            focus: Focus::All,
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
    );
    // 768 displayed panels, two triangles each, translucent
    assert_eq!(s.transparent.len(), 768 * 6);
    assert_eq!(s.edges.len(), 768 * 8);
    assert!(s.has_content);
    assert_eq!(s.axes.len(), 6);
}

#[test]
fn channel_walls_hold_the_water_column_and_the_free_surface_reaches_them() {
    let mut p = cylinder_project();
    p.environment.domain = Domain::Channel;
    p.environment.channel_width = 6.0;
    p.environment.depth_type = DepthType::Finite;
    p.environment.depth = 5.0;
    // a field-point grid much larger than the water column
    p.field_points = FieldPoints {
        enabled: true,
        x_start: -40.0,
        x_end: 40.0,
        y_start: -40.0,
        y_end: 40.0,
        z: -1.0,
        ..FieldPoints::default()
    };
    let s = build_scene(
        &p,
        &SceneInputs {
            focus: Focus::All,
            engine: None,
            overlay: None,
            smoothing: true,
            markers: None,
            body_overlay: None,
            free_surface: true,
            normals: false,
            spans: true,
            mode_shape: None,
            animation: None,
        },
    );
    let top = s.transparent.iter().map(|v| v.position[2]).fold(f32::MIN, f32::max);
    let bottom = s.transparent.iter().map(|v| v.position[2]).fold(f32::MAX, f32::min);
    // from the sea bed (half a slab below) to a freeboard of a tenth of the column above the hull
    assert!(bottom > -5.5 && top < 1.0, "the walls span {bottom}..{top}");
    let water = s.transparent.iter().filter(|v| v.position[2] == 0.0 && v.color == rgba(WATER, 0.28));
    let (y0, y1) = water.fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(v.position[1]), b.max(v.position[1])));
    assert_eq!((y0, y1), (-3.0, 3.0), "the free surface reaches both walls");
}

#[test]
fn an_animated_field_rises_by_the_display_scale_but_keeps_its_legend() {
    // an elevation of 0.3 m everywhere: the surface rises 0.3 m at scale 1, 3 m at scale 10;
    // the legend is the elevation itself, ±0.3 m, at both
    let mut p = cylinder_project();
    p.field_points = FieldPoints { enabled: true, ..FieldPoints::default() };
    let points = p.field_points.generate();
    assert!(!points.is_empty());
    let field = crate::animation::ComplexField {
        grid: p.field_points.clone(),
        values: vec![hydrobem_engine::Complex64::new(0.3, 0.0); points.len()],
        points,
        elevation: true,
        label: "Elevation [m]".into(),
    };
    let response = crate::animation::Response { omega: 0.5, field: Some(field), ..Default::default() };
    let frame = |scale: f64| {
        build_scene(
            &p,
            &SceneInputs {
                focus: Focus::All,
                engine: None,
                overlay: None,
                smoothing: true,
                markers: None,
                body_overlay: None,
                free_surface: false,
                normals: false,
                spans: true,
                mode_shape: None,
                animation: Some(crate::animation::Frame { response: &response, phase: 0.0, amplitude: scale }),
            },
        )
    };
    for scale in [1.0, 10.0] {
        let s = frame(scale);
        let c = s.color_scale.expect("a legend");
        assert!((c.max - 0.3).abs() < 1e-9 && (c.min + 0.3).abs() < 1e-9, "{c:?}");
        // no heatmap cells: the bar says solver values, not cell means
        assert_eq!(c.note, "solver values");
        let top = s.transparent.iter().chain(&s.opaque).map(|v| v.position[2]).fold(f32::MIN, f32::max);
        assert!((f64::from(top) - 0.3 * scale).abs() < 1e-3, "{top} at {scale}");
    }
}

#[test]
fn the_incident_wave_is_drawn_at_its_amplitude_times_the_display_scale() {
    // a 2 m wave at the phase of its crest at x = 0 (heading 0, k small): at scale 1 the
    // surface rises to 2 m, at scale 3 to 6 m
    let p = cylinder_project();
    let response = crate::animation::Response { omega: 0.5, incident: Some((1e-6, 0.0, 2.0)), ..Default::default() };
    let top = |scale: f64| {
        let s = build_scene(
            &p,
            &SceneInputs {
                focus: Focus::All,
                engine: None,
                overlay: None,
                smoothing: true,
                markers: None,
                body_overlay: None,
                free_surface: true,
                normals: false,
                spans: true,
                mode_shape: None,
                animation: Some(crate::animation::Frame { response: &response, phase: 0.0, amplitude: scale }),
            },
        );
        s.transparent.iter().chain(&s.opaque).map(|v| v.position[2]).fold(f32::MIN, f32::max)
    };
    assert!((top(1.0) - 2.0).abs() < 1e-3, "{}", top(1.0));
    assert!((top(3.0) - 6.0).abs() < 1e-3, "{}", top(3.0));
}

#[test]
fn walls_and_the_grid_stay_on_the_fluid_side() {
    let mut p = cylinder_project();
    p.bodies[0].position = [0.0, 3.0, 0.0];
    p.environment.domain = Domain::YWall;
    p.field_points = FieldPoints { enabled: true, ..FieldPoints::default() };
    let s = build_scene(
        &p,
        &SceneInputs {
            focus: Focus::All,
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
    );
    // the wall at y = 0 is a slab of 1.2 % of the scene's extent about the plane
    let half_wall = 0.006 * (s.bounds.1[0] - s.bounds.0[0]).max(1.0) as f32 + 1e-4;
    assert!(s.transparent.iter().all(|v| v.position[1] >= -half_wall), "every vertex on the fluid side");
    let mut x = [-5.0, 5.0];
    let mut y = [-5.0, 5.0];
    clip_to_fluid(&mut x, &mut y, Domain::Channel, 4.0);
    assert_eq!((x, y), ([-5.0, 5.0], [-2.0, 2.0]));
}

#[test]
fn a_focus_shows_one_element_alone() {
    let mut p = cylinder_project();
    p.add_mesh("cylinder", p.bodies[0].mesh.clone().unwrap());
    p.bodies[0].position = [50.0, 0.0, 0.0];
    p.add_body();
    p.bodies[1].mesh = p.bodies[0].mesh.clone();
    let scene = |p: &Project, focus| {
        build_scene(
            p,
            &SceneInputs {
                focus,
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
    assert_eq!(scene(&p, Focus::All).transparent.len(), 2 * 768 * 6);
    let body = scene(&p, Focus::Body(0));
    assert_eq!(body.transparent.len(), 768 * 6);
    assert!(body.bounds.0[0] > 48.0, "the body as placed");
    let mesh = scene(&p, Focus::Mesh(0));
    assert_eq!(mesh.transparent.len(), 768 * 6);
    assert!(mesh.bounds.0[0] < 0.0 && mesh.bounds.1[0] > 0.0, "the mesh in its own coordinates");
    // every stored record in its four symmetry copies, for picking
    assert_eq!(mesh.records.len(), 768);
    assert_eq!(mesh.records.iter().filter(|(r, _)| *r == 7).count(), 4);
    assert!(body.records.is_empty());
    assert!(!scene(&p, Focus::Nothing).has_content);
    // a body range draws only those records
    p.bodies[0].mesh_range = Some([1, 9]);
    assert_eq!(scene(&p, Focus::Body(0)).transparent.len(), 9 * 4 * 6);
}

#[test]
fn the_heatmap_is_drawn_between_the_field_points() {
    // a 3 x 3 grid of points 1 m apart, the value 10 x + y
    let grid = FieldPoints {
        enabled: true,
        x_start: 0.0,
        x_end: 2.0,
        x_count: 3,
        y_start: 0.0,
        y_end: 2.0,
        y_count: 3,
        z: 0.0,
        ..FieldPoints::default()
    };
    let nodes: Vec<V3> = (0..3).flat_map(|y| (0..3).map(move |x| [x as f64, y as f64, 0.0])).collect();
    let heatmap = |skip: Option<usize>, smoothing: bool| {
        let points: Vec<V3> = nodes.iter().enumerate().filter(|(k, _)| Some(*k) != skip).map(|(_, p)| *p).collect();
        let values = points.iter().map(|p| 10.0 * p[0] + p[1]).collect();
        let field = FieldOverlay { grid: grid.clone(), points, values, label: "Total [m]".into() };
        build_scene(
            &Project::default(),
            &SceneInputs {
                focus: Focus::All,
                engine: None,
                overlay: Some(&field),
                smoothing,
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
    let quads = |s: &Scene| s.transparent.len() / 6;
    let s = heatmap(None, true);
    // from the first node to the last one, nothing beyond (no half cell outside)
    let (lo, hi) =
        s.transparent.iter().fold((f32::MAX, f32::MIN), |(a, c), v| (a.min(v.position[0]), c.max(v.position[0])));
    assert!(lo.abs() < 1e-6 && (hi - 2.0).abs() < 1e-6, "{lo} {hi}");
    // the four cells, each in 6 x 6 pieces; the legend the solver values (0 .. 22)
    assert_eq!(quads(&s), 4 * HEATMAP_SUB * HEATMAP_SUB);
    assert_eq!(s.color_scale.as_ref().map(|c| (c.min, c.max)), Some((0.0, 22.0)));
    // the piece at the corner (0, 0): the value at its centre, a twelfth of a cell from the
    // node, (10 + 1) / 12 of 0 .. 22, in bin 2 of 64
    let corner = s.transparent.iter().find(|v| v.position[0].abs() < 1e-6 && v.position[1].abs() < 1e-6).unwrap();
    let t = (11.0 / 12.0) / 22.0;
    assert_eq!(corner.color, rgba(heat_color((t * 63.0_f64).floor() / 63.0), 0.82));
    // without interpolation: one colour per cell
    assert_eq!(quads(&heatmap(None, false)), 4);
    // the centre point left out (inside a body): the four cells about it are not drawn, and
    // nothing is drawn in their place
    assert_eq!(quads(&heatmap(Some(4), true)), 0);
    // a corner point left out: only its cell goes
    assert_eq!(quads(&heatmap(Some(0), true)), 3 * HEATMAP_SUB * HEATMAP_SUB);
}

#[test]
fn surface_points_are_drawn_as_a_triangulation_between_them() {
    // a lid like a ring (radius 1 to 2 m, a hole inside) at z = 0, its points unevenly spaced
    // as Gauss points are: triangles between them, none over the hole, each point at its value
    let mut points = Vec::new();
    for (r, n) in [(1.0, 40), (1.1, 40), (1.35, 48), (1.65, 56), (1.9, 64), (2.0, 64)] {
        for k in 0..n {
            let a = std::f64::consts::TAU * f64::from(k) / f64::from(n);
            points.push([r * a.cos(), r * a.sin(), 0.0]);
        }
    }
    let values: Vec<f64> = points.iter().map(|p| p[0]).collect();
    let overlay = BodyOverlay {
        normals: vec![[0.0, 0.0, 1.0]; points.len()],
        points: points.clone(),
        areas: Vec::new(),
        faces: Default::default(),
        values,
        label: "|η| [m]".into(),
        plane: true,
        cover: Default::default(),
    };
    let s = build_scene(
        &Project::default(),
        &SceneInputs {
            focus: Focus::All,
            engine: None,
            overlay: None,
            smoothing: true,
            markers: None,
            body_overlay: Some(&overlay),
            free_surface: false,
            normals: false,
            spans: true,
            mode_shape: None,
            animation: None,
        },
    );
    let tris: Vec<&[Vertex]> = s.opaque.chunks(3).collect();
    assert!(tris.len() > points.len(), "{} triangles", tris.len());
    for t in &tris {
        let c = [0, 1].map(|i| t.iter().map(|v| f64::from(v.position[i])).sum::<f64>() / 3.0);
        let r = c[0].hypot(c[1]);
        assert!((0.98..=2.0).contains(&r), "a triangle over the hole at r = {r}");
    }
    // the point of the largest value (x = 2) has the top colour; the bar says it interpolates
    let top = s.opaque.iter().find(|v| (v.position[0] - 2.0).abs() < 1e-6).unwrap();
    assert_eq!(top.color, rgba(heat_color(1.0), 1.0));
    let scale = s.color_scale.unwrap();
    assert_eq!(scale.note, "interpolated between points");

    // with the surfaces given (the ring in sectors, three quarters of it, from 90 to 360 deg):
    // triangles only over them, none in the missing quarter (x > 0, y > 0)
    let sector = |a0: f64, a1: f64| {
        let p = |r: f64, a: f64| [r * a.cos(), r * a.sin(), 0.0];
        [p(1.0, a0), p(2.0, a0), p(2.0, a1), p(1.0, a1)]
    };
    let step = std::f64::consts::TAU / 96.0;
    let cover: Vec<V3x4> = (24..96).map(|k| sector(step * f64::from(k), step * f64::from(k + 1))).collect();
    let covered = BodyOverlay { cover: std::sync::Arc::new(cover), ..overlay };
    let s = build_scene(
        &Project::default(),
        &SceneInputs {
            focus: Focus::All,
            engine: None,
            overlay: None,
            smoothing: true,
            markers: None,
            body_overlay: Some(&covered),
            free_surface: false,
            normals: false,
            spans: true,
            mode_shape: None,
            animation: None,
        },
    );
    let tris: Vec<&[Vertex]> = s.opaque.chunks(3).collect();
    assert!(!tris.is_empty());
    for t in &tris {
        let c = [0, 1].map(|i| t.iter().map(|v| f64::from(v.position[i])).sum::<f64>() / 3.0);
        assert!(!(c[0] > 0.05 && c[1] > 0.05), "a triangle off the surfaces at {c:?}");
    }
}

#[test]
fn a_heatmap_of_one_ring_shows_a_square_at_each_point() {
    // a polar grid of one ring (no cells between rings): each point a square in plan, not a
    // square of the lattice, which has no width across one ring
    let grid = FieldPoints {
        enabled: true,
        layout: GridLayout::Polar,
        radial_start: 5.0,
        radial_end: 5.0,
        radial_count: 1,
        angle_start: 0.0,
        angle_end: 360.0,
        angle_count: 12,
        ..FieldPoints::default()
    };
    let points = grid.generate();
    assert_eq!(points.len(), 12);
    let values = (0..points.len()).map(|k| k as f64).collect();
    let field = FieldOverlay { grid, points, values, label: "Total [m]".into() };
    let s = build_scene(
        &Project::default(),
        &SceneInputs {
            focus: Focus::All,
            engine: None,
            overlay: Some(&field),
            smoothing: true,
            markers: None,
            body_overlay: None,
            free_surface: false,
            normals: false,
            spans: true,
            mode_shape: None,
            animation: None,
        },
    );
    assert_eq!(s.transparent.len() / 6, 12, "a square of each point");
    // each of some width: the neighbours are 2 * 5 * sin(15 deg) = 2.59 m apart
    for q in s.transparent.chunks(6) {
        let xs = q.iter().map(|v| v.position[0]);
        let (lo, hi) = xs.fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
        assert!(hi - lo > 1.0, "{lo} .. {hi}");
    }
}

#[test]
fn the_heatmap_range_is_that_of_the_shown_values() {
    let grid = FieldPoints {
        enabled: true,
        x_start: 0.0,
        x_end: 2.0,
        x_count: 3,
        y_start: 0.0,
        y_end: 0.0,
        y_count: 1,
        z: 0.0,
        ..FieldPoints::default()
    };
    let field = FieldOverlay {
        grid: grid.clone(),
        points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
        values: vec![1.0, 2.0, 3.0],
        label: "Total [m]".into(),
    };
    let p = Project::default();
    let raw = build_scene(
        &p,
        &SceneInputs {
            focus: Focus::All,
            engine: None,
            overlay: Some(&field),
            smoothing: false,
            markers: None,
            body_overlay: None,
            free_surface: false,
            normals: false,
            spans: true,
            mode_shape: None,
            animation: None,
        },
    );
    let scale = raw.color_scale.unwrap();
    assert_eq!((scale.min, scale.max, scale.levels), (1.0, 3.0, 16));
    let smooth = build_scene(
        &p,
        &SceneInputs {
            focus: Focus::All,
            engine: None,
            overlay: Some(&field),
            smoothing: true,
            markers: None,
            body_overlay: None,
            free_surface: false,
            normals: false,
            spans: true,
            mode_shape: None,
            animation: None,
        },
    );
    let scale = smooth.color_scale.unwrap();
    // interpolated between the nodes, the range is still the solver values' (no blur)
    assert_eq!((scale.min, scale.max, scale.levels), (1.0, 3.0, 64));
    assert!(scale.smoothed);
    assert_eq!(scale.note, "interpolated between points");
}

#[test]
fn a_patch_mesh_shows_the_state_of_cad2bems_checks() {
    use crate::analytic::{AnalyticKind, AnalyticSpec};
    use crate::patch_report::{PatchReport, ReportPatch};
    let mut mesh = (*AnalyticSpec::of(AnalyticKind::Barge).build(None).expect("barge")).clone();
    let states = ["ok", "warn", "fail"];
    let patches = (0..mesh.panels.len())
        .map(|k| ReportPatch { state: states[k.min(2)].into(), ..ReportPatch::default() })
        .collect();
    mesh.report = Some(std::sync::Arc::new(PatchReport { patches, ..PatchReport::default() }));
    let mut p = Project::default();
    p.add_mesh("hull", std::sync::Arc::new(mesh));
    let inputs = SceneInputs {
        focus: Focus::Mesh(0),
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
    };
    let scene = build_scene(&p, &inputs);
    let names: Vec<&str> = scene.legend.iter().map(|(n, _)| *n).collect();
    // and the spans of its discretization
    assert_eq!(names, ["Body", "Check Warning", "Check Failed", "Spans (nu x nv)"]);
    assert!(scene.transparent.iter().any(|v| v.color == rgba(CHECK_FAIL, 0.82)));
}
