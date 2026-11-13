//! The values of items and the answers of the queries, read from the tables of the DLL.

use std::sync::OnceLock;

use crate::codes::{PanelRole, Phase, PointStatus, QualityFlags, SecondOrderStep, Stage};
use crate::error::{Error, ErrorKind, Result};
use crate::table::{Table, table};

/// An object of a model (bodies, meshes, lids, ...): valid while it is in its model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(pub(crate) u64);

impl ObjectId {
    /// The id as the C ABI writes it.
    pub fn to_bits(self) -> u64 {
        self.0
    }
}

/// The value of an item.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Real(f64),
    /// a text, a choice, a reference
    Text(String),
    Reals {
        shape: Vec<usize>,
        data: Vec<f64>,
    },
    Ints {
        shape: Vec<usize>,
        data: Vec<i64>,
    },
    Texts(Vec<String>),
}

impl Value {
    pub fn reals(shape: &[usize], data: Vec<f64>) -> Value {
        Value::Reals { shape: shape.to_vec(), data }
    }

    pub fn ints(shape: &[usize], data: Vec<i64>) -> Value {
        Value::Ints { shape: shape.to_vec(), data }
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let Value::Bool(b) = self { Some(*b) } else { None }
    }

    pub fn as_int(&self) -> Option<i64> {
        if let Value::Int(x) = self { Some(*x) } else { None }
    }

    pub fn as_real(&self) -> Option<f64> {
        if let Value::Real(x) = self { Some(*x) } else { None }
    }

    pub fn as_text(&self) -> Option<&str> {
        if let Value::Text(t) = self { Some(t) } else { None }
    }

    pub fn as_reals(&self) -> Option<&[f64]> {
        if let Value::Reals { data, .. } = self { Some(data) } else { None }
    }

    pub fn as_ints(&self) -> Option<&[i64]> {
        if let Value::Ints { data, .. } = self { Some(data) } else { None }
    }

    /// The ranges of records of a surface (`ranges` [n, 2]: one-based first and last).
    pub fn ranges(ranges: &[[i64; 2]]) -> Value {
        Value::Ints { shape: vec![ranges.len(), 2], data: ranges.as_flattened().to_vec() }
    }

    /// The rows of an [n, 2] integer value (the ranges of a surface).
    pub fn as_ranges(&self) -> Option<Vec<[i64; 2]>> {
        match self {
            Value::Ints { shape, data } if shape.len() == 2 && shape[1] == 2 => Some(data.as_chunks::<2>().0.to_vec()),
            _ => None,
        }
    }

    /// The value of a table of `hb_get` (None: no value).
    pub(crate) fn from_table(t: &Table) -> Result<Option<Value>> {
        let shape = || t.shape("value");
        Ok(Some(match t.text("kind")?.as_str() {
            "none" => return Ok(None),
            "bool" => Value::Bool(t.bool("value")?),
            "int" => Value::Int(t.i64("value")?),
            "real" => Value::Real(t.f64("value")?),
            "text" => Value::Text(t.text("value")?),
            "texts" => Value::Texts(t.texts("value")?),
            "reals" => Value::Reals { shape: shape()?, data: t.f64s("value")? },
            "ints" => Value::Ints { shape: shape()?, data: t.i64s("value")? },
            k => return Err(Error::new(ErrorKind::Internal, format!("a value of kind {k}"))),
        }))
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(x: i64) -> Value {
        Value::Int(x)
    }
}

impl From<i32> for Value {
    fn from(x: i32) -> Value {
        Value::Int(i64::from(x))
    }
}

impl From<f64> for Value {
    fn from(x: f64) -> Value {
        Value::Real(x)
    }
}

impl From<&str> for Value {
    fn from(t: &str) -> Value {
        Value::Text(t.to_string())
    }
}

impl From<String> for Value {
    fn from(t: String) -> Value {
        Value::Text(t)
    }
}

impl From<&String> for Value {
    fn from(t: &String) -> Value {
        Value::Text(t.clone())
    }
}

impl From<Vec<f64>> for Value {
    fn from(v: Vec<f64>) -> Value {
        Value::Reals { shape: vec![v.len()], data: v }
    }
}

impl<const N: usize> From<[f64; N]> for Value {
    fn from(v: [f64; N]) -> Value {
        Value::from(v.to_vec())
    }
}

impl From<Vec<String>> for Value {
    fn from(v: Vec<String>) -> Value {
        Value::Texts(v)
    }
}

impl From<&[&str]> for Value {
    fn from(v: &[&str]) -> Value {
        Value::Texts(v.iter().map(|s| s.to_string()).collect())
    }
}

/// The kind and state of an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemInfo {
    /// bool, int, real, text, choice, array, reference, origin, terms
    pub kind: String,
    pub is_set: bool,
    pub has_value: bool,
    pub is_array: bool,
    /// real, int, text
    pub dtype: String,
    pub shape: Vec<usize>,
    pub is_choice: bool,
}

/// An object's type and place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectInfo {
    pub type_name: String,
    pub name: String,
    /// the list of its parent
    pub list: String,
    /// its zero-based place in that list
    pub index: usize,
    pub path: String,
    pub parent: Option<ObjectId>,
}

/// The motion equation of the rigid bodies (`Model::motion_equation`).
#[derive(Clone, Debug, PartialEq)]
pub struct MotionEquation {
    /// d: the rows and columns of each matrix (6 per body)
    pub size: usize,
    /// [d, d] row-major
    pub inertia: Vec<f64>,
    pub stiffness: Vec<f64>,
    pub damping: Vec<f64>,
}

/// The sizes of the problem.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub bodies: usize,
    pub meshes: usize,
    pub lids: usize,
    pub ordinary_frequencies: usize,
    pub headings: usize,
    pub coordinates: usize,
    pub radiation_columns: usize,
    /// the field points in the sea
    pub field_points: usize,
    /// the field points in the tanks
    pub tank_points: usize,
    pub lid_elements: usize,
    pub pressure_elements: usize,
    pub has_zero_frequency: bool,
    pub has_infinite_frequency: bool,
    pub radiation_mode_ids: Vec<i32>,
    pub per_lid_elements: Vec<i32>,
}

/// The verdict of the field-point rules on a point: its status, the zero-based body of the status
/// (or the body of the tank of a point in a tank), the lid it is on, and of a point in a tank the
/// zero-based tank of its body (as the results number them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointClass {
    pub status: PointStatus,
    pub body: Option<usize>,
    pub lid: Option<usize>,
    pub tank: Option<usize>,
}

/// A mesh file and its options.
#[derive(Clone, Debug, Default)]
pub struct MeshFile {
    pub path: std::path::PathBuf,
    pub higher_order: bool,
    pub zero_trailer: bool,
    pub spl_path: Option<std::path::PathBuf>,
    pub discretization: Option<Vec<[i32; 6]>>,
    pub panel_size: Option<f64>,
    pub spline_order: Option<i32>,
    pub outer_quadrature: Option<i32>,
    pub inner_quadrature: Option<i32>,
}

/// A named analytic shape.
#[derive(Clone, Debug, Default)]
pub struct AnalyticShape {
    pub shape: String,
    pub dimensions: Vec<(String, f64)>,
    pub discs: Vec<[f64; 3]>,
    pub parts: Vec<String>,
    pub mapping: String,
    pub discretization: Vec<[i32; 6]>,
    pub reference_length: f64,
    pub symmetry: String,
    pub label: String,
}

/// Points and unit normals of a patch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatchPoints {
    pub points: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
}

/// The seven keys of an element as the engine solves it: within one query they identify one row
/// of the panels, and every quad of the tessellation of the same role and reflections carries
/// those of its row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PanelKey {
    pub body: i32,
    pub role: Option<PanelRole>,
    pub tank: i32,
    pub owner: i32,
    pub element: i32,
    pub part: i32,
    pub reflection: [i32; 2],
}

fn roles(t: &Table) -> Result<Vec<Option<PanelRole>>> {
    t.i32s("role")?
        .into_iter()
        .map(|r| match r {
            -1 => Ok(None),
            r => PanelRole::from_value(r).map(Some).ok_or_else(|| Error::new(ErrorKind::Internal, format!("role {r}"))),
        })
        .collect()
}

/// The elements as the engine uses them, with their seven keys.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Panels {
    /// m, global
    pub vertices: Vec<[[f64; 3]; 4]>,
    /// 3 or 4
    pub distinct_vertices: Vec<i32>,
    /// zero-based among the enabled bodies; -1 an element of no body
    pub body: Vec<i32>,
    /// None for an element of a mesh alone
    pub role: Vec<Option<PanelRole>>,
    /// the zero-based tank of its body the element bounds or lies in; -1 none
    pub tank: Vec<i32>,
    /// the zero-based place of its input object in its list; -1 none
    pub owner: Vec<i32>,
    /// one-based in the numbering of the mesh its input names (generated surfaces from 1)
    pub element: Vec<i32>,
    /// the zero-based piece of an element cut by a trim
    pub part: Vec<i32>,
    /// (sx, sy), each ±1: the copy about the mesh's symmetry planes
    pub reflection: Vec<[i32; 2]>,
    pub centroids: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
    pub areas: Vec<f64>,
}

impl Panels {
    pub fn len(&self) -> usize {
        self.areas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }

    /// The seven keys of row `k`.
    pub fn key(&self, k: usize) -> PanelKey {
        PanelKey {
            body: self.body[k],
            role: self.role[k],
            tank: self.tank[k],
            owner: self.owner[k],
            element: self.element[k],
            part: self.part[k],
            reflection: self.reflection[k],
        }
    }

    pub(crate) fn from_table(t: &Table) -> Result<Panels> {
        Ok(Panels {
            vertices: t.quads("vertices")?,
            distinct_vertices: t.i32s("distinct_vertices")?,
            body: t.i32s("body")?,
            role: roles(t)?,
            tank: t.i32s("tank")?,
            owner: t.i32s("owner")?,
            element: t.i32s("element")?,
            part: t.i32s("part")?,
            reflection: t.int_rows::<2>("reflection")?,
            centroids: t.rows::<3>("centroids")?,
            normals: t.rows::<3>("normals")?,
            areas: t.f64s("areas")?,
        })
    }
}

/// The stored elements of a mesh, mesh coordinates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshElements {
    /// one-based
    pub element: Vec<i32>,
    pub vertices: Vec<[[f64; 3]; 4]>,
    pub distinct_vertices: Vec<i32>,
    pub centroids: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
    pub areas: Vec<f64>,
}

impl MeshElements {
    pub fn len(&self) -> usize {
        self.areas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }

    pub(crate) fn from_table(t: &Table) -> Result<MeshElements> {
        Ok(MeshElements {
            element: t.i32s("element")?,
            vertices: t.quads("vertices")?,
            distinct_vertices: t.i32s("distinct_vertices")?,
            centroids: t.rows::<3>("centroids")?,
            normals: t.rows::<3>("normals")?,
            areas: t.f64s("areas")?,
        })
    }
}

/// Quadrilaterals for drawing: a low-order panel one, a patch divisions_u x divisions_v on its
/// exact surface; unit normals into the fluid at the corners; each with the keys of its panels row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tessellation {
    pub vertices: Vec<[[f64; 3]; 4]>,
    pub normals: Vec<[[f64; 3]; 4]>,
    /// zero-based among the enabled bodies; -1 an element of no body
    pub body: Vec<i32>,
    /// None for an element of a mesh alone
    pub role: Vec<Option<PanelRole>>,
    /// the zero-based tank of its body the element bounds or lies in; -1 none
    pub tank: Vec<i32>,
    /// the zero-based place of its input object in its list; -1 none
    pub owner: Vec<i32>,
    /// one-based in the numbering of the mesh its input names (generated surfaces from 1)
    pub element: Vec<i32>,
    /// the zero-based piece of an element cut by a trim
    pub part: Vec<i32>,
    /// (sx, sy), each ±1: the copy about the mesh's symmetry planes
    pub reflection: Vec<[i32; 2]>,
}

impl Tessellation {
    pub fn len(&self) -> usize {
        self.body.len()
    }

    pub fn is_empty(&self) -> bool {
        self.body.is_empty()
    }

    /// The seven keys of row `k`.
    pub fn key(&self, k: usize) -> PanelKey {
        PanelKey {
            body: self.body[k],
            role: self.role[k],
            tank: self.tank[k],
            owner: self.owner[k],
            element: self.element[k],
            part: self.part[k],
            reflection: self.reflection[k],
        }
    }

    pub(crate) fn from_table(t: &Table) -> Result<Tessellation> {
        Ok(Tessellation {
            vertices: t.quads("vertices")?,
            normals: t.quads("normals")?,
            body: t.i32s("body")?,
            role: roles(t)?,
            tank: t.i32s("tank")?,
            owner: t.i32s("owner")?,
            element: t.i32s("element")?,
            part: t.i32s("part")?,
            reflection: t.int_rows::<2>("reflection")?,
        })
    }
}

/// The geometry of a mesh alone, in its own coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeshGeometry {
    pub volumes: [f64; 3],
    pub volume: f64,
    pub center_of_volume: [f64; 3],
    pub area: f64,
    pub bounds: [[f64; 3]; 2],
    pub waterplane_area: f64,
    pub waterplane_centroid: [f64; 2],
    pub waterplane_moments: [f64; 3],
}

fn array<const N: usize>(t: &Table, name: &str) -> Result<[f64; N]> {
    t.f64s(name)?
        .try_into()
        .map_err(|v: Vec<f64>| Error::new(ErrorKind::Internal, format!("{name}: {} values, not {N}", v.len())))
}

impl MeshGeometry {
    pub(crate) fn from_table(t: &Table) -> Result<MeshGeometry> {
        let b = array::<6>(t, "bounds")?;
        Ok(MeshGeometry {
            volumes: array(t, "volumes")?,
            volume: t.f64("volume")?,
            center_of_volume: array(t, "center_of_volume")?,
            area: t.f64("area")?,
            bounds: [[b[0], b[1], b[2]], [b[3], b[4], b[5]]],
            waterplane_area: t.f64("waterplane_area")?,
            waterplane_centroid: array(t, "waterplane_centroid")?,
            waterplane_moments: array(t, "waterplane_moments")?,
        })
    }
}

/// The quality of meshes: per element, then per body.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshQuality {
    pub body: Vec<i32>,
    pub element: Vec<i32>,
    pub part: Vec<i32>,
    pub size: Vec<f64>,
    pub aspect: Vec<f64>,
    pub per_wavelength: Vec<f64>,
    pub flags: Vec<QualityFlags>,
    pub volumes: Vec<[f64; 3]>,
    pub min_per_wavelength: Vec<f64>,
    pub edges: Vec<[i32; 4]>,
}

impl MeshQuality {
    pub(crate) fn from_table(t: &Table) -> Result<MeshQuality> {
        Ok(MeshQuality {
            body: t.i32s("body")?,
            element: t.i32s("element")?,
            part: t.i32s("part")?,
            size: t.f64s("size")?,
            aspect: t.f64s("aspect")?,
            per_wavelength: t.f64s("per_wavelength")?,
            flags: t.i32s("flags")?.into_iter().map(|f| QualityFlags(f as u32)).collect(),
            volumes: t.rows::<3>("volumes")?,
            min_per_wavelength: t.f64s("min_per_wavelength")?,
            edges: t.int_rows::<4>("edges")?,
        })
    }
}

/// The hydrostatics of a body after its trim and placement; the centres and the waterplane
/// centroid in body coordinates, the stiffness about the rotation centre R.
#[derive(Clone, Copy, Debug)]
pub struct Hydrostatics {
    /// the three estimates of the divergence theorem
    pub volumes: [f64; 3],
    /// their mean
    pub volume: f64,
    pub cb: [f64; 3],
    pub wp_area: f64,
    pub wp_centroid: [f64; 2],
    pub c_rho_g: [f64; 36],
    /// a centre of gravity and a mass: the weight terms are in c
    pub has_weight: bool,
    /// NaN without a mass
    pub mass: f64,
    /// NaN without a centre of gravity
    pub cg: [f64; 3],
    pub c: [f64; 36],
    pub displaced_mass: f64,
    pub metacentric_height: [f64; 2],
}

impl Hydrostatics {
    pub(crate) fn from_table(t: &Table) -> Result<Hydrostatics> {
        Ok(Hydrostatics {
            volumes: array(t, "volumes")?,
            volume: t.f64("volume")?,
            cb: array(t, "center_of_buoyancy")?,
            wp_area: t.f64("waterplane_area")?,
            wp_centroid: array(t, "waterplane_centroid")?,
            c_rho_g: array(t, "stiffness_rho_g")?,
            has_weight: t.bool("has_weight")?,
            mass: t.f64("mass")?,
            cg: array(t, "center_of_gravity")?,
            c: array(t, "stiffness")?,
            displaced_mass: t.f64("displaced_mass")?,
            metacentric_height: array(t, "metacentric_height")?,
        })
    }
}

/// The memory of a solve in bytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryEstimate {
    /// shared by all frequencies: the influence data and the geometry
    pub statics: i64,
    /// one frequency thread: its matrices, LU and potentials (resident)
    pub per_frequency: i64,
    /// the outputs of all frequencies and the results built from them
    pub outputs: i64,
    /// resident: statics + frequency threads x per_frequency + outputs
    pub peak: i64,
    /// the commit charge
    pub commit: i64,
}

/// Where a progress report of a solve is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressAt {
    Statics,
    Limit,
    Frequency(usize),
    /// an item of a second-order step (`item` None: the work of the step all workers share)
    SecondOrder {
        step: SecondOrderStep,
        item: Option<usize>,
    },
    /// the solve as a whole: a phase, an idle worker
    Solve,
}

/// A progress report of a solve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    pub stage: Stage,
    pub at: ProgressAt,
    pub stage_done: usize,
    pub stage_total: usize,
    pub frequencies_done: usize,
    pub frequencies_total: usize,
    pub seconds: f64,
    /// the frequency worker that reports (0 to frequency_threads - 1); None for the work all
    /// threads share
    pub worker: Option<usize>,
    pub phase: Phase,
}

/// The state of a frequency (or a limit) in a solve.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RunState {
    #[default]
    Waiting,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl RunState {
    fn from_name(name: &str) -> RunState {
        match name {
            "running" => RunState::Running,
            "done" => RunState::Done,
            "failed" => RunState::Failed,
            "cancelled" => RunState::Cancelled,
            _ => RunState::Waiting,
        }
    }
}

/// One frequency (or limit, or item of a second-order step) of a solve: its state, stage and the
/// share of its work done (0 to 1, the stages weighted by their cost; never decreasing, 1 once
/// done), and the frequency worker that runs or ran it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrequencyProgress {
    pub state: RunState,
    pub stage: Option<Stage>,
    pub fraction: f64,
    /// None while it waits, and for an item all the workers share
    pub worker: Option<usize>,
}

/// The state of a frequency worker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkerState {
    /// on an item, or on the work all threads share
    Running,
    /// no item left for it in the phase while others still run, and while the results are written
    #[default]
    Idle,
    /// the solve has ended
    Done,
}

/// What a frequency worker is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerTask {
    /// a limit, by its index in `SolveProgress::limits`
    Limit(usize),
    /// an ordinary frequency, by its index in the results
    Frequency(usize),
    /// an item of a second-order step (None: the work of the step all workers share)
    SecondOrder { step: SecondOrderStep, item: Option<usize> },
}

/// A frequency worker of a solve at one instant (its number is its index in
/// `SolveProgress::workers`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorkerProgress {
    pub state: WorkerState,
    /// None without one (also the statics, which all threads share)
    pub task: Option<WorkerTask>,
    pub stage: Option<Stage>,
    /// of its item (0 without one)
    pub fraction: f64,
}

/// A step of the second order of a solve: its state, its share done (never decreasing, 1 once
/// done) and its items.
#[derive(Clone, Debug, PartialEq)]
pub struct SecondOrderProgress {
    pub step: SecondOrderStep,
    pub state: RunState,
    pub fraction: f64,
    pub items: Vec<FrequencyProgress>,
}

/// A limit of a solve (ω = 0 or ω = ∞) and its progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimitProgress {
    /// the limit ω = ∞, else ω = 0
    pub infinite: bool,
    pub progress: FrequencyProgress,
}

/// The progress of the running solve of a model, or of its last one, at an instant.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SolveProgress {
    pub running: bool,
    /// since the solve began, until it ended
    pub seconds: f64,
    /// the frequency-independent influence data done, 0 to 1
    pub preparation: f64,
    /// the frequencies and limits done and in all
    pub done: usize,
    pub total: usize,
    /// in the order they are solved
    pub limits: Vec<LimitProgress>,
    /// the ordinary frequencies in the order of the results
    pub frequencies: Vec<FrequencyProgress>,
    /// None before the first solve
    pub phase: Option<Phase>,
    /// the frequency workers (frequency_threads of them)
    pub workers: Vec<WorkerProgress>,
    /// the steps of the second order in the order they began
    pub second_order: Vec<SecondOrderProgress>,
}

impl SolveProgress {
    pub(crate) fn from_table(t: &Table) -> Result<SolveProgress> {
        let index = |i: i64| usize::try_from(i).ok();
        let rows = |state: &str, stage: &str, fraction: &str, worker: &str| -> Result<Vec<FrequencyProgress>> {
            let (s, g, f, w) = (t.texts(state)?, t.texts(stage)?, t.f64s(fraction)?, t.i64s(worker)?);
            Ok((0..s.len().min(g.len()).min(f.len()).min(w.len()))
                .map(|k| FrequencyProgress {
                    state: RunState::from_name(&s[k]),
                    stage: Stage::from_name(&g[k]),
                    fraction: f[k],
                    worker: index(w[k]),
                })
                .collect())
        };
        let limits = t
            .texts("limit")?
            .iter()
            .zip(rows("limit_state", "limit_stage", "limit_fraction", "limit_worker")?)
            .map(|(l, progress)| LimitProgress { infinite: l == "infinite", progress })
            .collect();
        let (ws, wt, wi, wg, wf) = (
            t.texts("worker_state")?,
            t.texts("worker_task")?,
            t.i64s("worker_index")?,
            t.texts("worker_stage")?,
            t.f64s("worker_fraction")?,
        );
        let workers = (0..ws.len().min(wt.len()).min(wi.len()).min(wg.len()).min(wf.len()))
            .map(|k| WorkerProgress {
                state: match ws[k].as_str() {
                    "running" => WorkerState::Running,
                    "done" => WorkerState::Done,
                    _ => WorkerState::Idle,
                },
                task: match (wt[k].as_str(), index(wi[k])) {
                    ("limit", Some(i)) => Some(WorkerTask::Limit(i)),
                    ("frequency", Some(i)) => Some(WorkerTask::Frequency(i)),
                    (name, item) => SecondOrderStep::from_name(name).map(|step| WorkerTask::SecondOrder { step, item }),
                },
                stage: Stage::from_name(&wg[k]),
                fraction: wf[k],
            })
            .collect();
        let mut items = rows("item_state", "item_stage", "item_fraction", "item_worker")?.into_iter();
        let (sn, ss, sf, si) =
            (t.texts("step")?, t.texts("step_state")?, t.f64s("step_fraction")?, t.i64s("step_items")?);
        let mut second_order = Vec::new();
        for k in 0..sn.len().min(ss.len()).min(sf.len()).min(si.len()) {
            let n = index(si[k]).unwrap_or(0);
            let step_items = items.by_ref().take(n).collect();
            if let Some(step) = SecondOrderStep::from_name(&sn[k]) {
                second_order.push(SecondOrderProgress {
                    step,
                    state: RunState::from_name(&ss[k]),
                    fraction: sf[k],
                    items: step_items,
                });
            }
        }
        Ok(SolveProgress {
            running: t.bool("running")?,
            seconds: t.f64("seconds")?,
            preparation: t.f64("preparation")?,
            done: t.count("done")?,
            total: t.count("total")?,
            limits,
            frequencies: rows("state", "stage", "fraction", "worker")?,
            phase: t.texts("phase")?.first().and_then(|p| Phase::from_name(p)),
            workers,
            second_order,
        })
    }
}

/// The build of the engine.
#[derive(Clone, Debug, Default)]
pub struct BuildInfo {
    pub version: String,
    pub commit: String,
    pub compiler: String,
    pub linear_algebra: String,
    pub build_type: String,
    /// the version of the engine's Rust API, also in the meta data of a results file
    pub api_version: i32,
    pub avx2: bool,
}

/// The build of the engine of the DLL (empty when it is not loaded).
pub fn build_info() -> &'static BuildInfo {
    static INFO: OnceLock<BuildInfo> = OnceLock::new();
    INFO.get_or_init(|| {
        let read = || -> Result<BuildInfo> {
            // SAFETY: a writable pointer
            #[allow(unsafe_code)]
            let t = table(|a, out| unsafe { (a.hb_build_info)(out) })?;
            Ok(BuildInfo {
                version: t.text("version")?,
                commit: t.text("commit")?,
                compiler: t.text("compiler")?,
                linear_algebra: t.text("linear_algebra")?,
                build_type: t.text("build_type")?,
                api_version: t.i32("api_version")?,
                avx2: t.bool("avx2")?,
            })
        };
        read().unwrap_or_default()
    })
}

/// An item of an object type of the schema.
#[derive(Clone, Debug)]
pub struct ItemDef {
    pub name: &'static str,
    pub kind: &'static str,
    pub unit: &'static str,
    pub description: &'static str,
}

/// A list of an object type of the schema.
#[derive(Clone, Debug)]
pub struct ListDef {
    pub name: &'static str,
    /// the object types it holds
    pub types: Vec<&'static str>,
}

/// An object type of the schema: its items and lists.
#[derive(Clone, Debug)]
pub struct TypeDef {
    pub name: &'static str,
    pub items: Vec<ItemDef>,
    pub lists: Vec<ListDef>,
}

/// The object types of the model (empty when the DLL is not loaded).
pub fn schema() -> &'static [TypeDef] {
    static SCHEMA: OnceLock<Vec<TypeDef>> = OnceLock::new();
    SCHEMA.get_or_init(|| read_schema().unwrap_or_default())
}

fn read_schema() -> Result<Vec<TypeDef>> {
    // texts kept for the life of the program, as the engine's own schema
    let keep = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    #[allow(unsafe_code)]
    // SAFETY: writable pointers
    let (items, lists) =
        (table(|a, out| unsafe { (a.hb_schema_items)(out) })?, table(|a, out| unsafe { (a.hb_schema_lists)(out) })?);
    let mut types: Vec<TypeDef> = Vec::new();
    let of = |types: &mut Vec<TypeDef>, name: String| -> usize {
        if let Some(k) = types.iter().position(|t| t.name == name) {
            return k;
        }
        types.push(TypeDef { name: keep(name), items: Vec::new(), lists: Vec::new() });
        types.len() - 1
    };
    let (ty, item, kind) = (items.texts("type")?, items.texts("item")?, items.texts("kind")?);
    let (unit, description) = (items.texts("unit")?, items.texts("description")?);
    for k in 0..ty.len() {
        let t = of(&mut types, ty[k].clone());
        types[t].items.push(ItemDef {
            name: keep(item[k].clone()),
            kind: keep(kind[k].clone()),
            unit: keep(unit[k].clone()),
            description: keep(description[k].clone()),
        });
    }
    let (ty, list, held) = (lists.texts("type")?, lists.texts("list")?, lists.texts("types")?);
    for k in 0..ty.len() {
        let t = of(&mut types, ty[k].clone());
        let held = held[k].split_whitespace().map(|s| keep(s.to_string())).collect();
        types[t].lists.push(ListDef { name: keep(list[k].clone()), types: held });
    }
    Ok(types)
}
