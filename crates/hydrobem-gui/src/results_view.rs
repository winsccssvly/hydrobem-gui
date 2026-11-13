//! Details and tables of the results (REQUIREMENTS.md: Results; docs/RESULTS.md).

use eframe::egui::Ui;
use gui_core::animation;
use gui_core::format::format_number;
use gui_core::hydrobem_engine::{Complex64, QuantityId, Severity};
use gui_core::project::{DOF_LABELS, DepthType, Project, V3};
use gui_core::results::{
    FieldKind, SolveResult, coefficient_unit, force_unit, location_text, motion_unit, phase_degrees, warning_name,
};

use crate::output::{Cell, Column, Output, PlotMark, PlotSpec, TableData, Tabs};
use crate::tree::ResultNode;
use crate::widgets::{check, choice, message, read_only, section};

const FREQUENCY_AXIS: &str = "Wave Frequency [rad/s]";

/// The choices of the result views; kept across nodes, reset with each new result.
#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub hydrostatics_body: usize,
    pub force_body: usize,
    pub force_dof: usize,
    pub motion_body: usize,
    pub motion_dof: usize,
    pub load_body: usize,
    pub load_dof: usize,
    pub heading: usize,
    pub real_imaginary: bool,
    pub frequency: usize,
    pub radiation_column: usize,
    /// The bodies whose radiation the Total adds: None every solved body, else one (solved index).
    pub total_body: Option<usize>,
    /// Of that body, one radiation column alone; None all its modes.
    pub total_mode: Option<usize>,
    /// Animation (Details → Animation): shown, playing, the speed choice, and the display scale
    /// of the displacements (1 the real size).
    pub animate: bool,
    pub paused: bool,
    pub speed: usize,
    pub display_scale: f64,
    /// Radiation and Total animations: the bodies kept still instead of moving with the radiation
    /// they add (Move Bodies off).
    pub still_bodies: bool,
    pub drift_method: usize,
    pub drift_body: usize,
    pub drift_component: usize,
    pub point_quantity: usize,
    pub velocity_component: usize,
    pub kochin_quantity: usize,
    pub pair_part: usize,
    /// QTF: the method, the sum frequencies (else the difference), the heading pair (a heading
    /// for Newman's), the body and component, the part shown and the row ω_a of the graph.
    pub qtf_method: usize,
    pub qtf_sum: bool,
    pub qtf_pair: usize,
    pub qtf_body: usize,
    pub qtf_component: usize,
    pub qtf_part: usize,
    pub qtf_row: usize,
    /// The graph of one row ω_a (magnitude and phase) instead of the heatmap of the matrix.
    pub qtf_row_graph: bool,
    /// The depth of the field-point grid shown.
    pub level: usize,
    /// Set by a view while it shows a radiation quantity (per unit motion: the wave amplitude
    /// does not apply to it); cleared before each view.
    pub showing_radiation: bool,
}

/// The depth of the field points shown, chosen when the grid has several; None: all points.
fn field_level(ui: &mut Ui, r: &SolveResult, s: &mut Selection, id: &str) -> Option<f64> {
    let levels = r.field_grid.levels();
    if levels.len() < 2 {
        return None;
    }
    let labels: Vec<String> = levels.iter().map(|z| format!("z = {} m", format_number(*z))).collect();
    s.level = s.level.min(levels.len() - 1);
    choice(ui, (id, "level"), "Depth", &labels, &mut s.level);
    Some(levels[s.level])
}

fn at_level(p: &V3, level: Option<f64>) -> bool {
    level.is_none_or(|z| (p[2] - z).abs() <= 1e-9 * z.abs().max(1.0))
}

/// What a result view colours in the viewport.
#[derive(Clone, Debug, PartialEq)]
pub enum Overlay {
    /// Values at the field points, on their grid.
    Field(FieldOverlay),
    /// Values at the body output points, on the hulls.
    Body(gui_core::scene::BodyOverlay),
}

/// The free-surface amplitudes shown in the viewport.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldOverlay {
    pub points: Vec<V3>,
    pub values: Vec<f64>,
    pub label: String,
}

fn body_labels(r: &SolveResult) -> Vec<String> {
    r.bodies.iter().enumerate().map(|(i, b)| format!("{}: {}", i + 1, b.name)).collect()
}

/// Playback speeds, as multiples of real time.
const SPEEDS: [(f64, &str); 5] = [
    (0.25, "0.25 × real time"),
    (0.5, "0.5 × real time"),
    (1.0, "Real time"),
    (2.0, "2 × real time"),
    (4.0, "4 × real time"),
];

pub fn speed_factor(s: &Selection) -> f64 {
    SPEEDS.get(s.speed).map_or(1.0, |v| v.0)
}

impl Selection {
    pub fn new() -> Selection {
        Selection { speed: 2, display_scale: 1.0, ..Selection::default() }
    }
}

/// The Animation rows of a view that has phases: shown or not, play or pause, speed, and the
/// amplitude of the wave (or of the radiation mode) shown. `moving`: a view with radiation offers
/// Move Bodies, with this tooltip (what moves them).
fn animation_section(ui: &mut Ui, s: &mut Selection, frequencies: Option<&[String]>, moving: Option<&str>) {
    section(ui, "Animation", |ui| {
        check(ui, "Animate", &mut s.animate, Some("Re{value · e^{iωt}}: the solved amplitudes and phases in time."));
        if !s.animate {
            return;
        }
        if let Some(frequencies) = frequencies.filter(|f| !f.is_empty()) {
            s.frequency = s.frequency.min(frequencies.len() - 1);
            choice(ui, "animation", "Wave Frequency [rad/s]", frequencies, &mut s.frequency);
        }
        if let Some(tooltip) = moving {
            let mut on = !s.still_bodies;
            if check(ui, "Move Bodies", &mut on, Some(tooltip)) {
                s.still_bodies = !on;
            }
        }
        let speeds: Vec<String> = SPEEDS.iter().map(|v| v.1.to_string()).collect();
        s.speed = s.speed.min(speeds.len() - 1);
        choice(ui, "animation", "Speed", &speeds, &mut s.speed);
        // a display scale of the displacements; the wave amplitude is Details → Waves
        let mut value = s.display_scale;
        if crate::widgets::number(ui, "animation", "Display Scale (×)", "", &mut value, true)
            && value > 0.0
            && value.is_finite()
        {
            s.display_scale = value;
        }
        message(
            ui,
            "The displacements at the wave amplitude (Waves) times this scale: 1 is the real size. A display of \
             the linear (first-order) result, not a larger-amplitude solution.",
        );
    });
}

/// The response the viewport animates for the selected view (Details → Animation), from the same
/// choices as the view: the motions with the incident wave, or a free-surface field with the
/// bodies that move in it (Total: by their motions; Radiation: the radiating mode).
pub fn animation_response(
    node: ResultNode,
    r: &SolveResult,
    s: &Selection,
    project: &Project,
) -> Option<animation::Response> {
    if !s.animate {
        return None;
    }
    let omega = *r.omegas().get(s.frequency)?;
    let project_body = |b: usize| r.bodies.get(b).and_then(|x| project.bodies.iter().position(|p| p.name == x.name));
    match node {
        ResultNode::Motion => {
            let heading = *r.headings().get(s.heading)?;
            let bodies = (0..r.bodies.len())
                .filter_map(|b| Some((project_body(b)?, r.rigid_motion(b, s.frequency, s.heading))))
                .collect();
            let env = &project.environment;
            let depth = if env.depth_type == DepthType::Infinite { f64::INFINITY } else { env.depth };
            let k = gui_core::hydrobem_engine::wavenumber(omega, depth, env.gravity).ok()?;
            Some(animation::Response {
                omega,
                bodies,
                field: None,
                incident: Some((k, heading, r.wave_amplitude)),
                body_values: None,
            })
        }
        ResultNode::Field(kind) => {
            if r.field_grid.list.is_some() || !r.has_field_results() {
                return None;
            }
            let modes = r.radiation_modes();
            let ids = r.radiation_mode_ids();
            let column_of = |b: usize, dof: usize| {
                r.coordinate_row(b, dof).and_then(|row| ids.iter().position(|id| *id as usize == row + 1))
            };
            let (case, columns) = match kind {
                FieldKind::Radiation => (s.radiation_column.min(modes.len().checked_sub(1)?), None),
                FieldKind::Total => {
                    let columns = s.total_body.map(|b| match s.total_mode {
                        Some(m) => vec![m],
                        None => (0..modes.len()).filter(|k| modes[*k].0 == b).collect(),
                    });
                    (s.heading, columns)
                }
                _ => (s.heading, None),
            };
            let values = r.field_complex(kind, s.frequency, case, columns.as_deref());
            let levels = r.field_grid.levels();
            let level = (levels.len() >= 2).then(|| levels[s.level.min(levels.len() - 1)]);
            let (points, values): (Vec<V3>, Vec<Complex64>) =
                r.field_points.iter().zip(values).filter(|(p, _)| at_level(p, level)).map(|(p, v)| (*p, v)).unzip();
            let elevation = level.unwrap_or(r.field_grid.z) >= 0.0;
            let label = if elevation { "Elevation η(t) [m]" } else { "p/ρg (t) [m]" };
            let mut bodies = Vec::new();
            match kind {
                _ if s.still_bodies => {}
                FieldKind::Radiation => {
                    let (b, _) = modes[case];
                    let row = ids.get(case).map(|id| *id as usize - 1)?;
                    let dof = (0..6).find(|d| r.coordinate_row(b, *d) == Some(row));
                    if let (Some(dof), Some(p)) = (dof, project_body(b)) {
                        let mut motion = [Complex64::default(); 6];
                        motion[dof] = Complex64::new(1.0, 0.0);
                        bodies.push((p, motion));
                    }
                }
                FieldKind::Total if r.has(QuantityId::Motion) => {
                    for b in 0..r.bodies.len() {
                        let Some(p) = project_body(b) else { continue };
                        let mut motion = r.rigid_motion(b, s.frequency, s.heading);
                        for (dof, m) in motion.iter_mut().enumerate() {
                            let added =
                                column_of(b, dof).is_some_and(|k| columns.as_ref().is_none_or(|c| c.contains(&k)));
                            if !added {
                                *m = Complex64::default();
                            }
                        }
                        bodies.push((p, motion));
                    }
                }
                _ => {}
            }
            let field = animation::ComplexField {
                grid: r.field_grid.clone(),
                points,
                values,
                elevation,
                label: label.to_string(),
            };
            Some(animation::Response { omega, bodies, field: Some(field), incident: None, body_values: None })
        }
        ResultNode::BodyPressure | ResultNode::BodyVelocity => {
            let table: &[PointQuantity] =
                if node == ResultNode::BodyPressure { &BODY_PRESSURE } else { &BODY_VELOCITY };
            let available: Vec<PointQuantity> = table.iter().copied().filter(|(q, ..)| r.has(*q)).collect();
            let (q, _, _, radiation, vector) = *available.get(s.point_quantity.min(available.len().checked_sub(1)?))?;
            let modes = r.radiation_modes();
            let case = if radiation { s.radiation_column.min(modes.len().checked_sub(1)?) } else { s.heading };
            let values: Vec<[Complex64; 3]> = if vector {
                r.point_vectors(q, s.frequency, case)
            } else {
                r.point_values(q, s.frequency, case)
                    .into_iter()
                    .map(|v| [v, Complex64::default(), Complex64::default()])
                    .collect()
            };
            let points = r.body_output_points();
            if values.len() != points.len() || points.is_empty() {
                return None;
            }
            let (normals, areas) = r.body_normals_and_areas();
            // a point without an area (a specified body point): a square of a share of the spacing
            let fallback = 0.5 * areas.iter().copied().filter(|a| *a > 0.0).map(f64::sqrt).fold(0.0, f64::max);
            let sizes = (0..points.len())
                .map(|k| areas.get(k).copied().filter(|a| *a > 0.0).map_or(fallback.max(0.02), |a| a.sqrt() / 2.0))
                .collect();
            let mut bodies = Vec::new();
            if radiation && !s.still_bodies {
                let (b, _) = modes[case];
                let ids = r.radiation_mode_ids();
                let row = ids.get(case).map(|id| *id as usize - 1)?;
                let dof = (0..6).find(|d| r.coordinate_row(b, *d) == Some(row));
                if let (Some(dof), Some(p)) = (dof, project_body(b)) {
                    let mut motion = [Complex64::default(); 6];
                    motion[dof] = Complex64::new(1.0, 0.0);
                    bodies.push((p, motion));
                }
            }
            let component = if vector { (s.velocity_component < 3).then_some(s.velocity_component) } else { Some(0) };
            let label = match (vector, component) {
                (false, _) => "p/ρg (t) [m]".to_string(),
                (true, Some(c)) => format!("{} (t) [m/s]", ["u", "v", "w"][c]),
                (true, None) => "|V(t)| [m/s]".to_string(),
            };
            let body_values = animation::BodyValues {
                normals: (0..points.len()).map(|k| normals.get(k).copied().unwrap_or([0.0, 0.0, 1.0])).collect(),
                faces: r.body_point_faces(),
                sizes,
                bodies: points.iter().map(|(b, _)| project_body(*b)).collect(),
                points: points.iter().map(|(_, p)| *p).collect(),
                values,
                component,
                label,
            };
            Some(animation::Response { omega, bodies, field: None, incident: None, body_values: Some(body_values) })
        }
        _ => None,
    }
}

/// The radiation mode chosen as its body and that body's coordinate: the K column and its label.
fn radiation_mode_choice(ui: &mut Ui, key: &str, r: &SolveResult, s: &mut Selection) -> Option<(usize, String)> {
    let modes = r.radiation_modes();
    if modes.is_empty() {
        return None;
    }
    s.radiation_column = s.radiation_column.min(modes.len() - 1);
    let mut bodies: Vec<usize> = modes.iter().map(|m| m.0).collect();
    bodies.dedup();
    let name = |b: usize| r.bodies.get(b).map_or_else(|| format!("Body {}", b + 1), |x| x.name.clone());
    let mut body = bodies.iter().position(|b| *b == modes[s.radiation_column].0).unwrap_or(0);
    let labels: Vec<String> = bodies.iter().map(|b| name(*b)).collect();
    if choice(ui, (key, "body"), "Radiating Body", &labels, &mut body) {
        s.radiation_column = modes.iter().position(|m| m.0 == bodies[body]).unwrap_or(0);
    }
    let columns: Vec<usize> = (0..modes.len()).filter(|k| modes[*k].0 == bodies[body]).collect();
    let mut mode = columns.iter().position(|k| *k == s.radiation_column).unwrap_or(0);
    let mode_labels: Vec<String> = columns.iter().map(|k| modes[*k].1.clone()).collect();
    choice(ui, (key, "mode"), "Radiation Mode", &mode_labels, &mut mode);
    s.radiation_column = columns[mode];
    Some((s.radiation_column, format!("{} · {}", name(bodies[body]), modes[s.radiation_column].1)))
}

/// The radiation columns the Total adds (None: every one) and their label.
fn total_contribution(ui: &mut Ui, r: &SolveResult, s: &mut Selection) -> (Option<Vec<usize>>, String) {
    let modes = r.radiation_modes();
    let mut bodies: Vec<usize> = modes.iter().map(|m| m.0).collect();
    bodies.dedup();
    let name = |b: usize| r.bodies.get(b).map_or_else(|| format!("Body {}", b + 1), |x| x.name.clone());
    let mut options = vec!["All Bodies".to_string()];
    options.extend(bodies.iter().map(|b| name(*b)));
    let mut index = s.total_body.and_then(|b| bodies.iter().position(|x| *x == b)).map_or(0, |i| i + 1);
    if choice(ui, ("field", "total body"), "Radiating Bodies", &options, &mut index) {
        s.total_body = index.checked_sub(1).map(|i| bodies[i]);
        s.total_mode = None;
    }
    let Some(body) = s.total_body.filter(|b| bodies.contains(b)) else {
        s.total_body = None;
        return (None, "All Bodies".into());
    };
    let columns: Vec<usize> = (0..modes.len()).filter(|k| modes[*k].0 == body).collect();
    let mut mode_options = vec![format!("All Modes of {}", name(body))];
    mode_options.extend(columns.iter().map(|k| modes[*k].1.clone()));
    let mut mode = s.total_mode.and_then(|m| columns.iter().position(|k| *k == m)).map_or(0, |i| i + 1);
    if choice(ui, ("field", "total mode"), "Radiation Modes", &mode_options, &mut mode) {
        s.total_mode = mode.checked_sub(1).map(|i| columns[i]);
    }
    match s.total_mode.filter(|m| columns.contains(m)) {
        Some(m) => (Some(vec![m]), format!("{} · {}", name(body), modes[m].1)),
        None => {
            s.total_mode = None;
            (Some(columns), name(body))
        }
    }
}

fn heading_labels(r: &SolveResult) -> Vec<String> {
    r.headings().iter().map(|h| format!("{}°", format_number(*h))).collect()
}

/// The coordinates of a body (rigid, generalized, pressure), the choice kept in range.
fn coordinates_of(r: &SolveResult, body: usize, dof: &mut usize) -> Vec<String> {
    let mut labels: Vec<String> = r.body_coordinates(body).into_iter().map(|(_, l)| l).collect();
    if labels.is_empty() {
        labels = DOF_LABELS.iter().map(|s| s.to_string()).collect();
    }
    *dof = (*dof).min(labels.len() - 1);
    labels
}

/// The computed (pneumatic) damping of the pressure modes, D x D: the rows and columns of the
/// coordinates that have any.
fn pressure_mode_damping(ui: &mut Ui, r: &SolveResult) -> Output {
    let mut out = Output { title: "Pressure Surfaces · Pressure Mode Damping".into(), ..Output::default() };
    section(ui, "Pressure Mode Damping", |ui| {
        let q = QuantityId::PressureModeDamping;
        let (Some(d), Ok(data)) = (r.dims(q), r.results.f64s(q)) else {
            message(ui, "This result was not calculated.");
            return;
        };
        if d.len() != 2 || d[0] != d[1] {
            message(ui, "This result was not calculated.");
            return;
        }
        let n = d[0];
        let labels = r.coordinate_labels();
        // the pressure coordinates, and any other with damping
        let used: Vec<usize> = (0..n)
            .filter(|&i| {
                labels.get(i).is_some_and(|l| l.contains(" Pressure "))
                    || (0..n).any(|j| data[i * n + j] != 0.0 || data[j * n + i] != 0.0)
            })
            .collect();
        read_only(ui, "Unit", q.unit());
        read_only(ui, "Coordinates", &format!("{} of {n}: the pressure modes and any with damping", used.len()));
        message(ui, "Computed by the engine from the pressure modes; the bodies' mode damping is an input.");
        let mut columns = vec![Column::new("Row")];
        columns
            .extend(used.iter().map(|&j| Column::new(labels.get(j).cloned().unwrap_or_else(|| (j + 1).to_string()))));
        out.table = Some(TableData {
            columns,
            rows: used
                .iter()
                .map(|&i| {
                    std::iter::once(Cell::Text(labels.get(i).cloned().unwrap_or_else(|| (i + 1).to_string())))
                        .chain(used.iter().map(|&j| Cell::Num(data[i * n + j])))
                        .collect()
                })
                .collect(),
        });
    });
    out
}

/// Details of a result node; returns the output pane's content and the viewport overlay.
/// Whether a result view shows what the incident waves drive (its values scale with the wave
/// amplitude): the wave loads, motions, free surface, drifts, Kochin functions and point values.
fn wave_driven(node: ResultNode) -> bool {
    matches!(
        node,
        ResultNode::FroudeKrylov
            | ResultNode::Scattering
            | ResultNode::Haskind
            | ResultNode::Excitation
            | ResultNode::Motion
            | ResultNode::Field(_)
            | ResultNode::Drift
            | ResultNode::Qtf
            | ResultNode::Kochin
            | ResultNode::BodyPressure
            | ResultNode::BodyVelocity
            | ResultNode::LidElevation
            | ResultNode::SurfaceElevation
            | ResultNode::FieldVelocity
    )
}

/// The Details of a result node and its output. `wave_amplitude` is the incident wave amplitude
/// the values are shown for (m, edited on the views of what the waves drive; not saved).
pub fn show(
    ui: &mut Ui,
    node: ResultNode,
    r: &SolveResult,
    s: &mut Selection,
    smoothing: &mut bool,
    wave_amplitude: &mut f64,
) -> (Output, Option<Overlay>) {
    if wave_driven(node) {
        section(ui, "Waves", |ui| {
            let mut a = *wave_amplitude;
            if crate::widgets::number(ui, "waves", "Wave Amplitude", "m", &mut a, true) && a > 0.0 && a.is_finite() {
                *wave_amplitude = a;
            }
            message(
                ui,
                "The results are per metre of wave amplitude (the drifts per square metre); they are shown \
                 for this amplitude. The radiation per unit motion does not change. Not saved.",
            );
        });
    }
    s.showing_radiation = false;
    let (mut out, overlay) = show_node(ui, node, r, s, smoothing);
    if wave_driven(node) && !s.showing_radiation && r.wave_amplitude != 1.0 && !out.title.is_empty() {
        out.title = format!("{} · A = {} m", out.title, format_number(r.wave_amplitude));
    }
    (out, overlay)
}

fn show_node(
    ui: &mut Ui,
    node: ResultNode,
    r: &SolveResult,
    s: &mut Selection,
    smoothing: &mut bool,
) -> (Output, Option<Overlay>) {
    let field = |(o, f): (Output, Option<FieldOverlay>)| (o, f.map(Overlay::Field));
    match node {
        ResultNode::Summary
        | ResultNode::ExcitationGroup
        | ResultNode::BodyPoints
        | ResultNode::FieldPoints
        | ResultNode::Lids
        | ResultNode::PressureSurfaces => (summary(ui, r), None),
        ResultNode::Hydrostatics => (hydrostatics(ui, r, s), None),
        ResultNode::AddedMass => (coefficient(ui, r, s, true), None),
        ResultNode::Damping => (coefficient(ui, r, s, false), None),
        ResultNode::FroudeKrylov => {
            (complex(ui, r, s, QuantityId::ExcitationFroudeKrylov, "Excitation · Froude Krylov"), None)
        }
        ResultNode::Scattering => {
            (complex(ui, r, s, QuantityId::ExcitationScattering, "Excitation · Scattering"), None)
        }
        ResultNode::Excitation => {
            (complex(ui, r, s, QuantityId::ExcitationDiffraction, "Excitation · Diffraction"), None)
        }
        ResultNode::Haskind => (complex(ui, r, s, QuantityId::ExcitationHaskind, "Excitation · Haskind"), None),
        ResultNode::Motion => (complex(ui, r, s, QuantityId::Motion, "Motion"), None),
        ResultNode::Field(kind) => field(free_surface(ui, r, s, kind, smoothing)),
        ResultNode::Drift => (drift(ui, r, s), None),
        ResultNode::Qtf => (qtf(ui, r, s), None),
        ResultNode::Kochin => (kochin(ui, r, s), None),
        ResultNode::BodyPressure => points(ui, r, s, &BODY_PRESSURE, "Body Points · Pressure"),
        ResultNode::LidElevation => points(ui, r, s, &LID_ELEVATION, "Lids · Elevation"),
        ResultNode::SurfaceElevation => points(ui, r, s, &SURFACE_ELEVATION, "Pressure Surfaces · Elevation"),
        ResultNode::PressureModeDamping => (pressure_mode_damping(ui, r), None),
        ResultNode::FieldVelocity => points(ui, r, s, &FIELD_VELOCITY, "Field Points · Velocity"),
        ResultNode::BodyVelocity => points(ui, r, s, &BODY_VELOCITY, "Body Points · Velocity"),
    }
}

/// The mean drifts: the quantity, its label, and whether it is shown by heading pairs (the
/// bidirectional drift of every pair of headings).
const DRIFT_METHODS: [(QuantityId, &str, bool); 6] = [
    (QuantityId::DriftPressureIntegration, "Pressure Integration", false),
    (QuantityId::DriftControlSurface, "Control Surface", false),
    (QuantityId::DriftMomentumConservation, "Momentum Conservation", false),
    (QuantityId::DriftMomentumConservation, "Momentum, Heading Pairs", true),
    (QuantityId::DriftPressureIntegrationBodyAxes, "Pressure Integration, Moving Origin", false),
    (QuantityId::DriftControlSurfaceBodyAxes, "Control Surface, Moving Origin", false),
];

/// The mean drift force and moment per unit wave amplitude squared, over frequency.
fn drift(ui: &mut Ui, r: &SolveResult, s: &mut Selection) -> Output {
    let mut out = Output { title: "Drift".into(), ..Output::default() };
    section(ui, "Drift", |ui| {
        let methods: Vec<(QuantityId, &str, bool)> = DRIFT_METHODS
            .iter()
            .copied()
            .filter(|(q, _, pairs)| r.has(*q) && (!pairs || r.drift_bidirectional()))
            .collect();
        let headings = heading_labels(r);
        if methods.is_empty() || headings.is_empty() {
            message(ui, "This result was not calculated.");
            return;
        }
        s.drift_method = s.drift_method.min(methods.len() - 1);
        let labels: Vec<String> = methods.iter().map(|(_, l, _)| l.to_string()).collect();
        choice(ui, "drift", "Method", &labels, &mut s.drift_method);
        let (q, method, pairs) = methods[s.drift_method];
        if pairs {
            drift_pairs(ui, r, s, q, &mut out);
            return;
        }
        let momentum = q == QuantityId::DriftMomentumConservation;
        let moving =
            matches!(q, QuantityId::DriftPressureIntegrationBodyAxes | QuantityId::DriftControlSurfaceBodyAxes);
        // per body six components (or the three moments about the moving rotation centre R) in body axes, or
        // surge, sway, yaw of all bodies in global axes
        let components: Vec<&str> = if momentum {
            vec!["Surge", "Sway", "Yaw"]
        } else if moving {
            DOF_LABELS[3..].to_vec()
        } else {
            DOF_LABELS.to_vec()
        };
        read_only(
            ui,
            "Axes",
            if momentum {
                "Global, all bodies"
            } else if moving {
                "Body axes, about the moving R"
            } else {
                "Body axes, about R (rotation centre)"
            },
        );
        // the columns of the components: the body's coordinate rows (the force and moment on its
        // rigid coordinates), its three columns of the body-axes moments, or the global three
        let mut columns: Vec<Option<usize>> = (0..components.len()).map(Some).collect();
        if !momentum {
            s.drift_body = s.drift_body.min(r.bodies.len().max(1) - 1);
            choice(ui, "drift", "Body", &body_labels(r), &mut s.drift_body);
            columns = if moving {
                (0..3).map(|c| Some(3 * s.drift_body + c)).collect()
            } else {
                (0..6).map(|c| r.coordinate_row(s.drift_body, c)).collect()
            };
        }
        let names: Vec<String> = components.iter().map(|c| c.to_string()).collect();
        s.drift_component = s.drift_component.min(names.len() - 1);
        choice(ui, "drift", "Component", &names, &mut s.drift_component);
        s.heading = s.heading.min(headings.len() - 1);
        choice(ui, "drift", "Wave Heading [deg]", &headings, &mut s.heading);
        let unit = |c: usize| {
            r.wave_unit(if moving || (momentum && c == 2) || (!momentum && c >= 3) { "N·m/m²" } else { "N/m²" })
        };
        let series: Vec<Vec<f64>> =
            columns.iter().map(|c| c.map(|c| r.drift_series(q, s.heading, c)).unwrap_or_default()).collect();
        if series.iter().any(Vec::is_empty) {
            read_only(ui, "Status", "The selected body is unavailable.");
            return;
        }
        let omegas = r.omegas();
        let mut columns = vec![Column::new("Number"), Column::new(FREQUENCY_AXIS)];
        columns.extend(components.iter().enumerate().map(|(c, n)| Column::new(format!("{n} [{}]", unit(c)))));
        out.title = format!("Drift · {method} · {}", headings[s.heading]);
        out.table = Some(TableData {
            columns,
            rows: (0..omegas.len())
                .map(|f| {
                    [Cell::Int(f as i64 + 1), Cell::Num(omegas[f])]
                        .into_iter()
                        .chain(series.iter().map(|v| Cell::Num(v[f])))
                        .collect()
                })
                .collect(),
        });
        let c = s.drift_component;
        out.plot = Some(PlotSpec::Single {
            x_label: FREQUENCY_AXIS.into(),
            y_label: format!("{} [{}]", components[c], unit(c)),
            x: omegas,
            y: series[c].clone(),
            marks: Vec::new(),
        });
    });
    out
}

/// The QTFs of the engine: per method its difference-frequency quantity, its sum-frequency one
/// (none for Newman's: real, by heading, the global three of the momentum conservation or the
/// body's coordinates) and its label. The first two are the quadratic parts, the next two the
/// potential parts.
const QTF_METHODS: [(QuantityId, Option<QuantityId>, &str); 7] = [
    (
        QuantityId::QtfQuadraticPressureIntegrationDifference,
        Some(QuantityId::QtfQuadraticPressureIntegrationSum),
        "Quadratic, Pressure Integration",
    ),
    (
        QuantityId::QtfQuadraticControlSurfaceDifference,
        Some(QuantityId::QtfQuadraticControlSurfaceSum),
        "Quadratic, Control Surface",
    ),
    (QuantityId::QtfPotentialDirectDifference, Some(QuantityId::QtfPotentialDirectSum), "Potential, Direct"),
    (QuantityId::QtfPotentialIndirectDifference, Some(QuantityId::QtfPotentialIndirectSum), "Potential, Indirect"),
    (QuantityId::QtfNewmanPressureIntegrationDifference, None, "Newman, Pressure Integration"),
    (QuantityId::QtfNewmanControlSurfaceDifference, None, "Newman, Control Surface"),
    (QuantityId::QtfNewmanMomentumConservationDifference, None, "Newman, Momentum Conservation"),
];

/// One way to show the QTFs: the quantities of the difference frequencies, those of the sum
/// frequencies (none for Newman's) and its label. A method of the engine has one quantity of
/// each; a total has a quadratic and a potential part, summed by the GUI (the engine gives the
/// parts only).
struct QtfMethod {
    difference: Vec<QuantityId>,
    sum: Option<Vec<QuantityId>>,
    label: String,
}

/// The methods the solve has: the quadratic and potential parts, the total of every quadratic
/// part with every potential part, then Newman's approximations.
fn qtf_methods(r: &SolveResult) -> Vec<QtfMethod> {
    let single = |(d, s, l): (QuantityId, Option<QuantityId>, &str)| QtfMethod {
        difference: vec![d],
        sum: s.map(|q| vec![q]),
        label: l.into(),
    };
    let mut methods: Vec<QtfMethod> = QTF_METHODS[..4].iter().copied().map(single).collect();
    for (qd, qs, ql) in &QTF_METHODS[..2] {
        for (pd, ps, pl) in &QTF_METHODS[2..4] {
            methods.push(QtfMethod {
                difference: vec![*qd, *pd],
                sum: qs.zip(*ps).map(|(a, b)| vec![a, b]),
                label: format!(
                    "Total, {} + {}",
                    ql.trim_start_matches("Quadratic, "),
                    pl.trim_start_matches("Potential, ")
                ),
            });
        }
    }
    methods.extend(QTF_METHODS[4..].iter().copied().map(single));
    let solved = |parts: &[QuantityId]| parts.iter().all(|q| r.has(*q));
    methods.retain(|m| solved(&m.difference) || m.sum.as_deref().is_some_and(solved));
    methods
}

/// The second-order force and moment of every pair of frequencies (per unit wave amplitude
/// squared): the matrix ω_a × ω_b of one method, frequency kind, heading pair and component.
fn qtf(ui: &mut Ui, r: &SolveResult, s: &mut Selection) -> Output {
    let mut out = Output { title: "QTF".into(), ..Output::default() };
    section(ui, "QTF", |ui| {
        let methods = qtf_methods(r);
        let omegas = r.omegas();
        if methods.is_empty() || omegas.is_empty() {
            message(ui, "This result was not calculated.");
            return;
        }
        s.qtf_method = s.qtf_method.min(methods.len() - 1);
        let labels: Vec<String> = methods.iter().map(|m| m.label.clone()).collect();
        choice(ui, "qtf", "Method", &labels, &mut s.qtf_method);
        let QtfMethod { difference, sum, label: method } = &methods[s.qtf_method];
        let newman = sum.is_none();
        if difference.len() > 1 {
            read_only(ui, "Total", "Quadratic + potential");
        }
        // the frequency kinds the solve computed
        let kinds: Vec<(&[QuantityId], &str)> =
            [(difference.as_slice(), "Difference ω_a − ω_b"), (sum.as_deref().unwrap_or(difference), "Sum ω_a + ω_b")]
                .into_iter()
                .take(if newman { 1 } else { 2 })
                .filter(|(parts, _)| parts.iter().all(|q| r.has(*q)))
                .collect();
        if kinds.is_empty() {
            message(ui, "This result was not calculated.");
            return;
        }
        let mut kind = usize::from(s.qtf_sum).min(kinds.len() - 1);
        let kind_labels: Vec<String> = kinds.iter().map(|(_, l)| l.to_string()).collect();
        if choice(ui, "qtf", "Frequencies", &kind_labels, &mut kind) {
            s.qtf_sum = kinds[kind].0 != difference.as_slice();
        }
        let (quantities, kind_label) = kinds[kind];
        let q = quantities[0];
        // the heading pairs (Newman's: the headings, each with itself)
        let pairs: Vec<String> = if newman {
            heading_labels(r)
        } else {
            r.qtf_heading_pairs()
                .iter()
                .map(|p| format!("{}°, {}°", format_number(p[0]), format_number(p[1])))
                .collect()
        };
        if pairs.is_empty() {
            message(ui, "This result was not calculated.");
            return;
        }
        s.qtf_pair = s.qtf_pair.min(pairs.len() - 1);
        choice(ui, "qtf", if newman { "Wave Heading [deg]" } else { "Heading Pair [deg]" }, &pairs, &mut s.qtf_pair);
        // the component: the global three of the momentum conservation, else a body's coordinate
        let global = q == QuantityId::QtfNewmanMomentumConservationDifference;
        let (column, rotation, component) = if global {
            let names = ["Surge".to_string(), "Sway".to_string(), "Yaw".to_string()];
            read_only(ui, "Axes", "Global, all bodies");
            s.qtf_component = s.qtf_component.min(2);
            choice(ui, "qtf", "Component", &names, &mut s.qtf_component);
            (s.qtf_component, s.qtf_component == 2, names[s.qtf_component].clone())
        } else {
            read_only(ui, "Axes", "Body axes, about R (rotation centre)");
            s.qtf_body = s.qtf_body.min(r.bodies.len().max(1) - 1);
            choice(ui, "qtf", "Body", &body_labels(r), &mut s.qtf_body);
            let names = coordinates_of(r, s.qtf_body, &mut s.qtf_component);
            choice(ui, "qtf", "Component", &names, &mut s.qtf_component);
            let row = r.body_coordinates(s.qtf_body).get(s.qtf_component).map(|(row, _)| *row);
            let Some(row) = row else {
                read_only(ui, "Status", "The selected body is unavailable.");
                return;
            };
            (row, r.is_rotation(row), names[s.qtf_component].clone())
        };
        let parts = ["Magnitude".to_string(), "Real".to_string(), "Imaginary".to_string(), "Phase".to_string()];
        s.qtf_part = s.qtf_part.min(parts.len() - 1);
        choice(ui, "qtf", "Show", &parts, &mut s.qtf_part);
        let unit = r.wave_unit(if rotation { "N·m/m²" } else { "N/m²" });
        read_only(ui, "Unit", if s.qtf_part == 3 { "deg" } else { &unit });
        let matrix = r.qtf_sum_of(quantities, s.qtf_pair, column);
        if matrix.is_empty() {
            read_only(ui, "Status", "The selected component is unavailable.");
            return;
        }
        let graphs = ["Heatmap".to_string(), "Row ω_a".to_string()];
        let mut graph = usize::from(s.qtf_row_graph);
        if choice(ui, "qtf", "Graph", &graphs, &mut graph) {
            s.qtf_row_graph = graph == 1;
        }
        let frequencies: Vec<String> = omegas.iter().map(|w| format!("{} rad/s", format_number(*w))).collect();
        s.qtf_row = s.qtf_row.min(frequencies.len() - 1);
        if s.qtf_row_graph {
            choice(ui, "qtf", "Graph Row ω_a", &frequencies, &mut s.qtf_row);
        }
        let part = |z: &gui_core::hydrobem_engine::Complex64| match s.qtf_part {
            1 => z.re,
            2 => z.im,
            3 => gui_core::results::phase_degrees(*z),
            _ => z.norm(),
        };
        let mut columns = vec![Column::new("ω_a \\ ω_b [rad/s]")];
        columns.extend(omegas.iter().map(|w| Column::new(format_number(*w))));
        let shown_unit = if s.qtf_part == 3 { "deg".to_string() } else { unit.clone() };
        out.title = format!("QTF · {method} · {kind_label} · {} · {component} [{shown_unit}]", pairs[s.qtf_pair]);
        if s.qtf_row_graph {
            out.title.push_str(&format!(" · graph at ω_a = {} rad/s", format_number(omegas[s.qtf_row])));
        }
        out.table = Some(TableData {
            columns,
            rows: matrix
                .iter()
                .zip(&omegas)
                .map(|(row, w)| std::iter::once(Cell::Num(*w)).chain(row.iter().map(|z| Cell::Num(part(z)))).collect())
                .collect(),
        });
        if !s.qtf_row_graph {
            // the matrix in the part shown: rows ω_a (the table's rows), columns ω_b
            out.plot = Some(PlotSpec::Heatmap {
                x_label: "ω_b [rad/s]".into(),
                y_label: "ω_a [rad/s]".into(),
                value_label: format!("{} of {component} [{shown_unit}]", parts[s.qtf_part]),
                x: omegas.clone(),
                y: omegas.clone(),
                values: matrix.iter().map(|row| row.iter().map(part).collect()).collect(),
            });
            return;
        }
        let row = &matrix[s.qtf_row];
        out.plot = Some(PlotSpec::Paired {
            x_label: "ω_b [rad/s]".into(),
            top_label: format!("|{component}| [{unit}]"),
            bottom_label: "Phase [deg]".into(),
            x: omegas.clone(),
            top: row.iter().map(|z| z.norm()).collect(),
            bottom: row.iter().map(|z| gui_core::results::phase_degrees(*z)).collect(),
            phase: true,
        });
    });
    out
}

/// The bidirectional momentum drift of every pair of headings at one frequency.
fn drift_pairs(ui: &mut Ui, r: &SolveResult, s: &mut Selection, q: QuantityId, out: &mut Output) {
    let omegas = r.omegas();
    let headings = heading_labels(r);
    if omegas.is_empty() || headings.is_empty() {
        message(ui, "This result was not calculated.");
        return;
    }
    read_only(ui, "Axes", "Global, all bodies; rows the first heading, columns the second");
    let frequencies: Vec<String> = omegas.iter().map(|w| format!("{} rad/s", format_number(*w))).collect();
    s.frequency = s.frequency.min(frequencies.len() - 1);
    choice(ui, "pairs", "Wave Frequency [rad/s]", &frequencies, &mut s.frequency);
    let components = ["Surge".to_string(), "Sway".to_string(), "Yaw".to_string()];
    s.drift_component = s.drift_component.min(2);
    choice(ui, "pairs", "Component", &components, &mut s.drift_component);
    let parts = ["Magnitude".to_string(), "Real".to_string(), "Imaginary".to_string()];
    choice(ui, "pairs", "Show", &parts, &mut s.pair_part);
    let unit = r.wave_unit(if s.drift_component == 2 { "N·m/m²" } else { "N/m²" });
    read_only(ui, "Unit", &unit);
    let pairs = r.drift_pairs(q, s.frequency, s.drift_component);
    if pairs.is_empty() {
        read_only(ui, "Status", "The selected frequency is unavailable.");
        return;
    }
    let part = |z: &gui_core::hydrobem_engine::Complex64| match s.pair_part {
        1 => z.re,
        2 => z.im,
        _ => z.norm(),
    };
    let mut columns = vec![Column::new("Heading 1 \\ 2")];
    columns.extend(headings.iter().map(|h| Column::new(h.clone())));
    out.title =
        format!("Drift · Heading Pairs · {} · {} [{unit}]", frequencies[s.frequency], components[s.drift_component]);
    out.table = Some(TableData {
        columns,
        rows: pairs
            .iter()
            .enumerate()
            .map(|(i, row)| {
                std::iter::once(Cell::Text(headings.get(i).cloned().unwrap_or_default()))
                    .chain(row.iter().map(|z| Cell::Num(part(z))))
                    .collect()
            })
            .collect(),
    });
}

/// The Kochin functions over the angle, for one frequency and heading or radiation mode.
fn kochin(ui: &mut Ui, r: &SolveResult, s: &mut Selection) -> Output {
    let mut out = Output { title: "Kochin".into(), ..Output::default() };
    section(ui, "Kochin", |ui| {
        let kinds: Vec<(QuantityId, &str, &str, bool)> = [
            (QuantityId::KochinDiffraction, "Diffraction (Scattered)", "m³/s per m", false),
            (QuantityId::KochinRadiation, "Radiation", "m^(2+r) per unit velocity", true),
            (QuantityId::KochinDiffractionDerivative, "Diffraction, Angular Derivative", "m³/s per m per rad", false),
            (
                QuantityId::KochinRadiationDerivative,
                "Radiation, Angular Derivative",
                "m^(2+r) per unit velocity per rad",
                true,
            ),
        ]
        .into_iter()
        .filter(|k| r.has(k.0))
        .collect();
        let omegas = r.omegas();
        let angles: Vec<f64> = r
            .results
            .f64s(QuantityId::KochinAngles)
            .map(|a| a.iter().map(|x| x.to_degrees()).collect())
            .unwrap_or_default();
        if kinds.is_empty() || omegas.is_empty() || angles.is_empty() {
            message(ui, "This result was not calculated.");
            return;
        }
        s.kochin_quantity = s.kochin_quantity.min(kinds.len() - 1);
        let labels: Vec<String> = kinds.iter().map(|k| k.1.to_string()).collect();
        choice(ui, "kochin", "Potential", &labels, &mut s.kochin_quantity);
        let (q, label, unit, radiation) = kinds[s.kochin_quantity];
        s.showing_radiation = radiation;
        // the diffraction's per metre of wave amplitude; the radiation's per unit velocity
        let unit = if radiation { unit.to_string() } else { r.wave_unit(unit) };
        read_only(ui, "Unit", &unit);
        read_only(ui, "Angles", &format!("{} (global, deg)", angles.len()));
        let frequencies: Vec<String> = omegas.iter().map(|w| format!("{} rad/s", format_number(*w))).collect();
        s.frequency = s.frequency.min(frequencies.len() - 1);
        choice(ui, "kochin", "Wave Frequency [rad/s]", &frequencies, &mut s.frequency);
        let case = if radiation {
            let Some((column, _)) = radiation_mode_choice(ui, "kochin", r, s) else { return };
            column
        } else {
            let headings = heading_labels(r);
            s.heading = s.heading.min(headings.len().max(1) - 1);
            choice(ui, "kochin", "Wave Heading [deg]", &headings, &mut s.heading);
            s.heading
        };
        let values = r.point_values(q, s.frequency, case);
        if values.len() != angles.len() {
            read_only(ui, "Status", "The selection is unavailable.");
            return;
        }
        let (mag, phase): (Vec<f64>, Vec<f64>) = values.iter().map(|v| (v.norm(), phase_degrees(*v))).unzip();
        out.title = format!("Kochin Function · {label} · {}", frequencies[s.frequency]);
        out.table = Some(TableData {
            columns: vec![
                Column::new("Number"),
                Column::new("Angle [deg]"),
                Column::new(format!("Magnitude [{unit}]")),
                Column::new("Phase [deg]"),
                Column::new(format!("Re [{unit}]")),
                Column::new(format!("Im [{unit}]")),
            ],
            rows: (0..values.len())
                .map(|i| {
                    vec![
                        Cell::Int(i as i64 + 1),
                        Cell::Num(angles[i]),
                        Cell::Num(mag[i]),
                        Cell::Fixed(phase[i], 3),
                        Cell::Num(values[i].re),
                        Cell::Num(values[i].im),
                    ]
                })
                .collect(),
        });
        out.plot = Some(PlotSpec::Paired {
            x_label: "Angle [deg]".into(),
            top_label: format!("Magnitude [{unit}]"),
            bottom_label: "Phase [deg]".into(),
            x: angles,
            top: mag,
            bottom: phase,
            phase: true,
        });
    });
    out
}

/// Where the points of a point quantity are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Field,
    Body,
    Lid,
    Surface,
}

/// A complex quantity at points: (quantity, label, its points, per radiation column, vector).
type PointQuantity = (QuantityId, &'static str, Source, bool, bool);

const BODY_PRESSURE: [PointQuantity; 2] = [
    (QuantityId::BodyPointsPressureDiffraction, "Diffraction", Source::Body, false, false),
    (QuantityId::BodyPointsPressureRadiation, "Radiation", Source::Body, true, false),
];

const FIELD_VELOCITY: [PointQuantity; 4] = [
    (QuantityId::FieldPointsVelocityIncident, "Incident", Source::Field, false, true),
    (QuantityId::FieldPointsVelocityScattering, "Scattering", Source::Field, false, true),
    (QuantityId::FieldPointsVelocityDiffraction, "Diffraction", Source::Field, false, true),
    (QuantityId::FieldPointsVelocityRadiation, "Radiation", Source::Field, true, true),
];

const BODY_VELOCITY: [PointQuantity; 2] = [
    (QuantityId::BodyPointsVelocityDiffraction, "Diffraction", Source::Body, false, true),
    (QuantityId::BodyPointsVelocityRadiation, "Radiation", Source::Body, true, true),
];

const LID_ELEVATION: [PointQuantity; 2] = [
    (QuantityId::LidsElevationDiffraction, "Diffraction", Source::Lid, false, false),
    (QuantityId::LidsElevationRadiation, "Radiation", Source::Lid, true, false),
];

const SURFACE_ELEVATION: [PointQuantity; 2] = [
    (QuantityId::PressureSurfacesElevationDiffraction, "Diffraction", Source::Surface, false, false),
    (QuantityId::PressureSurfacesElevationRadiation, "Radiation", Source::Surface, true, false),
];

/// The added mass at ω = 0 or ∞ of the force and motion bodies chosen: their 6 x 6 block (a
/// tab of Added Mass); None when that limit was not solved.
fn added_mass_limit_table(r: &SolveResult, s: &Selection, infinite: bool) -> Option<TableData> {
    let q = if infinite { QuantityId::AddedMassInfiniteFrequency } else { QuantityId::AddedMassZeroFrequency };
    if !r.has(q) || r.bodies.is_empty() {
        return None;
    }
    let mut columns = vec![Column::new("Row")];
    columns.extend(DOF_LABELS.iter().map(|d| Column::new(*d)));
    Some(TableData {
        columns,
        rows: (0..6)
            .map(|i| {
                let row = r.coordinate_row(s.force_body, i).unwrap_or(0);
                std::iter::once(Cell::Text(DOF_LABELS[i].to_string()))
                    .chain((0..6).map(|j| {
                        let id = r.coordinate_row(s.motion_body, j).unwrap_or(0) + 1;
                        match r.radiation_column(id).and_then(|c| r.added_mass_limit(infinite, row, c)) {
                            Some(v) => Cell::Num(v),
                            None => Cell::Text("—".into()),
                        }
                    }))
                    .collect()
            })
            .collect(),
    })
}

/// Body pressures or velocities at the output points for one frequency and case.
fn points(
    ui: &mut Ui,
    r: &SolveResult,
    s: &mut Selection,
    table: &[PointQuantity],
    title: &str,
) -> (Output, Option<Overlay>) {
    let mut out = Output { title: title.into(), ..Output::default() };
    let mut overlay = None;
    section(ui, title, |ui| {
        let available: Vec<PointQuantity> = table.iter().copied().filter(|(q, ..)| r.has(*q)).collect();
        let omegas = r.omegas();
        if available.is_empty() || omegas.is_empty() {
            message(ui, "This result was not calculated.");
            return;
        }
        s.point_quantity = s.point_quantity.min(available.len() - 1);
        let labels: Vec<String> = available.iter().map(|a| a.1.to_string()).collect();
        choice(ui, ("points", title), "Quantity", &labels, &mut s.point_quantity);
        let (q, label, source, radiation, vector) = available[s.point_quantity];
        s.showing_radiation = radiation;
        let field = source == Source::Field;
        let level = if field { field_level(ui, r, s, title) } else { None };
        let frequencies: Vec<String> = omegas.iter().map(|w| format!("{} rad/s", format_number(*w))).collect();
        s.frequency = s.frequency.min(frequencies.len() - 1);
        choice(ui, ("points", title), "Wave Frequency [rad/s]", &frequencies, &mut s.frequency);
        let case = if radiation {
            let Some((column, _)) = radiation_mode_choice(ui, title, r, s) else {
                message(ui, "No radiation mode was solved.");
                return;
            };
            column
        } else {
            let headings = heading_labels(r);
            s.heading = s.heading.min(headings.len().max(1) - 1);
            choice(ui, ("points", title), "Wave Heading [deg]", &headings, &mut s.heading);
            s.heading
        };
        let unit = if vector { "m/s" } else { "m" };
        // the Unit row: per unit motion for a radiation; for the waves per metre, or at the wave
        // amplitude (the columns and the legend keep the short unit)
        let unit_text = if radiation {
            format!("{unit} per unit displacement")
        } else if r.wave_amplitude == 1.0 {
            format!("{unit} per wave amplitude")
        } else {
            unit.to_string()
        };
        read_only(ui, "Unit", &unit_text);
        let colour_by = ["|u|".to_string(), "|v|".to_string(), "|w|".to_string(), "|V|".to_string()];
        if vector {
            s.velocity_component = s.velocity_component.min(3);
            choice(ui, ("points", title), "Colour By", &colour_by, &mut s.velocity_component);
        }
        let mut points: Vec<(Option<String>, V3)> = match source {
            Source::Field => r.field_points.iter().map(|p| (None, *p)).collect(),
            Source::Body => r
                .body_output_points()
                .into_iter()
                .map(|(b, p)| (Some(r.bodies.get(b).map_or_else(|| (b + 1).to_string(), |x| x.name.clone())), p))
                .collect(),
            Source::Lid => r
                .surface_points(QuantityId::Lids, QuantityId::LidsIds)
                .into_iter()
                .map(|(k, p)| (Some(format!("Lid {k}")), p))
                .collect(),
            Source::Surface => r
                .surface_points(QuantityId::PressureSurfaces, QuantityId::PressureSurfacesIds)
                .into_iter()
                .map(|(k, p)| (Some(format!("Surface {k}")), p))
                .collect(),
        };
        read_only(ui, "Points", &points.len().to_string());
        let mut columns = vec![Column::new("Number")];
        columns.push(Column::new(match source {
            Source::Field => "",
            Source::Body => "Body",
            Source::Lid => "Lid",
            Source::Surface => "Surface",
        }));
        if field {
            columns.pop();
        }
        columns.extend(["X [m]", "Y [m]", "Z [m]"].map(Column::new));
        let mut rows = Vec::new();
        let mut shade = Vec::new();
        if vector {
            columns.extend(["|u|", "|v|", "|w|"].map(|c| Column::new(format!("{c} [{unit}]"))));
            for (i, (v, (b, p))) in r.point_vectors(q, s.frequency, case).iter().zip(&points).enumerate() {
                let mut row = point_row(i, b, *p);
                row.extend(v.iter().map(|c| Cell::Num(c.norm())));
                rows.push(row);
                shade.push(match s.velocity_component {
                    3 => v.iter().map(|c| c.norm_sqr()).sum::<f64>().sqrt(),
                    k => v[k].norm(),
                });
            }
        } else {
            columns.push(Column::new(if source == Source::Body { "|p/ρg| [m]" } else { "|η| [m]" }));
            columns.push(Column::new("Phase [deg]"));
            for (i, (v, (b, p))) in r.point_values(q, s.frequency, case).iter().zip(&points).enumerate() {
                let mut row = point_row(i, b, *p);
                row.push(Cell::Num(v.norm()));
                row.push(Cell::Fixed(phase_degrees(*v), 3));
                rows.push(row);
                shade.push(v.norm());
            }
        }
        let quantity = if vector {
            colour_by[s.velocity_component.min(3)].clone()
        } else if source == Source::Body {
            "|p/ρg|".to_string()
        } else {
            "|η|".to_string()
        };
        let label_text = format!("{quantity} [{unit}]");
        if level.is_some() {
            // the rows and colours of the chosen depth
            let keep: Vec<bool> = points.iter().map(|(_, p)| at_level(p, level)).collect();
            let mut k = keep.iter();
            rows.retain(|_| *k.next().unwrap_or(&false));
            let mut k = keep.iter();
            shade.retain(|_| *k.next().unwrap_or(&false));
            let mut k = keep.iter();
            points.retain(|_| *k.next().unwrap_or(&false));
        }
        if !shade.is_empty() {
            let positions: Vec<V3> = points.iter().map(|(_, p)| *p).collect();
            overlay = Some(if field {
                Overlay::Field(FieldOverlay { points: positions, values: shade, label: label_text })
            } else {
                let (normals, areas) = if source == Source::Body {
                    r.body_normals_and_areas()
                } else {
                    (vec![[0.0, 0.0, 1.0]; positions.len()], Vec::new())
                };
                let faces = if source == Source::Body { r.body_point_faces() } else { Default::default() };
                Overlay::Body(gui_core::scene::BodyOverlay {
                    points: positions,
                    normals,
                    areas,
                    faces,
                    values: shade,
                    label: label_text,
                    // lids and pressure surfaces are horizontal: triangulated in plan
                    plane: matches!(source, Source::Lid | Source::Surface),
                    cover: r.planes.clone(),
                })
            });
        }
        out.title = format!("{title} · {label} · {} [global]", frequencies[s.frequency]);
        out.table = Some(TableData { columns, rows });
        if source == Source::Body {
            let moving = radiation.then_some("The radiating body moves by a unit displacement of its mode.");
            animation_section(ui, s, None, moving);
        }
    });
    (out, overlay)
}

fn point_row(i: usize, group: &Option<String>, p: V3) -> Vec<Cell> {
    let mut row = vec![Cell::Int(i as i64 + 1)];
    if let Some(g) = group {
        row.push(Cell::Text(g.clone()));
    }
    row.extend(p.iter().map(|x| Cell::Num(*x)));
    row
}

fn shape_text(r: &SolveResult, q: QuantityId) -> String {
    let Some(shape) = r.results.shape(q) else { return String::new() };
    let names: Vec<&str> = q.dims().split(',').filter(|s| !s.is_empty()).collect();
    let dims: Vec<String> = shape
        .dims
        .iter()
        .enumerate()
        .map(|(i, d)| match names.get(i) {
            Some(n) if n.parse::<usize>().is_err() => format!("{n} {d}"),
            _ => d.to_string(),
        })
        .collect();
    format!("{:?} [{}]", shape.element_type, dims.join(" × "))
}

fn summary(ui: &mut Ui, r: &SolveResult) -> Output {
    section(ui, "Solver Results", |ui| {
        read_only(ui, "Route", &r.route);
        read_only(ui, "Frequency Count", &r.omegas().len().to_string());
        read_only(ui, "Heading Count", &r.headings().len().to_string());
        read_only(ui, "Bodies", &r.bodies.len().to_string());
        read_only(ui, "Coordinates (D)", &r.coordinates.to_string());
        read_only(ui, "Radiation Columns (K)", &r.radiation_mode_ids().len().to_string());
        read_only(ui, "Submitted Field Points", &r.field_points.len().to_string());
        if r.engine_seconds > 0.0 {
            read_only(ui, "Engine Time [s]", &format_number(r.engine_seconds));
        }
        read_only(ui, "Threads", &format!("{} frequency × {} inner", r.frequency_threads, r.inner_threads));
    });
    // Engine warnings: the solve succeeded, but the result may not be what is expected.
    if !r.diagnostics.is_empty() {
        let names = &r.model_bodies;
        section(ui, "Engine Warnings", |ui| {
            for d in &r.diagnostics {
                let mut text = d.message.clone();
                let at = location_text(&d.location, names);
                if !at.is_empty() {
                    text.push_str(&format!(" ({at})"));
                }
                let name = if d.severity == Severity::Error { "Error".into() } else { warning_name(d) };
                read_only(ui, &name, &text);
            }
        });
    }
    let rows = r
        .results
        .available()
        .into_iter()
        .map(|q| {
            vec![
                Cell::Int(q as i64),
                Cell::Text(q.name().into()),
                Cell::Text(shape_text(r, q)),
                Cell::Text(q.unit().into()),
                Cell::Text(q.description().into()),
            ]
        })
        .collect();
    Output {
        title: "Available Native Quantities".into(),
        table: Some(TableData {
            columns: ["ID", "Quantity", "Shape", "Unit", "Description"].map(Column::new).to_vec(),
            rows,
        }),
        plot: None,
        tabs: None,
    }
}

/// The hydrostatics the engine computed with the solve, per body: the centres in body
/// coordinates, the stiffness about R.
fn hydrostatics(ui: &mut Ui, r: &SolveResult, s: &mut Selection) -> Output {
    let mut out = Output { title: "Hydrostatics".into(), ..Output::default() };
    section(ui, "Hydrostatics", |ui| {
        if r.hydrostatics.is_empty() {
            message(ui, "This result was not calculated.");
            return;
        }
        let n = r.hydrostatics.len().min(r.bodies.len().max(1));
        s.hydrostatics_body = s.hydrostatics_body.min(n - 1);
        choice(ui, "hydrostatics", "Body", &body_labels(r), &mut s.hydrostatics_body);
        let h = &r.hydrostatics[s.hydrostatics_body];
        let v3 = |v: [f64; 3]| format!("{}, {}, {}", format_number(v[0]), format_number(v[1]), format_number(v[2]));
        read_only(ui, "Axes", "Body coordinates; the stiffness about the rotation centre R");
        read_only(ui, "Displaced Volume [m³]", &format_number(h.displaced_volume));
        read_only(ui, "Volume x, y, z [m³]", &v3(h.volume));
        read_only(ui, "Centre of Buoyancy [m]", &v3(h.center_of_buoyancy));
        read_only(ui, "Waterplane Area [m²]", &format_number(h.waterplane_area));
        read_only(
            ui,
            "Waterplane Centroid [m]",
            &format!("{}, {}", format_number(h.waterplane_centroid[0]), format_number(h.waterplane_centroid[1])),
        );
        if h.mass.is_finite() {
            read_only(ui, "Mass [kg]", &format_number(h.mass));
        }
        if h.has_weight {
            read_only(ui, "Centre of Gravity [m]", &v3(h.center_of_gravity));
            // rho g from the heave stiffness, which has no weight term: C33 = rho g Awp
            let rho_g = if h.waterplane_area > 0.0 { h.stiffness[14] / h.waterplane_area } else { f64::NAN };
            let gm = |c: f64| c / (rho_g * h.displaced_volume);
            read_only(ui, "GM Transverse [m]", &format_number(gm(h.stiffness[21])));
            read_only(ui, "GM Longitudinal [m]", &format_number(gm(h.stiffness[28])));
            read_only(ui, "Stiffness", "with the weight terms of the mass and the centre of gravity");
        } else {
            read_only(ui, "Stiffness", "without weight terms: the body has no mass or centre of gravity");
        }
        let units = ["N/m", "N/m", "N/m", "N·m/rad", "N·m/rad", "N·m/rad"];
        let mut columns = vec![Column::new("Row")];
        columns.extend(DOF_LABELS.iter().map(|d| Column::new(*d)));
        out.title = format!(
            "Hydrostatic Stiffness · {} [6 × 6]",
            r.bodies.get(s.hydrostatics_body).map_or("", |b| b.name.as_str())
        );
        out.table = Some(TableData {
            columns,
            rows: (0..6)
                .map(|i| {
                    std::iter::once(Cell::Text(format!("{} [{}]", DOF_LABELS[i], units[i])))
                        .chain((0..6).map(|j| Cell::Num(h.stiffness[i * 6 + j])))
                        .collect()
                })
                .collect(),
        });
    });
    out
}

fn coefficient(ui: &mut Ui, r: &SolveResult, s: &mut Selection, added_mass: bool) -> Output {
    let title = if added_mass { "Added Mass" } else { "Radiation Damping" };
    let q = if added_mass { QuantityId::AddedMass } else { QuantityId::RadiationDamping };
    let mut out = Output { title: title.into(), ..Output::default() };
    section(ui, title, |ui| {
        if !r.has(q) || r.bodies.is_empty() || r.radiation_mode_ids().is_empty() {
            message(ui, "This result was not calculated.");
            return;
        }
        let n = r.bodies.len();
        s.force_body = s.force_body.min(n - 1);
        s.motion_body = s.motion_body.min(n - 1);
        let labels = body_labels(r);
        read_only(ui, "Quantity", title);
        read_only(ui, "Frequency Count", &r.omegas().len().to_string());
        choice(ui, "coef", "Force Body", &labels, &mut s.force_body);
        let force_dofs = coordinates_of(r, s.force_body, &mut s.force_dof);
        choice(ui, "coef", "Force Direction (Row i)", &force_dofs, &mut s.force_dof);
        choice(ui, "coef", "Motion Body", &labels, &mut s.motion_body);
        let motion_dofs = coordinates_of(r, s.motion_body, &mut s.motion_dof);
        choice(ui, "coef", "Motion Direction (Column j)", &motion_dofs, &mut s.motion_dof);
        let row = r.coordinate_row(s.force_body, s.force_dof).unwrap_or(0);
        let id = r.coordinate_row(s.motion_body, s.motion_dof).unwrap_or(0) + 1;
        let unit = coefficient_unit(added_mass, r.is_rotation(row), r.is_rotation(id - 1));
        let symbol = format!("{}{},{}", if added_mass { "A" } else { "B" }, row + 1, id);
        read_only(ui, "Unit", unit);
        read_only(ui, "Selected Coefficient", &symbol);
        // the limits ω = 0 and ω = ∞ when solved (Environment: Added Mass Limits)
        let limits: Vec<(bool, f64)> = if added_mass {
            [false, true]
                .into_iter()
                .filter_map(|infinite| {
                    r.radiation_column(id).and_then(|c| r.added_mass_limit(infinite, row, c)).map(|v| (infinite, v))
                })
                .collect()
        } else {
            Vec::new()
        };
        for (infinite, v) in &limits {
            let label = if *infinite { "At ω = ∞" } else { "At ω = 0" };
            read_only(ui, &format!("{label} [{unit}]"), &format_number(*v));
        }
        let values = r.radiation_column(id).map(|c| r.coefficient_series(q, row, c)).unwrap_or_default();
        if values.is_empty() {
            read_only(ui, "Status", "The selected motion mode was not solved.");
            return;
        }
        let omegas = r.omegas();
        let header = format!("{symbol} [{unit}]");
        out.title = format!("{title} · {symbol}");
        // the limits after the frequencies (the rows of the frequencies stay the points of the graph)
        let mut rows: Vec<Vec<Cell>> = omegas
            .iter()
            .zip(&values)
            .enumerate()
            .map(|(i, (w, v))| vec![Cell::Int(i as i64 + 1), Cell::Num(*w), Cell::Num(*v)])
            .collect();
        for (infinite, v) in &limits {
            let (name, omega) = if *infinite { ("A(∞)", "∞") } else { ("A(0)", "0") };
            rows.push(vec![Cell::Text(name.into()), Cell::Text(omega.into()), Cell::Num(*v)]);
        }
        out.table = Some(TableData {
            columns: vec![Column::new("Number"), Column::new(FREQUENCY_AXIS), Column::new(header.clone())],
            rows,
        });
        // A(0) a point at ω = 0; A(∞) a point at the right edge of the graph (ω = ∞ has no place on
        // the axis), each named in the legend
        let (lo, hi) = omegas.iter().fold((0.0_f64, f64::MIN), |(a, b), w| (a.min(*w), b.max(*w)));
        let right = hi + 0.08 * (hi - lo).max(hi.abs()).max(1e-9);
        let marks = limits
            .iter()
            .map(|(infinite, v)| PlotMark {
                x: if *infinite { right } else { 0.0 },
                y: *v,
                label: if *infinite { "ω = ∞ (at the right edge)" } else { "ω = 0" }.to_string(),
            })
            .collect();
        out.plot =
            Some(PlotSpec::Single { x_label: FREQUENCY_AXIS.into(), y_label: header, x: omegas, y: values, marks });
        // the whole limit matrices of the two bodies chosen, as tabs
        let matrices: Vec<(String, TableData)> = [(false, "ω = 0 Matrix"), (true, "ω = ∞ Matrix")]
            .into_iter()
            .filter_map(|(infinite, name)| added_mass_limit_table(r, s, infinite).map(|t| (name.to_string(), t)))
            .collect();
        if !matrices.is_empty() {
            let mut names = vec![title.to_string()];
            names.extend(matrices.iter().map(|(n, _)| n.clone()));
            out.tabs = Some(Tabs {
                rows_per_record: vec![1; names.len()],
                names,
                more: matrices.into_iter().map(|(_, t)| t).collect(),
            });
        }
    });
    out
}

fn complex(ui: &mut Ui, r: &SolveResult, s: &mut Selection, q: QuantityId, title: &str) -> Output {
    let motion = q == QuantityId::Motion;
    let mut out = Output { title: title.into(), ..Output::default() };
    section(ui, title, |ui| {
        let headings = heading_labels(r);
        if !r.has(q) || r.bodies.is_empty() || headings.is_empty() {
            message(ui, "This result was not calculated.");
            return;
        }
        s.load_body = s.load_body.min(r.bodies.len() - 1);
        s.heading = s.heading.min(headings.len() - 1);
        read_only(ui, "Quantity", title);
        read_only(ui, "Frequency Count", &r.omegas().len().to_string());
        choice(ui, "loads", if motion { "Body" } else { "Force Body" }, &body_labels(r), &mut s.load_body);
        let load_dofs = coordinates_of(r, s.load_body, &mut s.load_dof);
        choice(ui, "loads", if motion { "Motion Direction" } else { "Force Direction" }, &load_dofs, &mut s.load_dof);
        let rotation = r.coordinate_row(s.load_body, s.load_dof).is_some_and(|row| r.is_rotation(row));
        read_only(
            ui,
            "Magnitude Unit",
            &if motion { r.wave_unit(motion_unit(rotation)) } else { force_unit(rotation).into() },
        );
        read_only(ui, "Phase Unit", "deg");
        choice(ui, "loads", "Wave Heading [deg]", &headings, &mut s.heading);
        let mut graph = usize::from(s.real_imaginary);
        if choice(
            ui,
            "loads",
            "Graph Components",
            &["Magnitude / Phase".to_string(), "Real / Imaginary".to_string()],
            &mut graph,
        ) {
            s.real_imaginary = graph == 1;
        }
        let unit = if motion { r.wave_unit(motion_unit(rotation)) } else { force_unit(rotation).to_string() };
        if q == QuantityId::ExcitationHaskind {
            message(
                ui,
                "The whole exciting force by the Haskind relation, from the radiation of each coordinate \
                 (compare Diffraction, by pressure integration); only the radiated coordinates have it.",
            );
        }
        let series = r
            .coordinate_row(s.load_body, s.load_dof)
            .map(|row| {
                if q == QuantityId::ExcitationHaskind {
                    r.haskind_series(s.heading, row)
                } else {
                    r.fhd_series(q, s.heading, row)
                }
            })
            .unwrap_or_default();
        if series.is_empty() {
            read_only(ui, "Status", "The selected row is unavailable.");
            return;
        }
        let omegas = r.omegas();
        if motion {
            let frequencies: Vec<String> = omegas.iter().map(|w| format!("{} rad/s", format_number(*w))).collect();
            animation_section(ui, s, Some(&frequencies), None);
        }
        let (mag, phase): (Vec<f64>, Vec<f64>) = series.iter().map(|v| (v.norm(), phase_degrees(*v))).unzip();
        let (re, im): (Vec<f64>, Vec<f64>) = series.iter().map(|v| (v.re, v.im)).unzip();
        out.title = format!("{title} · {}", headings[s.heading]);
        out.table = Some(TableData {
            columns: vec![
                Column::new("Number"),
                Column::new(FREQUENCY_AXIS),
                Column::new(format!("Magnitude [{unit}]")),
                Column::new("Phase [deg]"),
                Column::new(format!("Re [{unit}]")),
                Column::new(format!("Im [{unit}]")),
            ],
            rows: (0..series.len())
                .map(|i| {
                    vec![
                        Cell::Int(i as i64 + 1),
                        Cell::Num(omegas[i]),
                        Cell::Num(mag[i]),
                        Cell::Fixed(phase[i], 3),
                        Cell::Num(re[i]),
                        Cell::Num(im[i]),
                    ]
                })
                .collect(),
        });
        out.plot = Some(if s.real_imaginary {
            PlotSpec::Paired {
                x_label: FREQUENCY_AXIS.into(),
                top_label: format!("Re [{unit}]"),
                bottom_label: format!("Im [{unit}]"),
                x: omegas,
                top: re,
                bottom: im,
                phase: false,
            }
        } else {
            PlotSpec::Paired {
                x_label: FREQUENCY_AXIS.into(),
                top_label: format!("Magnitude [{unit}]"),
                bottom_label: "Phase [deg]".into(),
                x: omegas,
                top: mag,
                bottom: phase,
                phase: true,
            }
        });
    });
    out
}

fn free_surface(
    ui: &mut Ui,
    r: &SolveResult,
    s: &mut Selection,
    kind: FieldKind,
    smoothing: &mut bool,
) -> (Output, Option<FieldOverlay>) {
    let title = crate::tree::result_title(ResultNode::Field(kind));
    // below z = 0 the engine gives the pressure p / (rho g) at the points, not an elevation
    let mut below = r.field_grid.z < 0.0;
    let section_title =
        if below { format!("Field Pressure p/ρg · {title}") } else { format!("Free-Surface {title} Amplitude") };
    let mut out = Output { title: section_title.clone(), ..Output::default() };
    let mut overlay = None;
    section(ui, &section_title, |ui| {
        let radiation = kind == FieldKind::Radiation;
        s.showing_radiation = radiation;
        let ids = r.radiation_mode_ids();
        let headings = heading_labels(r);
        let omegas = r.omegas();
        if !r.has_field_results() || omegas.is_empty() || (if radiation { ids.is_empty() } else { headings.is_empty() })
        {
            message(ui, "No field result was calculated.");
            return;
        }
        let unit = if radiation { "m per unit displacement" } else { "m" };
        read_only(
            ui,
            "Quantity",
            &if below {
                format!("{title} |p/ρg| at z = {}", format_number(r.field_grid.z))
            } else {
                format!("{title} Amplitude")
            },
        );
        read_only(ui, "Unit", unit);
        check(
            ui,
            "Interpolate",
            smoothing,
            Some(
                "Colours between the field points by bilinear interpolation (off: one colour per cell, the mean of its four points). Display only; it adds no solver samples.",
            ),
        );
        let frequencies: Vec<String> = omegas.iter().map(|w| format!("{} rad/s", format_number(*w))).collect();
        s.frequency = s.frequency.min(frequencies.len() - 1);
        choice(ui, "field", "Wave Frequency [rad/s]", &frequencies, &mut s.frequency);
        let mut columns = None;
        let (case, case_label) = if radiation {
            let Some(chosen) = radiation_mode_choice(ui, "field", r, s) else { return };
            chosen
        } else {
            s.heading = s.heading.min(headings.len() - 1);
            choice(ui, "field", "Wave Heading [deg]", &headings, &mut s.heading);
            let mut label = headings[s.heading].clone();
            if kind == FieldKind::Total {
                let (chosen, contribution) = total_contribution(ui, r, s);
                columns = chosen;
                label = format!("{label} · {contribution}");
                if !r.has(QuantityId::Motion) {
                    read_only(ui, "Dynamic Total", "Motion was not solved; showing fixed-body excitation.");
                }
            }
            (s.heading, label)
        };
        let level = field_level(ui, r, s, "field");
        if let Some(z) = level {
            below = z < 0.0;
        }
        read_only(ui, "Submitted Points", &r.field_points.len().to_string());
        read_only(ui, "Excluded Points", &r.generated_field_points.saturating_sub(r.field_points.len()).to_string());
        let all = r.field_amplitude(kind, s.frequency, case, columns.as_deref());
        let (points, values): (Vec<V3>, Vec<f64>) =
            r.field_points.iter().zip(all).filter(|(p, _)| at_level(p, level)).map(|(p, v)| (*p, v)).unzip();
        if !values.is_empty() {
            let (lo, hi) = values.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(*v), b.max(*v)));
            read_only(ui, "Amplitude Range [m]", &format!("{} … {}", format_number(lo), format_number(hi)));
        }
        out.title = format!("{section_title} · {} · {case_label}", frequencies[s.frequency]);
        out.table = Some(TableData {
            columns: ["Number", "X [m]", "Y [m]", "Z [m]", if below { "|p/ρg| [m]" } else { "Amplitude [m]" }]
                .map(Column::new)
                .to_vec(),
            rows: points
                .iter()
                .zip(&values)
                .enumerate()
                .map(|(i, (p, v))| {
                    vec![Cell::Int(i as i64 + 1), Cell::Num(p[0]), Cell::Num(p[1]), Cell::Num(p[2]), Cell::Num(*v)]
                })
                .collect(),
        });
        let quantity = if below { format!("{title} |p/ρg|") } else { title.to_string() };
        overlay = Some(FieldOverlay { points, values, label: format!("{quantity} [{unit}]") });
        if r.field_grid.list.is_none() {
            let moving = match kind {
                FieldKind::Radiation => Some("The radiating body moves by a unit displacement of its mode."),
                FieldKind::Total if r.has(QuantityId::Motion) => {
                    Some("The bodies whose radiation the Total adds move by their motions (RAO).")
                }
                _ => None,
            };
            animation_section(ui, s, None, moving);
        }
    });
    (out, overlay)
}
