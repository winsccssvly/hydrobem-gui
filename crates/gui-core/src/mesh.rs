//! A mesh as the GUI holds it: the stored records of a mesh object of the engine, read from a GDF
//! or geometry (`.hbg`) file by the engine's reader. A low-order mesh (kind `surface`) is its panels; a higher-order
//! (patch) mesh keeps the engine's object as read and is drawn by the engine's tessellation.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use hydrobem_engine::{MeshFile, Model, ObjectId, Value, schema};

use crate::project::V3;

/// The symmetry of a mesh by its planes, as the engine names it: Yz the plane x = 0, Xz the plane y = 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Symmetry {
    #[default]
    None,
    Yz,
    Xz,
    XzAndYz,
}

impl Symmetry {
    pub fn engine_name(self) -> &'static str {
        match self {
            Symmetry::None => "none",
            Symmetry::Yz => "yz",
            Symmetry::Xz => "xz",
            Symmetry::XzAndYz => "xz_and_yz",
        }
    }

    pub fn from_engine(name: &str) -> Option<Symmetry> {
        [Symmetry::None, Symmetry::Yz, Symmetry::Xz, Symmetry::XzAndYz].into_iter().find(|s| s.engine_name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            Symmetry::None => "None",
            Symmetry::Yz => "YZ (the plane x = 0)",
            Symmetry::Xz => "XZ (the plane y = 0)",
            Symmetry::XzAndYz => "XZ and YZ",
        }
    }

    fn x(self) -> bool {
        matches!(self, Symmetry::Yz | Symmetry::XzAndYz)
    }

    fn y(self) -> bool {
        matches!(self, Symmetry::Xz | Symmetry::XzAndYz)
    }
}

#[derive(Clone, Debug)]
pub struct Mesh {
    /// Unique per mesh read or made; two users of one mesh share it.
    pub id: u64,
    /// The file it was read from (absolute), empty for a mesh without one.
    pub source_file: String,
    pub label: String,
    pub reference_length: f64,
    pub file_gravity: Option<f64>,
    pub symmetry: Symmetry,
    /// The stored records, four vertices each (a triangle repeats one); of a patch mesh the
    /// corners of each patch.
    pub panels: Vec<[V3; 4]>,
    /// A higher-order mesh: its engine object and its tessellation for display.
    pub patch: Option<PatchMesh>,
    /// An analytic mesh made in the GUI: its shape and dimensions (editable).
    pub analytic: Option<crate::analytic::AnalyticSpec>,
    /// A patch mesh read from a geometry file with cad2bem's report beside it: per patch its CAD
    /// face and checks. Saved with the project (the GUI state).
    pub report: Option<Arc<crate::patch_report::PatchReport>>,
    /// Where the discretization rows of a patch mesh came from (shown in its Details).
    pub rows_from: RowsFrom,
}

/// Where the discretization rows of a patch mesh came from: the potential is solved with the rows
/// whatever their origin; this says why they are what they are.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum RowsFrom {
    /// Not recorded (a panel mesh, or a mesh made in code).
    #[default]
    Unknown,
    /// The SPL file beside the GDF file.
    Spl,
    /// `DEFAULT_DISCRETIZATION`: a file without a discretization of its own.
    Default,
    /// The geometry file's own (an .hbg with a discretization or a panel size).
    File,
    /// cad2bem's recommendation (its report beside the geometry file), by patch name.
    Cad2bem,
    /// The analytic shape's, as made in the GUI.
    Analytic,
    /// The engine's rule from this panel size (m), chosen at import or applied in this session.
    PanelSize(f64),
    /// These spans per patch, chosen when the GDF file was imported.
    Spans(i32, i32),
    /// A row edited in the Discretization table.
    Edited,
    /// As saved with the project (its origin is not saved).
    Project,
}

impl RowsFrom {
    pub fn label(&self) -> String {
        match self {
            RowsFrom::Unknown => "as read".into(),
            RowsFrom::Spl => "the SPL file beside the GDF file".into(),
            RowsFrom::Default => {
                "the default: 4 x 4 spans per patch, order 3, Gauss 3 and 4 (the file has none)".into()
            }
            RowsFrom::File => "the geometry file's own".into(),
            RowsFrom::Cad2bem => "cad2bem's recommendation".into(),
            RowsFrom::Analytic => "the analytic shape's".into(),
            RowsFrom::PanelSize(s) => format!("a panel size of {} m", crate::format::format_number(*s)),
            RowsFrom::Spans(nu, nv) => format!("{nu} x {nv} spans per patch, chosen at import"),
            RowsFrom::Edited => "edited in the table".into(),
            RowsFrom::Project => "as saved with the project".into(),
        }
    }

    /// The panel size applied in this session, if that is where the rows came from.
    pub fn panel_size(&self) -> Option<f64> {
        match self {
            RowsFrom::PanelSize(s) => Some(*s),
            _ => None,
        }
    }
}

/// A higher-order (patch) mesh: the engine's mesh object as read (kind `flat`, `spline` or
/// `analytic`), written back unchanged, and the quadrilaterals that draw it.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchMesh {
    pub kind: String,
    pub object: ObjectData,
    /// Per stored patch (zero-based) its display quadrilaterals, mesh coordinates.
    pub tessellation: Vec<(usize, [V3; 4])>,
    /// The surface's unit normals at the corners of each quadrilateral (smooth shading).
    pub normals: Vec<[V3; 4]>,
    /// Which sides (corner k to k + 1) of each quadrilateral lie on its patch's boundary.
    pub edges: Vec<[bool; 4]>,
}

/// A quadrilateral of a curved surface: its corners, the surface normals there and which of its
/// sides are patch boundaries (drawn as lines; the others are not).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmoothQuad {
    pub corners: [V3; 4],
    pub normals: [V3; 4],
    pub edges: [bool; 4],
}

/// The unit normal of a quadrilateral by its diagonals (+z when it has no area).
pub fn quad_normal(q: &[V3; 4]) -> V3 {
    let d1 = [q[2][0] - q[0][0], q[2][1] - q[0][1], q[2][2] - q[0][2]];
    let d2 = [q[3][0] - q[1][0], q[3][1] - q[1][1], q[3][2] - q[1][2]];
    let n = [d1[1] * d2[2] - d1[2] * d2[1], d1[2] * d2[0] - d1[0] * d2[2], d1[0] * d2[1] - d1[1] * d2[0]];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 { n.map(|v| v / len) } else { [0.0, 0.0, 1.0] }
}

/// The display subdivision per side of a patch: finer for few patches, about 20 000 quadrilaterals
/// in all at most, 4 at least.
pub fn tessellation_steps(patches: usize) -> i32 {
    let n = (20_000.0 / patches.max(1) as f64).sqrt().floor() as i32;
    n.clamp(4, 24)
}

/// For quadrilaterals grouped by patch (`group`), the sides no other quadrilateral of the same
/// group shares: the patch boundaries.
pub fn boundary_edges<G: std::hash::Hash + Eq + Copy>(quads: &[[V3; 4]], group: &[G]) -> Vec<[bool; 4]> {
    use std::collections::HashMap;
    let scale = quads.iter().flatten().flatten().fold(1.0f64, |m, v| m.max(v.abs()));
    let key = |p: V3| p.map(|x| (x / scale * 1e7).round() as i64);
    let side = |q: &[V3; 4], k: usize| {
        let (a, b) = (key(q[k]), key(q[(k + 1) % 4]));
        if a <= b { (a, b) } else { (b, a) }
    };
    // per group and side (its two corners, rounded), how many quadrilaterals have it
    type Side = ([i64; 3], [i64; 3]);
    let mut count: HashMap<(G, Side), u32> = HashMap::new();
    for (q, g) in quads.iter().zip(group) {
        for k in 0..4 {
            let s = side(q, k);
            // a collapsed side (a triangle's repeated corner) is no edge
            if s.0 != s.1 {
                *count.entry((*g, s)).or_default() += 1;
            }
        }
    }
    quads
        .iter()
        .zip(group)
        .map(|(q, g)| {
            std::array::from_fn(|k| {
                let s = side(q, k);
                s.0 != s.1 && count.get(&(*g, s)) == Some(&1)
            })
        })
        .collect()
}

/// The set items and the children of an engine object, to copy it into another model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectData {
    /// Its object type (`Mesh`, `SplinePatch`, ...).
    pub type_name: String,
    pub items: Vec<(String, Value)>,
    /// (list, name, object)
    pub children: Vec<(String, String, ObjectData)>,
}

impl ObjectData {
    /// The object `id` of `m`: every item that is set, every child.
    pub fn read(m: &Model, id: ObjectId) -> Result<ObjectData, String> {
        let e = |e: hydrobem_engine::Error| e.message().to_string();
        let info = m.object_info(id).map_err(e)?;
        let def = schema().iter().find(|t| t.name == info.type_name).ok_or("unknown object type")?;
        let mut out = ObjectData { type_name: info.type_name.to_string(), ..ObjectData::default() };
        for item in &def.items {
            // an item of another mesh kind is not there
            if item.name == "name" || !m.item_info(id, item.name).is_ok_and(|i| i.is_set) {
                continue;
            }
            if let Some(v) = m.get(id, item.name).map_err(e)? {
                out.items.push((item.name.to_string(), v));
            }
        }
        for list in &def.lists {
            for child in m.children(id, list.name).map_err(e)? {
                let name = m.object_info(child).map_err(e)?.name;
                out.children.push((list.name.to_string(), name, ObjectData::read(m, child)?));
            }
        }
        Ok(out)
    }

    /// Writes the items and children into the object `id` of `m`.
    pub fn write(&self, m: &mut Model, id: ObjectId) -> hydrobem_engine::Result<()> {
        for (item, value) in &self.items {
            m.set(id, item, value.clone())?;
        }
        for (list, name, data) in &self.children {
            let child = m.add(id, list, Some(data.type_name.as_str()), name, None)?;
            data.write(m, child)?;
        }
        Ok(())
    }

    pub fn get(&self, item: &str) -> Option<&Value> {
        self.items.iter().find(|(n, _)| n == item).map(|(_, v)| v)
    }

    pub fn set(&mut self, item: &str, value: Value) {
        match self.items.iter_mut().find(|(n, _)| n == item) {
            Some(slot) => slot.1 = value,
            None => self.items.push((item.to_string(), value)),
        }
    }
}

/// Whether a GDF file is of the higher-order format: its fourth line holds the patch count and
/// the geometry code (two integers), where a low-order file holds the panel count.
pub fn is_higher_order_gdf(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else { return false };
    let Some(line) = text.lines().nth(3) else { return false };
    let numbers: Vec<&str> = line.split_whitespace().take(2).collect();
    numbers.len() == 2 && numbers.iter().all(|t| t.parse::<i64>().is_ok())
}

/// The extensions of the mesh files the GUI reads: WAMIT GDF and the engine's geometry file.
pub const MESH_EXTENSIONS: [&str; 2] = ["gdf", "hbg"];

/// Whether a path is a geometry file (`.hbg`).
pub fn is_hbg(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("hbg"))
}

/// The discretization of every patch of a higher-order GDF file that has no SPL file, as asked
/// when it is imported (a GDF holds only the geometry; WAMIT takes these from its cfg file:
/// PANEL_SIZE, KSPLIN, IQUADO, IQUADI).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GdfDiscretization {
    /// By a panel size (the engine's rule, as WAMIT's PANEL_SIZE), else by `nu` x `nv` spans.
    pub by_panel_size: bool,
    pub panel_size: f64,
    pub nu: i32,
    pub nv: i32,
    /// B-spline order (ku = kv; 3 quadratic, WAMIT's KSPLIN).
    pub order: i32,
    /// Gauss orders of the outer (test) and inner (source) integrals (WAMIT's IQUADO, IQUADI).
    pub outer: i32,
    pub inner: i32,
}

impl Default for GdfDiscretization {
    fn default() -> Self {
        GdfDiscretization { by_panel_size: true, panel_size: 1.0, nu: 4, nv: 4, order: 3, outer: 3, inner: 4 }
    }
}

/// Whether importing `path` asks for a discretization: a higher-order GDF file without an SPL
/// file of the same name beside it.
pub fn needs_discretization(path: &Path) -> bool {
    is_higher_order_gdf(path) && !["spl", "SPL"].iter().any(|e| path.with_extension(e).is_file())
}

/// The discretization of a patch mesh read without an SPL file: per patch 4 x 4 spans, B-splines
/// of order 3 (quadratic, WAMIT's KSPLIN = 3), Gauss orders 3 (outer) and 4 (inner); editable
/// afterwards.
pub const DEFAULT_DISCRETIZATION: [i32; 6] = [4, 4, 3, 3, 3, 4];

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

impl Mesh {
    /// Reads a GDF or geometry (`.hbg`) file with the engine's reader; a GDF file ending with a
    /// record of zeros is accepted. The error is the engine's message (file and line).
    pub fn load(path: &Path) -> Result<Arc<Mesh>, String> {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        if is_hbg(&path) {
            return Mesh::load_hbg(&path);
        }
        if is_higher_order_gdf(&path) {
            return Mesh::load_patches(&path);
        }
        let read = |zero_trailer: bool| -> hydrobem_engine::Result<Mesh> {
            let mut m = Model::new();
            let id = m.load_mesh("mesh", &MeshFile { path: path.clone(), zero_trailer, ..MeshFile::default() })?;
            Mesh::from_object(&m, id).map_err(|e| hydrobem_engine::Error::new(hydrobem_engine::ErrorKind::Value, e))
        };
        match read(false) {
            Ok(mesh) => Ok(Arc::new(mesh)),
            Err(first) => read(true).map(Arc::new).map_err(|_| first.message().to_string()),
        }
    }

    /// Reads a higher-order GDF file: with the SPL file of the same name beside it (its omitted
    /// orders as the engine's defaults), else `DEFAULT_DISCRETIZATION`.
    pub fn load_patches(path: &Path) -> Result<Arc<Mesh>, String> {
        let spl = ["spl", "SPL"].iter().map(|e| path.with_extension(e)).find(|p| p.is_file());
        let mut attempts = Vec::new();
        let base = MeshFile { path: path.to_path_buf(), higher_order: true, ..MeshFile::default() };
        match &spl {
            Some(spl) => {
                let with = MeshFile { spl_path: Some(spl.clone()), ..base.clone() };
                attempts.push(with.clone());
                // an SPL file of spans only takes the orders as global values
                attempts.push(MeshFile {
                    spline_order: Some(3),
                    outer_quadrature: Some(3),
                    inner_quadrature: Some(4),
                    ..with
                });
            }
            None => attempts.push(MeshFile { discretization: Some(vec![DEFAULT_DISCRETIZATION]), ..base }),
        }
        let mut last_error = None;
        for file in attempts {
            for zero_trailer in [false, true] {
                let mut m = Model::new();
                let read = m
                    .load_mesh("mesh", &MeshFile { zero_trailer, ..file.clone() })
                    .map_err(|e| e.message().to_string())
                    .and_then(|id| Mesh::from_object(&m, id));
                match read {
                    Ok(mut mesh) => {
                        mesh.rows_from = if file.spl_path.is_some() { RowsFrom::Spl } else { RowsFrom::Default };
                        return Ok(Arc::new(mesh));
                    }
                    Err(e) => {
                        last_error = Some(e);
                    }
                }
            }
        }
        Err(last_error.unwrap_or_default())
    }

    /// Reads a higher-order GDF file with the discretization chosen for it (`needs_discretization`).
    pub fn load_gdf_with(path: &Path, choice: &GdfDiscretization) -> Result<Arc<Mesh>, String> {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let mut mesh = Mesh::read_patch_gdf(&path, choice)?;
        mesh.rows_from = if choice.by_panel_size {
            RowsFrom::PanelSize(choice.panel_size)
        } else {
            RowsFrom::Spans(choice.nu, choice.nv)
        };
        Ok(Arc::new(mesh))
    }

    /// The engine's reader of a higher-order GDF file with `choice` (a GDF file ending with a
    /// record of zeros is accepted).
    fn read_patch_gdf(path: &Path, choice: &GdfDiscretization) -> Result<Mesh, String> {
        if choice.by_panel_size && !(choice.panel_size > 0.0 && choice.panel_size.is_finite()) {
            return Err("the panel size is a positive length".into());
        }
        let base = MeshFile {
            path: path.to_path_buf(),
            higher_order: true,
            panel_size: choice.by_panel_size.then_some(choice.panel_size),
            discretization: (!choice.by_panel_size)
                .then(|| vec![[choice.nu, choice.nv, choice.order, choice.order, choice.outer, choice.inner]]),
            spline_order: choice.by_panel_size.then_some(choice.order),
            outer_quadrature: choice.by_panel_size.then_some(choice.outer),
            inner_quadrature: choice.by_panel_size.then_some(choice.inner),
            ..MeshFile::default()
        };
        let mut error = String::new();
        for zero_trailer in [false, true] {
            let mut m = Model::new();
            let read = m
                .load_mesh("mesh", &MeshFile { zero_trailer, ..base.clone() })
                .map_err(|e| e.message().to_string())
                .and_then(|id| Mesh::from_object(&m, id));
            match read {
                Ok(mesh) => return Ok(mesh),
                Err(e) => error = e,
            }
        }
        Err(error)
    }

    /// Reads a geometry file (`.hbg`): its mesh kind and discretization are the file's. A patch
    /// mesh whose file has no discretization takes the rows of cad2bem's report beside it (by
    /// patch name), else `DEFAULT_DISCRETIZATION`.
    pub fn load_hbg(path: &Path) -> Result<Arc<Mesh>, String> {
        // a file of version 2 (cad2bem's) is read from its text as version 3, the mesh still the
        // file's (`hbg_upgrade`)
        let upgraded = std::fs::read_to_string(path).ok().and_then(|t| crate::hbg_upgrade::upgrade(&t));
        let temp = match &upgraded {
            Some(text) => {
                let dir = std::env::temp_dir().join("hydrobem-gui");
                std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                let stamp =
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
                let file = dir.join(format!("v3-{}-{stamp}.hbg", std::process::id()));
                std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
                Some(file)
            }
            None => None,
        };
        let source = temp.as_deref().unwrap_or(path);
        let read = |discretization: Option<Vec<[i32; 6]>>| -> Result<Mesh, String> {
            let mut m = Model::new();
            let file = MeshFile { path: source.to_path_buf(), discretization, ..MeshFile::default() };
            let id = m
                .load_mesh("mesh", &file)
                .map_err(|e| e.message().to_string().replace(&*source.to_string_lossy(), &path.to_string_lossy()))?;
            let mut mesh = Mesh::from_object(&m, id)?;
            if temp.is_some() {
                mesh.source_file = path.to_string_lossy().into_owned();
            }
            Ok(mesh)
        };
        let result = Mesh::load_hbg_with(read);
        if let Some(file) = &temp {
            let _ = std::fs::remove_file(file);
        }
        result
    }

    /// `load_hbg` with the reader of the file at a discretization.
    fn load_hbg_with(read: impl Fn(Option<Vec<[i32; 6]>>) -> Result<Mesh, String>) -> Result<Arc<Mesh>, String> {
        let mut mesh = match read(None) {
            Ok(mut mesh) => {
                mesh.rows_from = RowsFrom::File;
                mesh
            }
            Err(first) => {
                let mut mesh = read(Some(vec![DEFAULT_DISCRETIZATION])).map_err(|_| first)?.with_report();
                mesh.rows_from = RowsFrom::Default;
                let rows = mesh.report.as_ref().and_then(|r| r.discretization());
                match rows {
                    Some(rows) => {
                        let mut mesh = read(Some(rows))?;
                        mesh.rows_from = RowsFrom::Cad2bem;
                        mesh
                    }
                    None => mesh,
                }
            }
        };
        if !mesh.is_patch() {
            mesh.rows_from = RowsFrom::Unknown;
        }
        Ok(Arc::new(mesh.with_report()))
    }

    /// The mesh with cad2bem's report of its geometry file, when there is one beside it.
    pub fn with_report(mut self) -> Mesh {
        let source = Path::new(&self.source_file);
        if self.is_patch() && is_hbg(source) {
            self.report = crate::patch_report::PatchReport::beside(source, &self.patch_names()).map(Arc::new);
        }
        self
    }

    /// Writes the mesh as a geometry file (`.hbg`), replacing one there.
    pub fn save_hbg(&self, path: &Path) -> Result<(), String> {
        let run = || -> hydrobem_engine::Result<()> {
            let mut m = Model::new();
            let root = m.root();
            let id = m.add(root, "meshes", None, "mesh", None)?;
            self.write_object(&mut m, id)?;
            m.write_mesh(id, path, None, true)
        };
        run().map_err(|e| e.message().to_string())
    }

    /// The names of the patches of a patch mesh, in their order (empty for an unnamed patch, and
    /// none for a flat mesh, whose patches are its records).
    pub fn patch_names(&self) -> Vec<String> {
        let Some(p) = &self.patch else { return Vec::new() };
        p.object.children.iter().filter(|(list, _, _)| list == "patches").map(|(_, name, _)| name.clone()).collect()
    }

    /// Whether this is a higher-order (patch) mesh.
    pub fn is_patch(&self) -> bool {
        self.patch.is_some()
    }

    /// The discretization rows of a patch mesh (nu, nv, ku, kv, iquo, iqui), one per patch.
    pub fn discretization(&self) -> Vec<[i64; 6]> {
        let Some(Value::Ints { data, .. }) = self.patch.as_ref().and_then(|p| p.object.get("discretization")) else {
            return Vec::new();
        };
        data.as_chunks::<6>().0.to_vec()
    }

    /// The patch mesh read again from its higher-order GDF file with the discretization the engine
    /// makes from a panel size (m): per patch n = floor(L/s) + 1 spans along each direction, L the
    /// longer of its two edges there, spline order 3, Gauss orders 3 (outer) and 4 (inner), as
    /// WAMIT's PANEL_SIZE. A relative source file (a shipped model) is beside the project, in
    /// `project_dir`. Refused when the mesh is not from such a file, the file is gone, or its
    /// patches are no longer those of the mesh (the file changed: read it again first).
    pub fn with_panel_size(&self, size: f64, project_dir: Option<&Path>) -> Result<Arc<Mesh>, String> {
        if !(size > 0.0 && size.is_finite()) {
            return Err("the panel size is a positive length".into());
        }
        let given = Path::new(&self.source_file);
        let path = match project_dir {
            Some(dir) if given.is_relative() => dir.join(given),
            _ => given.to_path_buf(),
        };
        let path = path.as_path();
        if self.patch.is_none() || !path.is_file() || !is_higher_order_gdf(path) {
            return Err(
                "a panel size applies to a patch mesh read from a higher-order GDF file that is still there".into()
            );
        }
        let choice = GdfDiscretization { by_panel_size: true, panel_size: size, ..GdfDiscretization::default() };
        let mut mesh = Mesh::read_patch_gdf(path, &choice)?;
        if mesh.panels != self.panels || mesh.symmetry != self.symmetry {
            return Err(format!(
                "{} has changed since the mesh was read: read it again (Source File) first",
                path.display()
            ));
        }
        mesh.rows_from = RowsFrom::PanelSize(size);
        // the mesh's own source (relative in a shipped model) and title, in the object that is
        // saved too: not the absolute path the engine's reader sets
        mesh.label = self.label.clone();
        mesh.source_file = self.source_file.clone();
        if let Some(patch) = mesh.patch.as_mut() {
            patch.object.set("source_file", Value::Text(self.source_file.clone()));
            patch.object.set("label", Value::Text(self.label.clone()));
        }
        Ok(Arc::new(mesh))
    }

    /// The unknowns of the potential over the stored patches: per patch (nu + ku - 1)(nv + kv - 1)
    /// B-spline coefficients (WAMIT's NEQN for the same patches); none for a panel mesh.
    pub fn unknowns(&self) -> Option<i64> {
        self.is_patch().then(|| self.discretization().iter().map(|r| (r[0] + r[2] - 1) * (r[1] + r[3] - 1)).sum())
    }

    /// A copy with the discretization of patch `row` changed (a new mesh: its users take it).
    pub fn with_discretization(&self, row: usize, column: usize, value: i64) -> Option<Mesh> {
        let mut rows = self.discretization();
        *rows.get_mut(row)?.get_mut(column)? = value;
        let mut mesh = self.clone();
        let patch = mesh.patch.as_mut()?;
        patch.object.set("discretization", Value::ints(&[rows.len(), 6], rows.into_iter().flatten().collect()));
        mesh.id = next_id();
        mesh.rows_from = RowsFrom::Edited;
        Some(mesh)
    }

    /// A mesh made from panels (tests, generated surfaces).
    pub fn from_panels(label: &str, symmetry: Symmetry, panels: Vec<[V3; 4]>) -> Arc<Mesh> {
        Arc::new(Mesh {
            id: next_id(),
            source_file: String::new(),
            label: label.into(),
            reference_length: 1.0,
            file_gravity: None,
            symmetry,
            panels,
            patch: None,
            analytic: None,
            report: None,
            rows_from: RowsFrom::Unknown,
        })
    }

    /// The mesh object `id` of a model: a low-order surface mesh, or a patch mesh.
    pub fn from_object(m: &Model, id: ObjectId) -> Result<Mesh, String> {
        let text = |item: &str| -> String {
            m.get(id, item).ok().flatten().and_then(|v| v.as_text().map(str::to_string)).unwrap_or_default()
        };
        let kind = text("kind");
        if kind == "trimmed_patches" {
            return Err("a trimmed mesh (made by the trim of a body) is not kept by the GUI".into());
        }
        if kind != "panels" {
            return Mesh::patch_object(m, id, kind);
        }
        let vertices = m.get(id, "vertices").map_err(|e| e.message().to_string())?;
        let data = vertices.as_ref().and_then(Value::as_reals).unwrap_or(&[]);
        let panels = data
            .as_chunks::<12>()
            .0
            .iter()
            .map(|c| std::array::from_fn(|v| [c[3 * v], c[3 * v + 1], c[3 * v + 2]]))
            .collect();
        let symmetry = Symmetry::from_engine(&text("symmetry")).unwrap_or_default();
        let reference_length = m.get(id, "reference_length").ok().flatten().and_then(|v| v.as_real()).unwrap_or(1.0);
        let file_gravity = m.get(id, "file_gravity").ok().flatten().and_then(|v| v.as_real());
        Ok(Mesh {
            id: next_id(),
            source_file: text("source_file"),
            label: text("label"),
            reference_length,
            file_gravity,
            symmetry,
            panels,
            patch: None,
            analytic: None,
            report: None,
            rows_from: RowsFrom::Unknown,
        })
    }

    /// A patch mesh from its engine object: the object as read, its patches' corners and their
    /// tessellation.
    fn patch_object(m: &Model, id: ObjectId, kind: String) -> Result<Mesh, String> {
        let e = |e: hydrobem_engine::Error| e.message().to_string();
        let object = ObjectData::read(m, id)?;
        let elements = m.mesh_elements(id).map_err(e)?;
        let n = tessellation_steps(elements.vertices.len());
        let t = m.mesh_tessellation(id, false, n, n).map_err(e)?;
        let tessellation =
            t.vertices.iter().zip(&t.element).map(|(q, r)| ((*r).max(1) as usize - 1, *q)).collect::<Vec<_>>();
        let edges = boundary_edges(&t.vertices, &t.element);
        let normals = t.normals;
        let text = |item: &str| object.get(item).and_then(|v| v.as_text()).unwrap_or_default().to_string();
        Ok(Mesh {
            id: next_id(),
            source_file: text("source_file"),
            label: text("label"),
            reference_length: object.get("reference_length").and_then(Value::as_real).unwrap_or(1.0),
            file_gravity: object.get("file_gravity").and_then(Value::as_real),
            symmetry: Symmetry::from_engine(&text("symmetry")).unwrap_or_default(),
            panels: elements.vertices,
            patch: Some(PatchMesh { kind, object, tessellation, normals, edges }),
            analytic: None,
            report: None,
            rows_from: RowsFrom::Unknown,
        })
    }

    /// Writes this mesh into the mesh object `id` of a model.
    pub fn write_object(&self, m: &mut Model, id: ObjectId) -> hydrobem_engine::Result<()> {
        if let Some(patch) = &self.patch {
            return patch.object.write(m, id);
        }
        m.set(id, "kind", "panels")?;
        m.set(id, "symmetry", self.symmetry.engine_name())?;
        m.set(id, "reference_length", self.reference_length)?;
        m.set(id, "label", self.label.as_str())?;
        if let Some(g) = self.file_gravity {
            m.set(id, "file_gravity", g)?;
        }
        if !self.source_file.is_empty() {
            m.set(id, "source_file", self.source_file.as_str())?;
        }
        let data: Vec<f64> = self.panels.iter().flatten().flatten().copied().collect();
        m.set(id, "vertices", Value::reals(&[self.panels.len(), 4, 3], data))
    }

    /// The panels after reflection about x = 0 and/or y = 0, mesh coordinates. A copy with an odd
    /// number of reflections has its vertex order reversed, keeping the normals into the fluid.
    pub fn expanded(&self) -> Vec<[V3; 4]> {
        self.expanded_range(1, self.panels.len() as i64)
    }

    /// The stored records first..=last (one-based, as the engine selects them) in every symmetry
    /// copy; out-of-range bounds are clamped.
    pub fn expanded_range(&self, first: i64, last: i64) -> Vec<[V3; 4]> {
        self.expanded_records(first, last).into_iter().map(|(_, q)| q).collect()
    }

    /// `expanded_range` of first..=last and of each further range.
    pub fn expanded_ranges(&self, first: i64, last: i64, more: &[[i64; 2]]) -> Vec<[V3; 4]> {
        crate::project::record_ranges(first, last, more).iter().flat_map(|r| self.expanded_range(r[0], r[1])).collect()
    }

    /// As `expanded_range`, each quadrilateral with its stored record (zero-based): a patch mesh
    /// gives the quadrilaterals of its tessellation.
    pub fn expanded_records(&self, first: i64, last: i64) -> Vec<(usize, [V3; 4])> {
        let n = self.panels.len() as i64;
        let (a, b) = (first.clamp(1, n.max(1)), last.clamp(1, n.max(1)));
        if n == 0 || a > b {
            return Vec::new();
        }
        let mut out: Vec<(usize, [V3; 4])> = match &self.patch {
            // a patch is drawn by its tessellation
            Some(p) => p.tessellation.iter().filter(|(r, _)| (a - 1..b).contains(&(*r as i64))).copied().collect(),
            None => (a - 1..b).map(|r| (r as usize, self.panels[r as usize])).collect(),
        };
        let mirror = |out: &mut Vec<(usize, [V3; 4])>, axis: usize| {
            let images: Vec<(usize, [V3; 4])> = out
                .iter()
                .map(|(r, q)| {
                    let mut m = [q[3], q[2], q[1], q[0]];
                    m.iter_mut().for_each(|p| p[axis] = -p[axis]);
                    (*r, m)
                })
                .collect();
            out.extend(images);
        };
        if self.symmetry.x() {
            mirror(&mut out, 0);
        }
        if self.symmetry.y() {
            mirror(&mut out, 1);
        }
        out
    }

    /// A patch mesh as curved surfaces: every quadrilateral of its tessellation with its normals and
    /// patch-boundary sides, in every symmetry copy (mesh coordinates); empty for a panel mesh.
    pub fn expanded_patches(&self) -> Vec<SmoothQuad> {
        self.expanded_patches_with_records().into_iter().map(|(_, s)| s).collect()
    }

    /// `expanded_patches`, each quadrilateral with its stored patch (zero-based).
    pub fn expanded_patches_with_records(&self) -> Vec<(usize, SmoothQuad)> {
        let Some(p) = &self.patch else { return Vec::new() };
        let mut out: Vec<(usize, SmoothQuad)> = p
            .tessellation
            .iter()
            .zip(&p.normals)
            .zip(&p.edges)
            .map(|(((r, q), n), e)| (*r, SmoothQuad { corners: *q, normals: *n, edges: *e }))
            .collect();
        // a mirror image reverses the corner order; side k of the image is side 2 - k (mod 4)
        let mirror = |out: &mut Vec<(usize, SmoothQuad)>, axis: usize| {
            let images: Vec<(usize, SmoothQuad)> = out
                .iter()
                .map(|(r, s)| {
                    let flip = |v: [V3; 4]| {
                        let mut m = [v[3], v[2], v[1], v[0]];
                        m.iter_mut().for_each(|p| p[axis] = -p[axis]);
                        m
                    };
                    (
                        *r,
                        SmoothQuad {
                            corners: flip(s.corners),
                            normals: flip(s.normals),
                            edges: [s.edges[2], s.edges[1], s.edges[0], s.edges[3]],
                        },
                    )
                })
                .collect();
            out.extend(images);
        };
        if self.symmetry.x() {
            mirror(&mut out, 0);
        }
        if self.symmetry.y() {
            mirror(&mut out, 1);
        }
        out
    }

    /// The lines between the spans of the potential's discretization on a patch mesh, mesh
    /// coordinates, every symmetry copy: per patch the iso-parameter curves u = -1 + 2k/nu
    /// (0 < k < nu) and v = -1 + 2k/nv inside it (its boundary is drawn as the patch's outline).
    /// The engine divides each patch's parameter square [-1, 1]² evenly into nu x nv spans; the
    /// points come from the engine (`patch_points`). Empty for a low-order mesh.
    pub fn span_lines(&self) -> Result<Vec<[V3; 2]>, String> {
        let Some(patch) = &self.patch else { return Ok(Vec::new()) };
        let rows = self.discretization();
        let run = || -> hydrobem_engine::Result<Vec<[V3; 2]>> {
            let mut m = Model::new();
            let root = m.root();
            let id = m.add(root, "meshes", None, "mesh", None)?;
            patch.object.write(&mut m, id)?;
            let mut lines = Vec::new();
            for record in 0..self.panels.len() {
                // one row for every patch, or one per patch
                let Some(row) = rows.get(record).or_else(|| rows.first()) else { continue };
                let (nu, nv) = (row[0].max(1) as usize, row[1].max(1) as usize);
                // each line sampled finer than the spans it crosses, for the curvature
                let samples = |across: usize| (4 * across).clamp(8, 64);
                let mut uv: Vec<[f64; 2]> = Vec::new();
                let mut polylines = Vec::new();
                for (n, across, along_u) in [(nu, nv, true), (nv, nu, false)] {
                    let s = samples(across);
                    for k in 1..n {
                        let fixed = -1.0 + 2.0 * k as f64 / n as f64;
                        let start = uv.len();
                        for j in 0..=s {
                            let t = -1.0 + 2.0 * j as f64 / s as f64;
                            uv.push(if along_u { [fixed, t] } else { [t, fixed] });
                        }
                        polylines.push(start..uv.len());
                    }
                }
                if uv.is_empty() {
                    continue;
                }
                let points = m.patch_points(id, record as i32 + 1, &uv)?.points;
                for range in polylines {
                    lines.extend(points[range].windows(2).map(|w| [w[0], w[1]]));
                }
            }
            Ok(lines)
        };
        let mut lines = run().map_err(|e| e.message().to_string())?;
        // the symmetry copies, as the patches' (`expanded_patches`)
        let mirror = |lines: &mut Vec<[V3; 2]>, axis: usize| {
            let images: Vec<[V3; 2]> = lines
                .iter()
                .map(|l| {
                    l.map(|mut p| {
                        p[axis] = -p[axis];
                        p
                    })
                })
                .collect();
            lines.extend(images);
        };
        if self.symmetry.x() {
            mirror(&mut lines, 0);
        }
        if self.symmetry.y() {
            mirror(&mut lines, 1);
        }
        Ok(lines)
    }

    pub fn displayed_panels(&self) -> usize {
        self.panels.len() * if self.symmetry.x() { 2 } else { 1 } * if self.symmetry.y() { 2 } else { 1 }
    }

    /// A checksum of the geometry (FNV-1a over the vertices and the symmetry), for finding changes.
    pub fn checksum(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |bytes: &[u8]| {
            for b in bytes {
                hash = (hash ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        for v in self.panels.iter().flatten().flatten() {
            feed(&v.to_le_bytes());
        }
        feed(self.symmetry.engine_name().as_bytes());
        if let Some(p) = &self.patch {
            feed(format!("{:?}", p.object).as_bytes());
        }
        feed(&self.reference_length.to_le_bytes());
        feed(self.label.as_bytes());
        hash
    }

    /// The file name shown for this mesh.
    pub fn display_name(&self) -> String {
        Path::new(&self.source_file)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.label.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
    }

    #[test]
    fn a_gdf_file_reads_with_its_symmetry() {
        let m = Mesh::load(&data().join("cylinder.gdf")).expect("cylinder");
        assert_eq!(m.panels.len(), 192);
        assert_eq!(m.symmetry, Symmetry::XzAndYz);
        assert_eq!(m.displayed_panels(), 768);
        assert_eq!(m.expanded().len(), 768);
        assert_eq!(m.reference_length, 1.0);
        assert!(m.source_file.ends_with("cylinder.gdf"));
        assert!(m.label.starts_with("cylinder R = 1 m"), "{}", m.label);
    }

    #[test]
    fn a_range_covers_every_symmetry_copy() {
        let m = Mesh::load(&data().join("cylinder.gdf")).expect("cylinder");
        let r = m.expanded_range(1, 6);
        assert_eq!(r.len(), 24);
        // the x mirror of the first record reverses its order and the sign of x
        assert_eq!(r[6][0][0], -m.panels[0][3][0]);
        // a range past the last record ends there
        assert_eq!(m.expanded_range(100, 500).len(), 93 * 4);
    }

    #[test]
    fn a_missing_file_names_the_file() {
        let e = Mesh::load(&data().join("no_such.gdf")).unwrap_err();
        assert!(e.contains("no_such.gdf"), "{e}");
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("hydrobem-gui-mesh-tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn a_geometry_file_keeps_the_mesh() {
        // a surface mesh
        let gdf = Mesh::load(&data().join("cylinder.gdf")).expect("cylinder");
        let path = scratch("cylinder.hbg");
        gdf.save_hbg(&path).expect("write");
        let hbg = Mesh::load(&path).expect("read");
        assert_eq!(hbg.panels, gdf.panels);
        assert_eq!(hbg.symmetry, Symmetry::XzAndYz);
        assert!(hbg.source_file.ends_with("cylinder.hbg"), "{}", hbg.source_file);

        // a patch mesh with its discretization
        let mut spec = crate::analytic::AnalyticSpec::of(crate::analytic::AnalyticKind::Barge);
        spec.dimensions = vec![3.0, 1.5, 0.75];
        let rows = vec![[2, 3, 3, 3, 3, 4], [4, 2, 3, 3, 3, 4], [3, 3, 3, 3, 3, 4], [2, 2, 3, 3, 3, 4]];
        let analytic = spec.build(Some(rows.clone())).expect("barge shape");
        let path = scratch("barge_patches.hbg");
        analytic.save_hbg(&path).expect("write");
        let back = Mesh::load(&path).expect("read");
        assert!(back.is_patch());
        assert_eq!(back.panels, analytic.panels);
        let rows: Vec<[i64; 6]> = rows.iter().map(|r| r.map(i64::from)).collect();
        assert_eq!(back.discretization(), rows);
        assert_eq!(back.patch_names(), analytic.patch_names());
    }

    #[test]
    fn a_geometry_file_without_a_discretization_takes_the_default() {
        let side = r#"{"name": "face_12", "u_order": 3, "v_order": 2, "u_knots": [0, 0, 0, 1, 1, 1],
            "v_knots": [0, 0, 1, 1],
            "coefficients": [[[1, 0, 0], [1, 1, 0], [0, 1, 0]], [[1, 0, -0.5], [1, 1, -0.5], [0, 1, -0.5]]],
            "weights": [[1, 0.7071067811865476, 1], [1, 0.7071067811865476, 1]]}"#;
        let text = format!(
            r#"{{"format": "hbg", "version": 2, "kind": "spline_patches", "symmetry": "xy", "patches": [{side}]}}"#
        );
        let path = scratch("quarter_side.hbg");
        std::fs::write(&path, text).unwrap();
        let mesh = Mesh::load(&path).expect("read");
        assert_eq!(mesh.discretization(), vec![DEFAULT_DISCRETIZATION.map(i64::from)]);
        assert_eq!(mesh.patch_names(), vec!["face_12".to_string()]);

        // a file the engine refuses names its member
        std::fs::write(&path, r#"{"format": "hbg", "version": 2, "kind": "spline_patches", "colour": 1}"#).unwrap();
        let e = Mesh::load(&path).unwrap_err();
        assert!(e.contains("quarter_side.hbg"), "{e}");
    }

    #[test]
    fn a_geometry_file_takes_the_rows_of_cad2bems_report() {
        let side = r#"{"name": "face_12", "u_order": 3, "v_order": 2, "u_knots": [0, 0, 0, 1, 1, 1],
            "v_knots": [0, 0, 1, 1],
            "coefficients": [[[1, 0, 0], [1, 1, 0], [0, 1, 0]], [[1, 0, -0.5], [1, 1, -0.5], [0, 1, -0.5]]],
            "weights": [[1, 0.7071067811865476, 1], [1, 0.7071067811865476, 1]]}"#;
        let path = scratch("cad2bem_side.hbg");
        let file = |rows: &str| {
            format!(
                r#"{{"format": "hbg", "version": 2, "kind": "spline_patches", "symmetry": "xy"{rows}, "patches": [{side}]}}"#
            )
        };
        std::fs::write(&path, file("")).unwrap();
        std::fs::write(
            crate::patch_report::report_path(&path),
            r#"{"format": "hopatch-report", "version": 1, "summary": {"state": "warn"}, "patches": [{"name": "face_12",
               "face": 12, "face_type": "Cylinder", "state": "warn", "checks": {"gap": "warn"},
               "discretization": [5, 2, 3, 3, 3, 4]}]}"#,
        )
        .unwrap();
        let mesh = Mesh::load(&path).expect("read");
        assert_eq!(mesh.discretization(), vec![[5, 2, 3, 3, 3, 4]]);
        let report = mesh.report.as_ref().expect("report");
        assert_eq!((report.state.as_str(), report.patches[0].face), ("warn", 12));
        assert_eq!(report.patches[0].problems, vec!["gap warn".to_string()]);

        // the file's own rows are kept, the checks still shown
        std::fs::write(&path, file(r#", "discretization": [[2, 2, 3, 3, 3, 4]]"#)).unwrap();
        let mesh = Mesh::load(&path).expect("read");
        assert_eq!(mesh.discretization(), vec![[2, 2, 3, 3, 3, 4]]);
        assert!(mesh.report.is_some());
    }
}
