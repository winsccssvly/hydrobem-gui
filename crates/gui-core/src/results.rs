//! The results of a solve and their views (docs/RESULTS.md): the engine's `Results` with the
//! bodies laid out on the coordinate axis D, and the indexing of the documented shapes
//! (coefficients [F][D][K], wave loads and motions [F][H][D], field elevations [F][P][K|H]).
//! Dimensions always come from `Results::shape`, never from the length of the data.

use std::collections::HashMap;
use std::sync::Arc;

use hydrobem_engine::{Complex32, Complex64, Diagnostic, ElementType, Model, QuantityId, Results};

use crate::field_points::PointSource;
use crate::project::{FieldPoints, V3};

/// A panel of the solved bodies as the engine built it.
#[derive(Clone, Debug)]
pub struct SolvedPanel {
    /// Zero-based over the solved bodies.
    pub body: i32,
    /// One-based element of its mesh.
    pub element: i32,
    pub reflection: [i32; 2],
    pub quad: [V3; 4],
    pub centroid: V3,
}

/// The panels of the bodies of a model, as it is solved (every symmetry copy).
pub fn solved_panels(m: &mut Model) -> Vec<SolvedPanel> {
    let Ok(p) = m.panels(None, true) else { return Vec::new() };
    (0..p.body.len())
        .filter(|&k| p.body[k] >= 0)
        .map(|k| SolvedPanel {
            body: p.body[k],
            element: p.element[k],
            reflection: p.reflection[k],
            quad: p.vertices[k],
            centroid: p.centroids[k],
        })
        .collect()
}

/// The horizontal output surfaces as solved (global, every symmetry copy): the pressure surfaces
/// and damping lids by the engine's tessellation (a panel one quadrilateral, a patch several on its
/// exact surface). Their values are drawn between the output points over these surfaces only.
pub fn solved_planes(m: &mut Model) -> Arc<Vec<[V3; 4]>> {
    const PLANES: &str = "pressure_surface,lid";
    let Ok(p) = m.panels(Some(PLANES), true) else { return Arc::default() };
    if p.is_empty() {
        return Arc::default();
    }
    let n = surface_steps(p.len());
    match m.tessellation(Some(PLANES), true, n, n) {
        Ok(t) => Arc::new(t.vertices),
        Err(_) => Arc::new(p.vertices),
    }
}

/// Per body output point the faces it colours (`SolveResult::body_point_faces`).
pub type PointFaces = Arc<Vec<Vec<[V3; 4]>>>;

/// A quadrilateral of a patch's tessellation as solved (global, every symmetry copy): the faces
/// the higher-order body outputs are coloured on.
#[derive(Clone, Debug, PartialEq)]
pub struct SolvedQuad {
    /// Zero-based over the solved bodies.
    pub body: i32,
    pub reflection: [i32; 2],
    pub quad: [V3; 4],
}

/// The quadrilaterals per patch side of `solved_surface`: finer than the output cells of the usual
/// discretizations (16 per side for 4 spans of order 3), about 60 000 in all at most.
fn surface_steps(patches: usize) -> i32 {
    ((60_000.0 / patches.max(1) as f64).sqrt().floor() as i32).clamp(8, 48)
}

/// The wetted and pressure-surface patches of the bodies of a model solved by the higher-order
/// method, tessellated (`Model::tessellation`); empty for the low order.
pub fn solved_surface(m: &mut Model) -> Vec<SolvedQuad> {
    let higher = m
        .object("options")
        .ok()
        .and_then(|o| m.get(o, "higher_order").ok().flatten())
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !higher {
        return Vec::new();
    }
    const OUTPUTS: &str = "wetted,pressure_surface";
    let Ok(p) = m.panels(Some(OUTPUTS), true) else { return Vec::new() };
    let n = surface_steps(p.len());
    let Ok(t) = m.tessellation(Some(OUTPUTS), true, n, n) else { return Vec::new() };
    (0..t.len())
        .filter(|&k| t.body[k] >= 0)
        .map(|k| SolvedQuad { body: t.body[k], reflection: t.reflection[k], quad: t.vertices[k] })
        .collect()
}

/// Whether a model asks for the bidirectional drifts.
pub fn asks_bidirectional_drift(m: &Model) -> bool {
    m.object("second_order")
        .ok()
        .and_then(|s| m.get(s, "drift_bidirectional").ok().flatten())
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// A body of the solve on the D axis (`QuantityId::Coordinates`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SolvedBody {
    pub name: String,
    /// The first row of this body on the D axis.
    pub coordinate_offset: usize,
    /// Pressure coordinates following its six rigid ones.
    pub pressure_modes: usize,
}

/// The hydrostatics of one solved body, SI: the centres and the waterplane centroid in body
/// coordinates, the stiffness about R.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyHydrostatics {
    /// The three estimates of the divergence theorem.
    pub volume: [f64; 3],
    /// Their mean, the displaced volume.
    pub displaced_volume: f64,
    pub center_of_buoyancy: V3,
    pub waterplane_area: f64,
    pub waterplane_centroid: [f64; 2],
    pub stiffness_rho_g: Vec<f64>,
    pub has_weight: bool,
    pub mass: f64,
    /// Only with `has_weight`.
    pub center_of_gravity: V3,
    pub stiffness: Vec<f64>,
}

#[derive(Clone)]
pub struct SolveResult {
    /// The engine's results with the model solved (what a results file is written from).
    pub results: Arc<Results>,
    pub route: String,
    /// The active bodies in submission order.
    pub bodies: Vec<SolvedBody>,
    /// Every body of the model solved, the suppressed ones too: the bodies of the warnings'
    /// locations.
    pub model_bodies: Vec<String>,
    /// The panels of the bodies as solved (global, every symmetry copy): the faces the body
    /// outputs are coloured on.
    pub panels: Vec<SolvedPanel>,
    /// Higher order: the tessellated patches the body outputs are coloured on.
    pub surface: Vec<SolvedQuad>,
    /// The pressure surfaces and lids as solved (`solved_planes`): where their values are drawn.
    pub planes: Arc<Vec<[V3; 4]>>,
    /// `body_point_faces`, made once (after `panels` and `surface` are set).
    point_faces: Arc<std::sync::OnceLock<PointFaces>>,
    /// D, the rows of the coordinates.
    pub coordinates: usize,
    /// The grid at solve time.
    pub field_grid: FieldPoints,
    /// The submitted points, after the engine's exclusions (global, grid order).
    pub field_points: Vec<V3>,
    /// Where the results of each submitted point are: the points in the sea, or those of a tank.
    pub field_sources: Vec<Option<PointSource>>,
    /// The model solved asked for the bidirectional drifts (`second_order/drift_bidirectional`):
    /// with one heading its pairs are those of the unidirectional ones.
    pub bidirectional_drift: bool,
    /// Grid points before the exclusions.
    pub generated_field_points: usize,
    /// The engine's warnings of the model and of the solve.
    pub diagnostics: Vec<Diagnostic>,
    pub hydrostatics: Vec<BodyHydrostatics>,
    /// The engine's clock.
    pub engine_seconds: f64,
    /// The whole solve call; 0 for results read from a file.
    pub total_seconds: f64,
    pub frequency_threads: i32,
    pub inner_threads: i32,
    /// The incident wave amplitude the values are shown for (m): the engine's results are per unit
    /// amplitude, and the quantities driven by the waves scale with it (`wave_scale`). A display
    /// choice: results files keep the engine's values.
    pub wave_amplitude: f64,
}

impl SolveResult {
    /// Lays out `results` of a model whose bodies are `body_names` (in order) and whose field
    /// points are `field_points` (global), their results where `field_sources` says.
    pub fn new(
        results: Results,
        body_names: &[String],
        field_points: Vec<V3>,
        field_sources: Vec<Option<PointSource>>,
    ) -> SolveResult {
        let meta = results.meta().clone();
        let mut out = SolveResult {
            route: results.route().to_string(),
            diagnostics: results.diagnostics().to_vec(),
            bodies: body_names.iter().map(|n| SolvedBody { name: n.clone(), ..SolvedBody::default() }).collect(),
            model_bodies: body_names.to_vec(),
            panels: Vec::new(),
            surface: Vec::new(),
            planes: Arc::default(),
            point_faces: Arc::default(),
            coordinates: 0,
            field_grid: FieldPoints::default(),
            field_points,
            field_sources,
            bidirectional_drift: false,
            generated_field_points: 0,
            hydrostatics: Vec::new(),
            engine_seconds: meta.engine_seconds.iter().sum(),
            total_seconds: 0.0,
            frequency_threads: meta.frequency_threads,
            inner_threads: meta.inner_threads,
            wave_amplitude: 1.0,
            results: Arc::new(results),
        };
        out.lay_out_bodies();
        out.read_hydrostatics();
        out
    }

    /// The bodies on the D axis (rows of body, kind 0 rigid / 1 generalized / 2 pressure, index).
    fn lay_out_bodies(&mut self) {
        // the table [rows][4], by its shape
        let Some(rows) = self.dims(QuantityId::Coordinates).filter(|d| d.len() == 2 && d[1] == 4).map(|d| d[0]) else {
            return;
        };
        let data = self.results.i32s(QuantityId::Coordinates).unwrap_or(&[]).to_vec();
        self.coordinates = rows;
        let mut seen = vec![false; self.bodies.len()];
        for (r, row) in data.as_chunks::<4>().0.iter().enumerate() {
            let Ok(body) = usize::try_from(row[0]) else { continue };
            if body >= self.bodies.len() {
                continue;
            }
            if !seen[body] {
                seen[body] = true;
                self.bodies[body].coordinate_offset = r;
            }
            if row[1] == 2 {
                self.bodies[body].pressure_modes += 1;
            }
        }
    }

    /// The hydrostatics the engine computes with every solve.
    fn read_hydrostatics(&mut self) {
        let r = &self.results;
        let get = |q: QuantityId| r.f64s(q).map(<[f64]>::to_vec).unwrap_or_default();
        let mean = get(QuantityId::HydrostaticsVolume);
        let (volume, buoyancy, area, centroid) = (
            get(QuantityId::HydrostaticsVolumes),
            get(QuantityId::HydrostaticsCenterOfBuoyancy),
            get(QuantityId::HydrostaticsWaterplaneArea),
            get(QuantityId::HydrostaticsWaterplaneCentroid),
        );
        let (rho_g, mass, gravity, stiffness) = (
            get(QuantityId::HydrostaticsStiffnessRhoG),
            get(QuantityId::HydrostaticsMass),
            get(QuantityId::HydrostaticsCenterOfGravity),
            get(QuantityId::HydrostaticsStiffness),
        );
        // per body: the shapes the engine reports, [n], [n][3] and [n][6][6]
        let Some(n) = self.dims(QuantityId::HydrostaticsMass).filter(|d| d.len() == 1).map(|d| d[0]) else { return };
        let shaped = |q: QuantityId, tail: &[usize]| self.dims(q).is_some_and(|d| d[0] == n && d[1..] == *tail);
        let threes = [QuantityId::HydrostaticsVolumes, QuantityId::HydrostaticsCenterOfBuoyancy];
        if !threes.iter().all(|&q| shaped(q, &[3]))
            || !shaped(QuantityId::HydrostaticsWaterplaneArea, &[])
            || !shaped(QuantityId::HydrostaticsWaterplaneCentroid, &[2])
            || !shaped(QuantityId::HydrostaticsCenterOfGravity, &[3])
            || !shaped(QuantityId::HydrostaticsStiffnessRhoG, &[6, 6])
            || !shaped(QuantityId::HydrostaticsStiffness, &[6, 6])
            || mass.len() != n
            || mean.len() != n
        {
            return;
        }
        for b in 0..n {
            let has_weight = !gravity[3 * b].is_nan();
            self.hydrostatics.push(BodyHydrostatics {
                volume: [volume[3 * b], volume[3 * b + 1], volume[3 * b + 2]],
                displaced_volume: mean[b],
                center_of_buoyancy: [buoyancy[3 * b], buoyancy[3 * b + 1], buoyancy[3 * b + 2]],
                waterplane_area: area[b],
                waterplane_centroid: [centroid[2 * b], centroid[2 * b + 1]],
                stiffness_rho_g: rho_g[36 * b..36 * b + 36].to_vec(),
                has_weight,
                mass: mass[b],
                center_of_gravity: if has_weight {
                    [gravity[3 * b], gravity[3 * b + 1], gravity[3 * b + 2]]
                } else {
                    [0.0; 3]
                },
                stiffness: stiffness[36 * b..36 * b + 36].to_vec(),
            });
        }
    }

    pub fn has(&self, q: QuantityId) -> bool {
        self.results.has(q)
    }

    /// These results shown for incident waves of amplitude `amplitude` (m).
    pub fn with_wave_amplitude(&self, amplitude: f64) -> SolveResult {
        SolveResult { wave_amplitude: amplitude, ..self.clone() }
    }

    /// The factor of `q`'s values at the wave amplitude A: by the engine's quantity table, a
    /// quantity over frequency and heading is per metre of wave amplitude (A), the mean drifts per
    /// square metre (A²); the others (added mass, damping, the radiation per unit motion, ...) do
    /// not depend on the waves (1).
    pub fn wave_scale(&self, q: QuantityId) -> f64 {
        let axes: Vec<&str> = q.dims().split(',').map(str::trim).collect();
        // a heading axis: the headings, or the heading pairs of the drifts and the QTFs
        let heading = axes.iter().any(|a| matches!(*a, "heading" | "drift_heading_pair" | "qtf_heading_pair"));
        if !(axes.contains(&"frequency") && heading) {
            return 1.0;
        }
        let a = self.wave_amplitude;
        if q.unit().contains("per m^2") { a * a } else { a }
    }

    /// A unit per unit wave amplitude ("m/m", "N/m²", "m³/s per m") as these results show it: as it
    /// is at A = 1 m, else per nothing ("m", "N", "m³/s"): the values are those of the waves.
    pub fn wave_unit(&self, per_amplitude: &str) -> String {
        if self.wave_amplitude == 1.0 {
            return per_amplitude.to_string();
        }
        if let Some(head) = per_amplitude.strip_suffix(" per m per rad") {
            return format!("{head} per rad");
        }
        for suffix in ["/m²", "/m", " per m"] {
            if let Some(head) = per_amplitude.strip_suffix(suffix) {
                return head.to_string();
            }
        }
        per_amplitude.to_string()
    }

    /// The dimensions of `q`, as the engine reports them.
    pub fn dims(&self, q: QuantityId) -> Option<Vec<usize>> {
        self.results.shape(q).map(|s| s.dims)
    }

    pub fn omegas(&self) -> Vec<f64> {
        self.results.f64s(QuantityId::Omegas).map(<[f64]>::to_vec).unwrap_or_default()
    }

    pub fn headings(&self) -> Vec<f64> {
        self.results.f64s(QuantityId::Headings).map(<[f64]>::to_vec).unwrap_or_default()
    }

    pub fn radiation_mode_ids(&self) -> Vec<i32> {
        self.results.i32s(QuantityId::RadiationModeIds).map(<[i32]>::to_vec).unwrap_or_default()
    }

    /// The row of (body, local dof 0..5) on the D axis.
    pub fn coordinate_row(&self, body: usize, dof: usize) -> Option<usize> {
        let rows = self.body_coordinates(body);
        match rows.get(dof) {
            Some((row, _)) => Some(*row),
            // a body without rows on the D axis has none (its offset would be another body's)
            None if rows.is_empty() => None,
            None => (dof < 6).then(|| self.bodies.get(body).map(|b| b.coordinate_offset + dof)).flatten(),
        }
    }

    /// The coordinates of a body on the D axis, in order: its row and label (Surge ... Yaw, the
    /// generalized modes G1, G2, ..., the pressure modes P1, P2, ...).
    pub fn body_coordinates(&self, body: usize) -> Vec<(usize, String)> {
        let data = self.results.i32s(QuantityId::Coordinates).unwrap_or(&[]);
        data.as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, row)| row[0] as usize == body)
            .map(|(r, row)| {
                let k = row[2].max(1) as usize;
                let label = match row[1] {
                    0 => crate::project::DOF_LABELS.get(k - 1).map_or_else(|| format!("Rigid {k}"), |s| s.to_string()),
                    1 => format!("Generalized {k}"),
                    _ => format!("Pressure {k}"),
                };
                (r, label)
            })
            .collect()
    }

    /// Whether the coordinate of row `row` on the D axis is a rotation (the engine's coordinates;
    /// a generalized or pressure coordinate is not: a length, normalized as a translation).
    pub fn is_rotation(&self, row: usize) -> bool {
        match self.results.i32s(QuantityId::Coordinates).ok().and_then(|d| d.as_chunks::<4>().0.get(row).copied()) {
            Some(c) => c[3] == 1,
            None => (3..6).contains(&(row % 6)),
        }
    }

    /// A label for every coordinate: the body and its coordinate.
    pub fn coordinate_labels(&self) -> Vec<String> {
        let mut labels = vec![String::new(); self.coordinates];
        for (b, body) in self.bodies.iter().enumerate() {
            for (r, label) in self.body_coordinates(b) {
                if let Some(slot) = labels.get_mut(r) {
                    *slot = format!("{} {label}", body.name);
                }
            }
        }
        labels
    }

    /// Per column of the K axis (a radiation mode): the solved body it moves and its coordinate's
    /// label ("Heave", "Generalized 1", ...), from the engine's coordinate table.
    pub fn radiation_modes(&self) -> Vec<(usize, String)> {
        let table = self.results.i32s(QuantityId::Coordinates).unwrap_or(&[]);
        let rows = table.as_chunks::<4>().0;
        self.radiation_mode_ids()
            .iter()
            .map(|id| {
                let row = (*id as usize).saturating_sub(1);
                let body = rows
                    .get(row)
                    .map(|c| c[0] as usize)
                    .or_else(|| self.bodies.iter().rposition(|b| b.coordinate_offset <= row))
                    .unwrap_or(0);
                let label = self
                    .body_coordinates(body)
                    .into_iter()
                    .find(|(r, _)| *r == row)
                    .map_or_else(|| format!("Coordinate {}", row + 1), |(_, l)| l);
                (body, label)
            })
            .collect()
    }

    /// The rigid motion of solved body `body` (surge ... yaw, body axes about R, per metre of wave
    /// amplitude) at one frequency and heading; zero where it was not solved.
    pub fn rigid_motion(&self, body: usize, frequency: usize, heading: usize) -> [Complex64; 6] {
        std::array::from_fn(|dof| {
            self.coordinate_row(body, dof)
                .and_then(|row| self.fhd_series(QuantityId::Motion, heading, row).get(frequency).copied())
                .unwrap_or_default()
        })
    }

    /// The column on the K axis of a one-based coordinate id, `None` when that mode was not solved.
    pub fn radiation_column(&self, coordinate_id: usize) -> Option<usize> {
        self.radiation_mode_ids().iter().position(|id| *id as usize == coordinate_id)
    }

    /// Added mass or radiation damping [F][D][K] for one (row, column) over frequency.
    pub fn coefficient_series(&self, q: QuantityId, row: usize, column: usize) -> Vec<f64> {
        let (Some(d), Ok(data)) = (self.dims(q), self.results.f32s(q)) else { return Vec::new() };
        if d.len() != 3 || row >= d[1] || column >= d[2] {
            return Vec::new();
        }
        (0..d[0]).map(|f| f64::from(data[(f * d[1] + row) * d[2] + column])).collect()
    }

    /// A complex [F][H][D] quantity (wave loads, motions) for one (heading, row) over frequency.
    pub fn fhd_series(&self, q: QuantityId, heading: usize, row: usize) -> Vec<Complex64> {
        let (Some(d), Ok(data)) = (self.dims(q), self.results.complex32s(q)) else { return Vec::new() };
        if d.len() != 3 || heading >= d[1] || row >= d[2] {
            return Vec::new();
        }
        let k = self.wave_scale(q);
        (0..d[0]).map(|f| widen(data[(f * d[1] + heading) * d[2] + row]) * k).collect()
    }

    /// The exciting force by the Haskind relation (`excitation.haskind`, [F][H][K]: its last axis
    /// the radiation columns, not the coordinates) for one heading and coordinate row over
    /// frequency; empty when the row's mode was not radiated or the result is not there.
    pub fn haskind_series(&self, heading: usize, row: usize) -> Vec<Complex64> {
        match self.radiation_column(row + 1) {
            Some(column) => self.fhd_series(QuantityId::ExcitationHaskind, heading, column),
            None => Vec::new(),
        }
    }

    /// The heading pairs of the mean drifts (deg; `DriftHeadingPairs`): the pair (β, β) of every
    /// heading, and with the bidirectional drifts every pair β_a ≤ β_b.
    pub fn drift_heading_pairs(&self) -> Vec<[f64; 2]> {
        self.results.f64s(QuantityId::DriftHeadingPairs).map(|p| p.as_chunks::<2>().0.to_vec()).unwrap_or_default()
    }

    /// Whether the drifts are the bidirectional ones (pairs of two headings).
    pub fn drift_bidirectional(&self) -> bool {
        self.bidirectional_drift || self.drift_heading_pairs().iter().any(|p| p[0] != p[1])
    }

    /// The pair of the drifts of heading `heading` with itself.
    fn drift_pair(&self, heading: usize) -> Option<usize> {
        let h = *self.headings().get(heading)?;
        self.drift_heading_pairs().iter().position(|p| p[0] == h && p[1] == h)
    }

    /// A complex mean drift [F][P][C] (a heading pair of the drifts) at every frequency.
    fn drift_values(&self, q: QuantityId, pair: usize, component: usize) -> Vec<Complex64> {
        let Some(shape) = self.results.shape(q) else { return Vec::new() };
        let d = &shape.dims;
        if d.len() != 3 || pair >= d[1] || component >= d[2] {
            return Vec::new();
        }
        let at = |f: usize| (f * d[1] + pair) * d[2] + component;
        let k = self.wave_scale(q);
        match shape.element_type {
            ElementType::Complex128 => {
                self.results.complex64s(q).map(|v| (0..d[0]).map(|f| v[at(f)] * k).collect()).unwrap_or_default()
            }
            ElementType::Complex64 => {
                self.results.complex32s(q).map(|v| (0..d[0]).map(|f| widen(v[at(f)]) * k).collect()).unwrap_or_default()
            }
            ElementType::Float64 => self
                .results
                .f64s(q)
                .map(|v| (0..d[0]).map(|f| Complex64::new(v[at(f)] * k, 0.0)).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// A mean drift [F][P][C] for one heading with itself and one component (a coordinate row, or
    /// a column of the global or body-axes drifts) over frequency: real, as the drift of equal
    /// headings is.
    pub fn drift_series(&self, q: QuantityId, heading: usize, component: usize) -> Vec<f64> {
        let Some(pair) = self.drift_pair(heading) else { return Vec::new() };
        self.drift_values(q, pair, component).iter().map(|z| z.re).collect()
    }

    /// The values of a field-point quantity of the sea [F][P][C] (or [F][P][C][3]) at the
    /// submitted points in grid order, unscaled: a point in the sea from `q`, one in a tank from
    /// its tank's part of the tank quantity (radiation only; the waves do not reach a tank's
    /// fluid, so its incident, scattering and diffraction parts are zero). None when the results
    /// have neither.
    fn field_values(&self, q: QuantityId, frequency: usize, case: usize) -> Option<Vec<[Complex64; 3]>> {
        let vector = q.dims().split(',').count() == 4;
        let zero = Complex64::new(0.0, 0.0);
        let read = |d: &[usize], data: &[Complex32], p: usize| -> [Complex64; 3] {
            if vector {
                let at = ((frequency * d[1] + p) * d[2] + case) * 3;
                [widen(data[at]), widen(data[at + 1]), widen(data[at + 2])]
            } else {
                [widen(data[(frequency * d[1] + p) * d[2] + case]), zero, zero]
            }
        };
        let fits = |d: &[usize]| d.len() == if vector { 4 } else { 3 } && frequency < d[0] && case < d[2];
        let sea = self.dims(q).filter(|d| fits(d)).zip(self.results.complex32s(q).ok());
        let tank_view = match q {
            QuantityId::FieldPointsPressureRadiation => Some("tanks.pressure_radiation"),
            QuantityId::FieldPointsVelocityRadiation => Some("tanks.velocity_radiation"),
            _ => None,
        };
        let mut tanks: HashMap<(usize, usize), Option<TankPart>> = HashMap::new();
        let any_tank = self.field_sources.iter().any(|s| matches!(s, Some(PointSource::Tank { .. })));
        if sea.is_none() && !(any_tank && tank_view.is_some()) {
            return None;
        }
        Some(
            self.field_sources
                .iter()
                .map(|source| match source {
                    Some(PointSource::Sea(p)) => match &sea {
                        Some((d, data)) if *p < d[1] => read(d, data, *p),
                        _ => [zero; 3],
                    },
                    Some(PointSource::Tank { body, tank, index }) => {
                        let Some(view) = tank_view else { return [zero; 3] };
                        let part = tanks.entry((*body, *tank)).or_insert_with(|| {
                            self.results.select_complex32(view, *body, *tank).ok().flatten().filter(|(d, _)| fits(d))
                        });
                        match part {
                            Some((d, data)) if *index < d[1] => read(d, data, *index),
                            _ => [zero; 3],
                        }
                    }
                    None => [zero; 3],
                })
                .collect(),
        )
    }

    /// Whether `q` is a quantity at the field points (laid out by `field_values`).
    fn is_field(q: QuantityId) -> bool {
        q.name().starts_with("field_points_") && q != QuantityId::FieldPoints
    }

    /// A complex [F][P][C] quantity (pressures at points) at one frequency and case (a heading or a
    /// radiation column), for every point.
    pub fn point_values(&self, q: QuantityId, frequency: usize, case: usize) -> Vec<Complex64> {
        if Self::is_field(q) {
            let k = self.wave_scale(q);
            return self.field_values(q, frequency, case).unwrap_or_default().iter().map(|v| v[0] * k).collect();
        }
        let (Some(d), Ok(data)) = (self.dims(q), self.results.complex32s(q)) else { return Vec::new() };
        if d.len() != 3 || frequency >= d[0] || case >= d[2] {
            return Vec::new();
        }
        let k = self.wave_scale(q);
        (0..d[1]).map(|p| widen(data[(frequency * d[1] + p) * d[2] + case]) * k).collect()
    }

    /// A complex [F][P][C][3] quantity (velocities at points) at one frequency and case, for every
    /// point: the x, y, z components.
    pub fn point_vectors(&self, q: QuantityId, frequency: usize, case: usize) -> Vec<[Complex64; 3]> {
        if Self::is_field(q) {
            let k = self.wave_scale(q);
            return self
                .field_values(q, frequency, case)
                .unwrap_or_default()
                .iter()
                .map(|v| v.map(|c| c * k))
                .collect();
        }
        let (Some(d), Ok(data)) = (self.dims(q), self.results.complex32s(q)) else { return Vec::new() };
        if d.len() != 4 || d[3] != 3 || frequency >= d[0] || case >= d[2] {
            return Vec::new();
        }
        let k = self.wave_scale(q);
        (0..d[1])
            .map(|p| {
                let at = ((frequency * d[1] + p) * d[2] + case) * 3;
                [widen(data[at]) * k, widen(data[at + 1]) * k, widen(data[at + 2]) * k]
            })
            .collect()
    }

    /// The unit normals into the fluid at the body output points (global), and their areas.
    pub fn body_normals_and_areas(&self) -> (Vec<V3>, Vec<f64>) {
        let normals =
            self.results.f64s(QuantityId::BodyPointsNormals).map(|n| n.as_chunks::<3>().0.to_vec()).unwrap_or_default();
        let areas = self.results.f64s(QuantityId::BodyPointsAreas).map(<[f64]>::to_vec).unwrap_or_default();
        (normals, areas)
    }

    /// A bidirectional mean drift at one frequency and component as the matrix of the headings:
    /// rows the first heading, columns the second. The engine gives the pairs β_a ≤ β_b; the pair
    /// (β_b, β_a) is its complex conjugate (A_b A_a* = conj(A_a A_b*)).
    pub fn drift_pairs(&self, q: QuantityId, frequency: usize, component: usize) -> Vec<Vec<Complex64>> {
        let headings = self.headings();
        let pairs = self.drift_heading_pairs();
        if frequency >= self.omegas().len() || pairs.is_empty() {
            return Vec::new();
        }
        let index = |h: f64| headings.iter().position(|x| *x == h);
        let mut matrix = vec![vec![Complex64::new(f64::NAN, f64::NAN); headings.len()]; headings.len()];
        for (p, pair) in pairs.iter().enumerate() {
            let (Some(a), Some(b)) = (index(pair[0]), index(pair[1])) else { continue };
            let Some(z) = self.drift_values(q, p, component).get(frequency).copied() else { return Vec::new() };
            matrix[a][b] = z;
            matrix[b][a] = z.conj();
        }
        matrix
    }

    /// The heading pairs of the QTFs (deg; `QtfHeadingPairs`): (β, β) of every heading, and with
    /// the bidirectional QTFs every pair β_a ≤ β_b.
    pub fn qtf_heading_pairs(&self) -> Vec<[f64; 2]> {
        self.results.f64s(QuantityId::QtfHeadingPairs).map(|p| p.as_chunks::<2>().0.to_vec()).unwrap_or_default()
    }

    /// The sum of QTFs of one frequency kind: the total of a quadratic part and a potential part
    /// (HydroBEM: "the total is the sum of a quadratic part and a potential part"), which the
    /// engine does not compute itself. Empty when a part is missing or their matrices differ in
    /// size. One quantity alone is `qtf_matrix`.
    pub fn qtf_sum_of(&self, parts: &[QuantityId], pair: usize, component: usize) -> Vec<Vec<Complex64>> {
        let mut parts = parts.iter();
        let Some(first) = parts.next() else { return Vec::new() };
        let mut total = self.qtf_matrix(*first, pair, component);
        for q in parts {
            let m = self.qtf_matrix(*q, pair, component);
            if m.len() != total.len() || m.iter().zip(&total).any(|(a, b)| a.len() != b.len()) {
                return Vec::new();
            }
            for (row, add) in total.iter_mut().zip(&m) {
                for (z, w) in row.iter_mut().zip(add) {
                    *z += w;
                }
            }
        }
        total
    }

    /// A QTF [F][F][P][C] (complex: the quadratic and potential parts by heading pair) or a
    /// Newman approximation [F][F][H][C] (real: by heading) as the matrix of the frequencies for
    /// one pair (or heading) and one component (a coordinate row, or a global column): rows ω_a,
    /// columns ω_b, scaled to the wave amplitude squared. Empty when the results do not have it.
    pub fn qtf_matrix(&self, q: QuantityId, pair: usize, component: usize) -> Vec<Vec<Complex64>> {
        let Some(shape) = self.results.shape(q) else { return Vec::new() };
        let d = &shape.dims;
        if d.len() != 4 || d[0] != d[1] || pair >= d[2] || component >= d[3] {
            return Vec::new();
        }
        let at = |a: usize, b: usize| ((a * d[1] + b) * d[2] + pair) * d[3] + component;
        let k = self.wave_scale(q);
        let matrix = |value: &dyn Fn(usize) -> Complex64| -> Vec<Vec<Complex64>> {
            (0..d[0]).map(|a| (0..d[1]).map(|b| value(at(a, b)) * k).collect()).collect()
        };
        match shape.element_type {
            ElementType::Complex128 => match self.results.complex64s(q) {
                Ok(v) => matrix(&|i| v[i]),
                Err(_) => Vec::new(),
            },
            ElementType::Complex64 => match self.results.complex32s(q) {
                Ok(v) => matrix(&|i| widen(v[i])),
                Err(_) => Vec::new(),
            },
            ElementType::Float64 => match self.results.f64s(q) {
                Ok(v) => matrix(&|i| Complex64::new(v[i], 0.0)),
                Err(_) => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// The added mass at omega = 0 or inf [D][K] for one (row, column); None when not solved.
    pub fn added_mass_limit(&self, infinite: bool, row: usize, column: usize) -> Option<f64> {
        let q = if infinite { QuantityId::AddedMassInfiniteFrequency } else { QuantityId::AddedMassZeroFrequency };
        let (d, data) = (self.dims(q)?, self.results.f32s(q).ok()?);
        (d.len() == 2 && row < d[0] && column < d[1]).then(|| f64::from(data[row * d[1] + column]))
    }

    /// Points (global) with the one-based lid or pressure surface of each: `LidPoints` and `LidIds`,
    /// or `PressureSurfacePoints` and `PressureSurfaceIds`.
    pub fn surface_points(&self, points: QuantityId, ids: QuantityId) -> Vec<(usize, V3)> {
        let (Ok(p), Ok(i)) = (self.results.f64s(points), self.results.i32s(ids)) else { return Vec::new() };
        p.as_chunks::<3>().0.iter().zip(i).map(|(p, id)| (id.max(&1).to_owned() as usize, *p)).collect()
    }

    /// The body output points (global) with their zero-based body.
    pub fn body_output_points(&self) -> Vec<(usize, V3)> {
        let (Ok(points), Ok(ids)) =
            (self.results.f64s(QuantityId::BodyPoints), self.results.i32s(QuantityId::BodyPointsIds))
        else {
            return Vec::new();
        };
        points
            .as_chunks::<3>()
            .0
            .iter()
            .zip(ids.as_chunks::<4>().0)
            .map(|(p, id)| (id[0].max(0) as usize, [p[0], p[1], p[2]]))
            .collect()
    }

    /// The panel of each body output point, when the point is the centroid of a panel of the solved
    /// bodies (the engine's points without `body_points`): the face to colour.
    pub fn body_point_panels(&self) -> Vec<Option<[V3; 4]>> {
        let (Ok(points), Ok(ids)) =
            (self.results.f64s(QuantityId::BodyPoints), self.results.i32s(QuantityId::BodyPointsIds))
        else {
            return Vec::new();
        };
        let mut by_key: std::collections::HashMap<[i32; 4], Vec<&SolvedPanel>> = std::collections::HashMap::new();
        for p in &self.panels {
            by_key.entry([p.body, p.element, p.reflection[0], p.reflection[1]]).or_default().push(p);
        }
        let dist = |a: V3, b: V3| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        points
            .as_chunks::<3>()
            .0
            .iter()
            .zip(ids.as_chunks::<4>().0)
            .map(|(p, id)| {
                let p = [p[0], p[1], p[2]];
                // a record a trim cut has several pieces: the one whose centroid the point is
                let best = by_key.get(id)?.iter().min_by(|a, b| dist(a.centroid, p).total_cmp(&dist(b.centroid, p)))?;
                let size = dist(best.quad[0], best.quad[2]).max(dist(best.quad[1], best.quad[3]));
                (dist(best.centroid, p) <= 1e-6 * size.max(1.0)).then_some(best.quad)
            })
            .collect()
    }

    /// The faces each body output point colours (global, at rest): its panel when it is a panel's
    /// centroid; in the higher order the quadrilaterals of the tessellated patches nearest to it
    /// (the points are the centres of uniform cells in the patch parameters, so each face goes to
    /// the cell it lies in); none for a point elsewhere (a specified body point), which is drawn
    /// as a square.
    pub fn body_point_faces(&self) -> PointFaces {
        self.point_faces.get_or_init(|| Arc::new(self.find_point_faces())).clone()
    }

    fn find_point_faces(&self) -> Vec<Vec<[V3; 4]>> {
        let points = self.body_output_points();
        let ids = self.results.i32s(QuantityId::BodyPointsIds).map(<[i32]>::to_vec).unwrap_or_default();
        let mut faces: Vec<Vec<[V3; 4]>> =
            self.body_point_panels().into_iter().map(|q| q.into_iter().collect()).collect();
        faces.resize(points.len(), Vec::new());
        if self.surface.is_empty() {
            return faces;
        }
        // the points of each (body, reflection) without a panel, by a grid of cells about their
        // spacing
        let (_, areas) = self.body_normals_and_areas();
        let mut groups: HashMap<(i32, [i32; 2]), Vec<usize>> = HashMap::new();
        for (k, own) in faces.iter().enumerate() {
            let Some(id) = ids.get(4 * k..4 * k + 4) else { continue };
            if own.is_empty() && areas.get(k).is_some_and(|a| *a > 0.0) && id[2] != 0 {
                groups.entry((id[0], [id[2], id[3]])).or_default().push(k);
            }
        }
        for ((body, reflection), members) in groups {
            // twice the largest cell: the cell a face lies in is among the neighbours searched
            let h = 2.0 * members.iter().map(|&k| areas[k].sqrt()).fold(0.0, f64::max).max(1e-9);
            let cell = |p: V3| p.map(|x| (x / h).floor() as i64);
            let mut grid: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
            for &k in &members {
                grid.entry(cell(points[k].1)).or_default().push(k);
            }
            let distance = |a: V3, b: V3| (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>();
            for q in self.surface.iter().filter(|q| q.body == body && q.reflection == reflection) {
                let c: V3 = std::array::from_fn(|i| q.quad.iter().map(|v| v[i]).sum::<f64>() / 4.0);
                let at = cell(c);
                let mut best: Option<(f64, usize)> = None;
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            for &k in grid.get(&[at[0] + dx, at[1] + dy, at[2] + dz]).into_iter().flatten() {
                                let d = distance(points[k].1, c);
                                if best.is_none_or(|(b, _)| d < b) {
                                    best = Some((d, k));
                                }
                            }
                        }
                    }
                }
                if let Some((_, k)) = best {
                    faces[k].push(q.quad);
                }
            }
        }
        faces
    }

    pub fn has_field_results(&self) -> bool {
        !self.field_points.is_empty()
            && [
                QuantityId::FieldPointsPressureRadiation,
                QuantityId::FieldPointsPressureIncident,
                QuantityId::FieldPointsPressureScattering,
                QuantityId::FieldPointsPressureDiffraction,
                QuantityId::TanksPressureRadiation,
            ]
            .iter()
            .any(|q| self.has(*q))
    }

    /// The free-surface amplitude |eta| at the submitted points for one frequency
    /// (`field_complex`).
    pub fn field_amplitude(
        &self,
        kind: FieldKind,
        frequency: usize,
        case: usize,
        columns: Option<&[usize]>,
    ) -> Vec<f64> {
        self.field_complex(kind, frequency, case, columns).iter().map(|v| v.norm()).collect()
    }

    /// The complex free-surface elevation eta at the submitted points for one frequency (time
    /// factor e^{+iωt}). `case` is a radiation column (Radiation) or a heading (the others).
    /// Excitation is the engine's fixed-body diffraction elevation (incident + scattering). Total
    /// adds the radiation of the solved modes (every one, or the K columns `columns`) times the
    /// complex motion of its coordinate; without motions it is the excitation. The engine computes
    /// no total itself: this sum of its quantities, by the superposition it states, is the GUI's.
    pub fn field_complex(
        &self,
        kind: FieldKind,
        frequency: usize,
        case: usize,
        columns: Option<&[usize]>,
    ) -> Vec<Complex64> {
        let points = self.field_points.len();
        let zero = Complex64::new(0.0, 0.0);
        // the unscaled elevation of one case at every point (zero where it was not solved)
        let field = |q: QuantityId, c: usize| -> Vec<Complex64> {
            self.field_values(q, frequency, c).map(|v| v.iter().map(|x| x[0]).collect()).unwrap_or_default()
        };
        let at = |f: &[Complex64], p: usize| f.get(p).copied().unwrap_or(zero);
        if kind == FieldKind::Radiation {
            let radiation = field(QuantityId::FieldPointsPressureRadiation, case);
            return (0..points).map(|p| at(&radiation, p)).collect();
        }
        let base_quantity = match kind {
            FieldKind::Incident => QuantityId::FieldPointsPressureIncident,
            FieldKind::Scattering => QuantityId::FieldPointsPressureScattering,
            _ => QuantityId::FieldPointsPressureDiffraction,
        };
        let base = field(base_quantity, case);
        // the waves' part and the motions scale with the amplitude, the radiation per unit motion not
        let (base_scale, motion_scale) = (self.wave_scale(base_quantity), self.wave_scale(QuantityId::Motion));
        let motion = (kind == FieldKind::Total)
            .then(|| {
                let d = self.dims(QuantityId::Motion)?;
                let data = self.results.complex32s(QuantityId::Motion).ok()?;
                (d.len() == 3 && frequency < d[0] && case < d[1]).then_some((d, data))
            })
            .flatten();
        let ids = self.radiation_mode_ids();
        let radiation: Vec<Vec<Complex64>> = if motion.is_some() {
            (0..ids.len()).map(|k| field(QuantityId::FieldPointsPressureRadiation, k)).collect()
        } else {
            Vec::new()
        };
        (0..points)
            .map(|p| {
                let mut eta = at(&base, p) * base_scale;
                if let Some((d, data)) = &motion {
                    for (k, id) in ids.iter().enumerate() {
                        if columns.is_some_and(|c| !c.contains(&k)) {
                            continue;
                        }
                        let Some(coordinate) = usize::try_from(*id).ok().and_then(|i| i.checked_sub(1)) else {
                            continue;
                        };
                        if coordinate >= d[2] {
                            continue;
                        }
                        let xi = widen(data[(frequency * d[1] + case) * d[2] + coordinate]) * motion_scale;
                        eta += radiation.get(k).map_or(zero, |r| at(r, p)) * xi;
                    }
                }
                eta
            })
            .collect()
    }
}

/// A tank's part of a quantity: its dimensions and values.
type TankPart = (Vec<usize>, Vec<Complex32>);

fn widen(c: Complex32) -> Complex64 {
    Complex64::new(f64::from(c.re), f64::from(c.im))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldKind {
    Radiation,
    Incident,
    Scattering,
    Excitation,
    Total,
}

/// The unit of a coefficient between a force row and a motion column, each a rotation or not (the
/// engine's coordinates: a generalized or pressure coordinate is a length, as a translation).
pub fn coefficient_unit(added_mass: bool, force_rotation: bool, motion_rotation: bool) -> &'static str {
    let (f, m) = (force_rotation, motion_rotation);
    match (added_mass, f && m, f || m) {
        (true, true, _) => "kg·m²",
        (true, false, true) => "kg·m",
        (true, false, false) => "kg",
        (false, true, _) => "N·m·s",
        (false, false, true) => "N·s",
        (false, false, false) => "N·s/m",
    }
}

pub fn force_unit(rotation: bool) -> &'static str {
    if rotation { "N·m" } else { "N" }
}

pub fn motion_unit(rotation: bool) -> &'static str {
    if rotation { "rad/m" } else { "m/m" }
}

pub fn phase_degrees(value: Complex64) -> f64 {
    value.arg().to_degrees()
}

/// "Coarse mesh", "Mesh orientation", ... for a warning code.
pub fn warning_name(d: &Diagnostic) -> String {
    use hydrobem_engine::Warning;
    match d.warning() {
        Some(Warning::PressureDriftPressureSurfaces) => "Pressure drift without pressure surfaces".into(),
        Some(Warning::CoarseMesh) => "Coarse mesh".into(),
        Some(Warning::MeshOrientation) => "Mesh orientation".into(),
        Some(Warning::MeshOpen) => "Open mesh".into(),
        Some(Warning::MeshVolume) => "Mesh volume".into(),
        Some(Warning::FieldPointNearBody) => "Field point near a body".into(),
        Some(Warning::TankZeroFrequency) => "Tank at zero frequency".into(),
        Some(Warning::DisabledObject) => "Disabled object".into(),
        Some(Warning::EnvironmentHook) => "Environment hook".into(),
        None => format!("Warning {}", d.code),
    }
}

/// "body 'Hull', panel 12, frequency 3" (one-based for people); empty when nothing is located.
pub fn location_text(l: &hydrobem_engine::Location, body_names: &[String]) -> String {
    let mut parts = Vec::new();
    if let Some(b) = l.body {
        parts.push(match body_names.get(b) {
            Some(name) => format!("body '{name}'"),
            None => format!("body {}", b + 1),
        });
    }
    if let Some(r) = l.element.filter(|r| *r > 0) {
        parts.push(format!("panel {r}"));
    }
    if let Some(i) = l.lid {
        parts.push(format!("lid {}", i + 1));
    }
    if let Some(i) = l.field_point {
        parts.push(format!("field point {}", i + 1));
    }
    if let Some(i) = l.frequency {
        parts.push(format!("frequency {}", i + 1));
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_follow_the_degrees_of_freedom() {
        assert_eq!(coefficient_unit(true, false, false), "kg");
        assert_eq!(coefficient_unit(true, true, false), "kg·m");
        assert_eq!(coefficient_unit(true, true, true), "kg·m²");
        assert_eq!(coefficient_unit(false, false, false), "N·s/m");
        assert_eq!(coefficient_unit(false, false, true), "N·s");
        assert_eq!(coefficient_unit(false, true, true), "N·m·s");
        assert_eq!(
            (force_unit(false), force_unit(true), motion_unit(false), motion_unit(true)),
            ("N", "N·m", "m/m", "rad/m")
        );
        assert!((phase_degrees(Complex64::new(0.0, 1.0)) - 90.0).abs() < 1e-12);
    }
}
