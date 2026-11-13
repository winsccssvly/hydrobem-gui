//! A model of the engine: its objects and items, meshes, queries, solve and results.
#![allow(unsafe_code)] // the calls of the DLL

use std::ffi::{CString, c_char, c_void};
use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::codes::{ModelState, Phase, PointStatus, SecondOrderStep, Stage};
use crate::error::{Diagnostic, Error, ErrorKind, Result};
use crate::ffi::{self, Api, HbProgress};
use crate::results::Results;
use crate::table::{Table, api, c, c_path, check, owned_text, table};
use crate::types::*;

struct Handle {
    a: &'static Api,
    p: *mut ffi::Model,
}

// SAFETY: the DLL locks a model in each call, from any thread; hb_model_solve releases the lock and other
// threads may read, cancel and poll meanwhile (engine/hydrobem.h: hb_model_cancel,
// hb_model_progress, hb_model_solve)
unsafe impl Send for Handle {}
// SAFETY: as Send
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a model of the library, freed once by its last owner; no solve runs then: a solve
        // borrows the `Model` mutably, and the last `Arc` is dropped after it
        let status = unsafe { (self.a.hb_model_free)(self.p) };
        debug_assert_eq!(status, 0, "hb_model_free refused the model");
    }
}

/// Cancels the solve of a model and reads its progress, from any thread.
#[derive(Clone)]
pub struct CancelToken(Arc<Handle>);

impl CancelToken {
    /// Requests the cancellation of the running solve, or of the next one; never waits.
    pub fn cancel(&self) {
        // SAFETY: a model of the library
        unsafe { (self.0.a.hb_model_cancel)(self.0.p) };
    }

    /// The progress of the running solve, or of the last one, at this instant (never waits).
    pub fn progress(&self) -> Result<SolveProgress> {
        let p = self.0.p;
        // SAFETY: a model of the library, a writable pointer
        SolveProgress::from_table(&table(|a, out| unsafe { (a.hb_model_progress)(p, out) })?)
    }
}

/// A model of the engine.
pub struct Model {
    h: Arc<Handle>,
    /// the results of the last solve, fetched once; dropped with every change of the inputs
    results: OnceLock<Results>,
}

impl Default for Model {
    fn default() -> Model {
        Model::new()
    }
}

fn texts_c(v: &[impl AsRef<str>]) -> Result<Vec<CString>> {
    v.iter().map(|s| c(s.as_ref())).collect()
}

fn pointers(v: &[CString]) -> Vec<*const c_char> {
    v.iter().map(|s| s.as_ptr()).collect()
}

fn opt_ptr(s: &Option<CString>) -> *const c_char {
    s.as_ref().map_or(std::ptr::null(), |s| s.as_ptr())
}

/// The calculation of the wave number k of ω² = g k tanh(k h) (h = depth, infinite allowed).
pub fn wavenumber(omega: f64, depth: f64, gravity: f64) -> Result<f64> {
    let a = api()?;
    let mut k = 0.0;
    // SAFETY: a writable real
    check(a, unsafe { (a.hb_wavenumber)(omega, depth, gravity, &mut k) })?;
    Ok(k)
}

impl Model {
    fn take(a: &'static Api, p: *mut ffi::Model) -> Result<Model> {
        if p.is_null() {
            return Err(Error::new(ErrorKind::Internal, "hydrobem.dll handed out no model"));
        }
        Ok(Model { h: Arc::new(Handle { a, p }), results: OnceLock::new() })
    }

    pub(crate) fn make(f: impl FnOnce(&'static Api, *mut *mut ffi::Model) -> ffi::Status) -> Result<Model> {
        let a = api()?;
        let mut p = std::ptr::null_mut();
        let status = f(a, &mut p);
        check(a, status)?;
        Model::take(a, p)
    }

    /// A new empty model (hydrobem.dll must load: the application checks it at start-up).
    pub fn new() -> Model {
        // SAFETY: a writable pointer
        Model::make(|a, out| unsafe { (a.hb_model_new)(out) }).expect("hydrobem.dll makes a model")
    }

    /// A model read from a model file (.hbm), or the inputs of a results file (.hbr).
    pub fn load(path: &Path) -> Result<Model> {
        let path = c_path(path)?;
        // SAFETY: a NUL-terminated path, a writable pointer
        Model::make(|a, out| unsafe { (a.hb_model_load)(path.as_ptr(), out) })
    }

    /// A calculated model from a results file (.hbr): its inputs and its results.
    pub fn load_results(path: &Path) -> Result<Model> {
        let path = c_path(path)?;
        // SAFETY: as load
        Model::make(|a, out| unsafe { (a.hb_model_load_results)(path.as_ptr(), out) })
    }

    /// A new model of a copy of the inputs (no results).
    pub fn clone_inputs(&self) -> Result<Model> {
        let p = self.p();
        // SAFETY: a model of the library, a writable pointer
        Model::make(|a, out| unsafe { (a.hb_model_copy)(p, out) })
    }

    fn p(&self) -> *mut ffi::Model {
        self.h.p
    }

    fn a(&self) -> &'static Api {
        self.h.a
    }

    /// A call of the DLL on this model.
    fn call(&self, f: impl FnOnce(&'static Api, *mut ffi::Model) -> ffi::Status) -> Result<()> {
        check(self.a(), f(self.a(), self.p()))
    }

    /// A call that changes the inputs (the results are dropped).
    fn change(&mut self, f: impl FnOnce(&'static Api, *mut ffi::Model) -> ffi::Status) -> Result<()> {
        self.results = OnceLock::new();
        self.call(f)
    }

    /// A call that hands out a table.
    fn query(
        &self,
        f: impl FnOnce(&'static Api, *mut ffi::Model, *mut *mut ffi::Table) -> ffi::Status,
    ) -> Result<Table> {
        let p = self.p();
        table(|a, out| f(a, p, out))
    }

    /// A call that hands out an object id.
    fn object_call(
        &mut self,
        f: impl FnOnce(&'static Api, *mut ffi::Model, *mut u64) -> ffi::Status,
    ) -> Result<ObjectId> {
        self.results = OnceLock::new();
        let mut id = ffi::HB_NO_OBJECT;
        self.call(|a, p| f(a, p, &mut id))?;
        Ok(ObjectId(id))
    }

    /// A call that hands out a text.
    fn text_call(
        &self,
        f: impl FnOnce(&'static Api, *mut ffi::Model, *mut *mut c_char) -> ffi::Status,
    ) -> Result<String> {
        let mut out = std::ptr::null_mut();
        self.call(|a, p| f(a, p, &mut out))?;
        // SAFETY: a text of the library
        Ok(unsafe { owned_text(self.a(), out) })
    }

    pub fn save(&self, path: &Path, overwrite: bool) -> Result<()> {
        let path = c_path(path)?;
        // SAFETY: a model of the library, a NUL-terminated path
        self.call(|a, p| unsafe { (a.hb_model_save)(p, path.as_ptr(), i32::from(overwrite)) })
    }

    /// Writes the model with its results as a results file (.hbr).
    pub fn save_results(&self, path: &Path, overwrite: bool) -> Result<()> {
        let path = c_path(path)?;
        // SAFETY: as save
        self.call(|a, p| unsafe { (a.hb_model_save_results)(p, path.as_ptr(), i32::from(overwrite)) })
    }

    pub fn state(&self) -> ModelState {
        let mut s = 0;
        // SAFETY: a model of the library, a writable state
        match self.call(|a, p| unsafe { (a.hb_model_state)(p, &mut s) }) {
            Ok(()) if s == 1 => ModelState::Calculating,
            Ok(()) if s == 2 => ModelState::Calculated,
            _ => ModelState::Reset,
        }
    }

    /// Cancels the solve and reads its progress from another thread.
    pub fn cancel_token(&self) -> CancelToken {
        CancelToken(self.h.clone())
    }

    /// The results of the last solve (`ErrorKind::State` when the model is not calculated).
    pub fn results(&self) -> Result<&Results> {
        if let Some(r) = self.results.get() {
            return Ok(r);
        }
        let mut out = std::ptr::null_mut();
        // SAFETY: a model of the library, a writable pointer
        self.call(|a, p| unsafe { (a.hb_model_results)(p, &mut out) })?;
        let r = Results::take(self.a(), out)?;
        Ok(self.results.get_or_init(|| r))
    }

    /// Discards the results, the inputs kept.
    pub fn reset(&mut self) -> Result<()> {
        // SAFETY: a model of the library
        self.change(|a, p| unsafe { (a.hb_model_reset)(p) })
    }

    /// Every input and the results back to those of a new model.
    pub fn clear(&mut self) -> Result<()> {
        // SAFETY: a model of the library
        self.change(|a, p| unsafe { (a.hb_model_clear)(p) })
    }

    /// The rules of the model; its warnings are then the diagnostics.
    pub fn check(&mut self) -> Result<()> {
        // SAFETY: a model of the library; no table asked for
        self.call(|a, p| unsafe { (a.hb_model_check)(p, std::ptr::null_mut()) })
    }

    /// The warnings of the last rule check.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        // SAFETY: a model of the library, a writable pointer
        self.query(|a, p, out| unsafe { (a.hb_model_diagnostics)(p, out) })
            .and_then(|t| Diagnostic::from_table(&t))
            .unwrap_or_default()
    }

    /// The route the solver takes for the model.
    pub fn route(&mut self) -> Result<String> {
        // SAFETY: a model of the library, a writable pointer
        self.text_call(|a, p, out| unsafe { (a.hb_model_route)(p, out) })
    }

    /// Solves the model on this thread, `progress` called with each report (maybe from another
    /// thread, one call at a time). A panic of `progress` cancels the solve and goes on here once
    /// the DLL has returned.
    pub fn solve<F: FnMut(&Progress) + Send>(&mut self, progress: F) -> Result<&Results> {
        /// The caller's closure and the panic it raised.
        struct User<F> {
            f: F,
            panic: Option<Box<dyn std::any::Any + Send>>,
        }
        unsafe extern "C" fn report<F: FnMut(&Progress)>(p: *const HbProgress, user: *mut c_void) -> i32 {
            // SAFETY: the report of the DLL and the closure given to hb_model_solve, one call at a time
            let (p, u) = unsafe { (&*p, &mut *user.cast::<User<F>>()) };
            if u.panic.is_some() {
                return 0;
            }
            let count = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
            let index = |i: i64| usize::try_from(i).ok();
            // -1 a limit, -2 the statics, -3 an item of a second-order step, -4 the solve as a whole
            let at = match p.frequency {
                -1 => ProgressAt::Limit,
                -2 => ProgressAt::Statics,
                -3 => match SecondOrderStep::from_code(p.step) {
                    Some(step) => ProgressAt::SecondOrder { step, item: index(p.item) },
                    None => ProgressAt::Solve,
                },
                i if i < 0 => ProgressAt::Solve,
                i => ProgressAt::Frequency(count(i as u64)),
            };
            let r = Progress {
                stage: Stage::from_code(p.stage),
                at,
                stage_done: count(p.stage_done),
                stage_total: count(p.stage_total),
                frequencies_done: count(p.frequencies_done),
                frequencies_total: count(p.frequencies_total),
                seconds: p.seconds,
                worker: index(i64::from(p.worker)),
                phase: Phase::from_code(p.phase),
            };
            // a panic must not cross into the DLL: it cancels the solve and is kept
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (u.f)(&r))) {
                Ok(()) => 1,
                Err(panic) => {
                    u.panic = Some(panic);
                    0
                }
            }
        }
        self.results = OnceLock::new();
        let mut u = User { f: progress, panic: None };
        let user = (&mut u as *mut User<F>).cast::<c_void>();
        let mut out = std::ptr::null_mut();
        // SAFETY: a model of the library; the report function of F with its closure, which lives
        // until hb_model_solve returns; a writable pointer
        let status = self.call(|a, p| unsafe { (a.hb_model_solve)(p, Some(report::<F>), user, &mut out) });
        if let Some(panic) = u.panic.take() {
            if !out.is_null() {
                drop(Results::take(self.a(), out));
            }
            std::panic::resume_unwind(panic);
        }
        status?;
        let r = Results::take(self.a(), out)?;
        Ok(self.results.get_or_init(|| r))
    }

    /// The model itself.
    pub fn root(&self) -> ObjectId {
        self.object("").unwrap_or(ObjectId(ffi::HB_NO_OBJECT))
    }

    /// The object of a path ("" the model, "options", "bodies/hull").
    pub fn object(&self, path: &str) -> Result<ObjectId> {
        let path = c(path)?;
        let mut id = ffi::HB_NO_OBJECT;
        // SAFETY: a model of the library, a NUL-terminated path, a writable id
        self.call(|a, p| unsafe { (a.hb_object)(p, path.as_ptr(), &mut id) })?;
        Ok(ObjectId(id))
    }

    pub fn object_info(&self, id: ObjectId) -> Result<ObjectInfo> {
        // SAFETY: a model of the library, a writable pointer
        let t = self.query(|a, p, out| unsafe { (a.hb_object_info)(p, id.0, out) })?;
        let parent = t.u64("parent")?;
        Ok(ObjectInfo {
            type_name: t.text("type")?,
            name: t.text("name")?,
            list: t.text("list")?,
            index: t.count("index")?,
            path: t.text("path")?,
            parent: (parent != ffi::HB_NO_OBJECT).then_some(ObjectId(parent)),
        })
    }

    /// The path of an object ("bodies/hull").
    pub fn path(&self, id: ObjectId) -> Result<String> {
        Ok(self.object_info(id)?.path)
    }

    /// The parent of an object (None for the model).
    pub fn parent(&self, id: ObjectId) -> Result<Option<ObjectId>> {
        Ok(self.object_info(id)?.parent)
    }

    /// The objects of a list of an object, in list order.
    pub fn children(&self, id: ObjectId, list: &str) -> Result<Vec<ObjectId>> {
        let list = c(list)?;
        // SAFETY: a model of the library, a NUL-terminated name, a writable pointer
        let t = self.query(|a, p, out| unsafe { (a.hb_children)(p, id.0, list.as_ptr(), out) })?;
        Ok(t.u64s("ids")?.into_iter().map(ObjectId).collect())
    }

    /// Adds an object to a list of an object: of a type (None: the list's first type), with a name
    /// ("" none, in a list whose objects may be unnamed), at a zero-based place (None: the end).
    pub fn add(
        &mut self,
        id: ObjectId,
        list: &str,
        type_: Option<&str>,
        name: &str,
        index: Option<usize>,
    ) -> Result<ObjectId> {
        let (list, name) = (c(list)?, c(name)?);
        let type_ = type_.map(c).transpose()?;
        let index = index.map_or(-1, |i| i64::try_from(i).unwrap_or(i64::MAX));
        // SAFETY: a model of the library, NUL-terminated texts (or null), a writable id
        self.object_call(|a, p, out| unsafe {
            (a.hb_add)(p, id.0, list.as_ptr(), opt_ptr(&type_), name.as_ptr(), index, out)
        })
    }

    /// Removes an object and its children.
    pub fn remove(&mut self, id: ObjectId) -> Result<()> {
        // SAFETY: a model of the library
        self.change(|a, p| unsafe { (a.hb_remove)(p, id.0) })
    }

    /// Moves an object to a zero-based place of its list.
    pub fn move_to(&mut self, id: ObjectId, index: usize) -> Result<()> {
        let index = i64::try_from(index).unwrap_or(i64::MAX);
        // SAFETY: a model of the library
        self.change(|a, p| unsafe { (a.hb_move)(p, id.0, index) })
    }

    /// Removes every object of a list of an object (all or nothing; refused while another object
    /// refers to one of them).
    pub fn clear_list(&mut self, id: ObjectId, list: &str) -> Result<()> {
        let list = c(list)?;
        // SAFETY: a model of the library, a NUL-terminated name
        self.change(|a, p| unsafe { (a.hb_clear)(p, id.0, list.as_ptr()) })
    }

    pub fn rename(&mut self, id: ObjectId, name: &str) -> Result<()> {
        let name = c(name)?;
        // SAFETY: a model of the library, a NUL-terminated name
        self.change(|a, p| unsafe { (a.hb_rename)(p, id.0, name.as_ptr()) })
    }

    pub fn item_info(&self, id: ObjectId, item: &str) -> Result<ItemInfo> {
        let item = c(item)?;
        // SAFETY: a model of the library, a NUL-terminated name, a writable pointer
        let t = self.query(|a, p, out| unsafe { (a.hb_item_info)(p, id.0, item.as_ptr(), out) })?;
        Ok(ItemInfo {
            kind: t.text("kind")?,
            is_set: t.bool("is_set")?,
            has_value: t.bool("has_value")?,
            is_array: t.bool("is_array")?,
            dtype: t.text("dtype")?,
            shape: t.i64s("shape")?.into_iter().map(|d| usize::try_from(d).unwrap_or(0)).collect(),
            is_choice: t.bool("is_choice")?,
        })
    }

    /// The value of an item (its default when it is not set; None: no value).
    pub fn get(&self, id: ObjectId, item: &str) -> Result<Option<Value>> {
        let item = c(item)?;
        // SAFETY: a model of the library, a NUL-terminated name, a writable pointer
        let t = self.query(|a, p, out| unsafe { (a.hb_get)(p, id.0, item.as_ptr(), out) })?;
        Value::from_table(&t)
    }

    /// Sets an item; the engine checks the value.
    pub fn set(&mut self, id: ObjectId, item: &str, value: impl Into<Value>) -> Result<()> {
        let item = c(item)?;
        let i = item.as_ptr();
        let o = id.0;
        let value = value.into();
        // the DLL reads the product of the shape's elements: a shorter array would be read past
        let counts = match &value {
            Value::Reals { shape, data } => Some((shape.iter().product::<usize>(), data.len())),
            Value::Ints { shape, data } => Some((shape.iter().product::<usize>(), data.len())),
            _ => None,
        };
        if let Some((wanted, given)) = counts
            && wanted != given
        {
            return Err(Error::new(ErrorKind::Argument, format!("{given} values for a shape of {wanted}")));
        }
        match value {
            // SAFETY (each arm): a model of the library, NUL-terminated texts, arrays of the
            // counts passed
            Value::Bool(b) => self.change(|a, p| unsafe { (a.hb_set_bool)(p, o, i, i32::from(b)) }),
            Value::Int(x) => self.change(|a, p| unsafe { (a.hb_set_int)(p, o, i, x) }),
            Value::Real(x) => self.change(|a, p| unsafe { (a.hb_set_real)(p, o, i, x) }),
            Value::Text(t) => {
                let t = c(&t)?;
                self.change(|a, p| unsafe { (a.hb_set_text)(p, o, i, t.as_ptr()) })
            }
            Value::Texts(v) => {
                let v = texts_c(&v)?;
                let ptrs = pointers(&v);
                self.change(|a, p| unsafe { (a.hb_set_texts)(p, o, i, ptrs.as_ptr(), ptrs.len()) })
            }
            Value::Reals { shape, data } => {
                self.change(|a, p| unsafe { (a.hb_set_reals)(p, o, i, data.as_ptr(), shape.as_ptr(), shape.len()) })
            }
            Value::Ints { shape, data } => {
                self.change(|a, p| unsafe { (a.hb_set_ints)(p, o, i, data.as_ptr(), shape.as_ptr(), shape.len()) })
            }
        }
    }

    /// Sets a surface: the mesh item (`mesh`, `control_surface_mesh`, ...) to a mesh by name and
    /// its ranges item (`ranges`, `control_surface_ranges`, ...) to records of it (one-based first
    /// and last; none: every record, the ranges item reset).
    pub fn set_surface(
        &mut self,
        id: ObjectId,
        mesh_item: &str,
        ranges_item: &str,
        mesh: &str,
        ranges: &[[i64; 2]],
    ) -> Result<()> {
        self.set(id, mesh_item, mesh)?;
        if ranges.is_empty() {
            self.reset_item(id, ranges_item)
        } else {
            self.set(id, ranges_item, Value::ranges(ranges))
        }
    }

    /// Returns an item to its default (or unsets it).
    pub fn reset_item(&mut self, id: ObjectId, item: &str) -> Result<()> {
        let item = c(item)?;
        // SAFETY: a model of the library, a NUL-terminated name
        self.change(|a, p| unsafe { (a.hb_reset)(p, id.0, item.as_ptr()) })
    }

    /// A mesh read from a file (GDF with its SPL file, or a geometry file .hbg).
    pub fn load_mesh(&mut self, name: &str, file: &MeshFile) -> Result<ObjectId> {
        let (name, path) = (c(name)?, c_path(&file.path)?);
        let spl = file.spl_path.as_deref().map(c_path).transpose()?;
        let rows = file.discretization.clone().unwrap_or_default();
        let options = ffi::HbMeshOptions {
            higher_order: i32::from(file.higher_order),
            zero_trailer: i32::from(file.zero_trailer),
            spl_path: opt_ptr(&spl),
            discretization: if rows.is_empty() { std::ptr::null() } else { rows.as_flattened().as_ptr() },
            discretization_rows: rows.len(),
            panel_size: file.panel_size.unwrap_or(0.0),
            spline_order: file.spline_order.unwrap_or(0),
            outer_quadrature: file.outer_quadrature.unwrap_or(0),
            inner_quadrature: file.inner_quadrature.unwrap_or(0),
        };
        // SAFETY: a model of the library, NUL-terminated texts, the options and their arrays
        // alive during the call, a writable id
        self.object_call(|a, p, out| unsafe { (a.hb_model_load_mesh)(p, name.as_ptr(), path.as_ptr(), &options, out) })
    }

    /// A mesh from the text of a geometry file (.hbg).
    pub fn hbg_mesh(&mut self, name: &str, text: &str) -> Result<ObjectId> {
        let (name, text) = (c(name)?, c(text)?);
        // SAFETY: a model of the library, NUL-terminated texts, a writable id
        self.object_call(|a, p, out| unsafe { (a.hb_model_mesh_from_hbg)(p, name.as_ptr(), text.as_ptr(), out) })
    }

    /// The text of the geometry file (.hbg) of a mesh.
    pub fn mesh_hbg(&self, mesh: ObjectId) -> Result<String> {
        // SAFETY: a model of the library, a writable pointer
        self.text_call(|a, p, out| unsafe { (a.hb_mesh_hbg)(p, mesh.0, out) })
    }

    /// A mesh of a named analytic shape.
    pub fn analytic_mesh(&mut self, name: &str, shape: &AnalyticShape) -> Result<ObjectId> {
        let name = c(name)?;
        let shape_name = c(&shape.shape)?;
        let dim_names = texts_c(&shape.dimensions.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>())?;
        let dim_name_ptrs = pointers(&dim_names);
        let dim_values: Vec<f64> = shape.dimensions.iter().map(|(_, v)| *v).collect();
        let parts = texts_c(&shape.parts)?;
        let part_ptrs = pointers(&parts);
        let mapping = c(&shape.mapping)?;
        let symmetry = c(&shape.symmetry)?;
        let label = (!shape.label.is_empty()).then(|| c(&shape.label)).transpose()?;
        let s = ffi::HbAnalyticShape {
            shape: shape_name.as_ptr(),
            dimension_names: dim_name_ptrs.as_ptr(),
            dimension_values: dim_values.as_ptr(),
            dimensions: dim_values.len(),
            discs: shape.discs.as_flattened().as_ptr(),
            disc_count: shape.discs.len(),
            parts: part_ptrs.as_ptr(),
            part_count: part_ptrs.len(),
            mapping: mapping.as_ptr(),
            discretization: shape.discretization.as_flattened().as_ptr(),
            discretization_rows: shape.discretization.len(),
            reference_length: shape.reference_length,
            symmetry: symmetry.as_ptr(),
            label: opt_ptr(&label),
        };
        // SAFETY: a model of the library, the shape and its arrays alive during the call, a
        // writable id
        self.object_call(|a, p, out| unsafe { (a.hb_model_analytic_mesh)(p, name.as_ptr(), &s, out) })
    }

    /// Writes a mesh: a path ending in .hbg the geometry file, otherwise GDF (with the SPL file).
    pub fn write_mesh(&self, mesh: ObjectId, gdf_path: &Path, spl_path: Option<&Path>, overwrite: bool) -> Result<()> {
        let path = c_path(gdf_path)?;
        let spl = spl_path.map(c_path).transpose()?;
        // SAFETY: a model of the library, NUL-terminated paths (or null)
        self.call(|a, p| unsafe { (a.hb_mesh_write)(p, mesh.0, path.as_ptr(), opt_ptr(&spl), i32::from(overwrite)) })
    }

    /// The sizes of the problem.
    pub fn counts(&mut self) -> Result<Counts> {
        // SAFETY: a model of the library, a writable pointer
        let t = self.query(|a, p, out| unsafe { (a.hb_model_counts)(p, out) })?;
        Ok(Counts {
            bodies: t.count("bodies")?,
            meshes: t.count("meshes")?,
            lids: t.count("lids")?,
            ordinary_frequencies: t.count("ordinary_frequencies")?,
            headings: t.count("headings")?,
            coordinates: t.count("coordinates")?,
            radiation_columns: t.count("radiation_columns")?,
            field_points: t.count("field_points")?,
            tank_points: t.count("tank_points")?,
            lid_elements: t.count("lid_elements")?,
            pressure_elements: t.count("pressure_elements")?,
            has_zero_frequency: t.bool("has_zero_frequency")?,
            has_infinite_frequency: t.bool("has_infinite_frequency")?,
            radiation_mode_ids: t.i32s("radiation_mode_ids")?,
            per_lid_elements: t.i32s("per_lid_elements")?,
        })
    }

    /// The motion equation (`hb_model_motion_equation`, HydroBEM theory §3.1): the rigid-body
    /// inertia, stiffness (hydrostatic, weight and additional) and additional damping, 6N x 6N in
    /// body axes about each rotation centre; None when it is not ready (a body without a centre of
    /// gravity, a mass or inertia).
    pub fn motion_equation(&mut self) -> Result<Option<MotionEquation>> {
        // SAFETY: a model of the library, a writable pointer
        let t = self.query(|a, p, out| unsafe { (a.hb_model_motion_equation)(p, out) })?;
        if !t.bool("ready")? {
            return Ok(None);
        }
        let shape = t.shape("inertia")?;
        Ok(Some(MotionEquation {
            size: shape.first().copied().unwrap_or(0),
            inertia: t.f64s("inertia")?,
            stiffness: t.f64s("stiffness")?,
            damping: t.f64s("damping")?,
        }))
    }

    /// The mode ids of the radiation columns.
    pub fn radiation_mode_ids(&mut self) -> Result<Vec<i32>> {
        Ok(self.counts()?.radiation_mode_ids)
    }

    /// The elements of each lid.
    pub fn lid_elements(&mut self) -> Result<Vec<i32>> {
        Ok(self.counts()?.per_lid_elements)
    }

    fn role(role: Option<&str>) -> Result<Option<CString>> {
        role.map(c).transpose()
    }

    /// The elements as the engine uses them (after a trim) of every body, then of no body (the lids
    /// not placed with a body, the fixed dampers, the free surface), of the roles of `role` (names
    /// of `PanelRole`, separated by commas; None every role but the control surface and the free
    /// surface); with their reflections when `include_reflections`. The seven keys identify a row.
    pub fn panels(&mut self, role: Option<&str>, include_reflections: bool) -> Result<Panels> {
        let role = Model::role(role)?;
        let r = i32::from(include_reflections);
        // SAFETY: a model of the library, a NUL-terminated role (or null), a writable pointer
        Panels::from_table(&self.query(|a, p, out| unsafe { (a.hb_model_panels)(p, opt_ptr(&role), r, out) })?)
    }

    /// The elements of a body as the engine uses them, as `panels`.
    pub fn body_panels(&mut self, body: ObjectId, role: Option<&str>, include_reflections: bool) -> Result<Panels> {
        let role = Model::role(role)?;
        let r = i32::from(include_reflections);
        // SAFETY: as panels
        Panels::from_table(&self.query(|a, p, out| unsafe { (a.hb_body_panels)(p, body.0, opt_ptr(&role), r, out) })?)
    }

    /// Quadrilaterals for drawing the elements of `panels` with the same role and reflections: a
    /// low-order panel one, a patch divisions_u x divisions_v on its exact surface (both 0: one
    /// per span of its potential discretization); each quad with the keys of its panels row.
    pub fn tessellation(
        &mut self,
        role: Option<&str>,
        include_reflections: bool,
        divisions_u: i32,
        divisions_v: i32,
    ) -> Result<Tessellation> {
        let role = Model::role(role)?;
        let r = i32::from(include_reflections);
        // SAFETY: a model of the library, a NUL-terminated role (or null), a writable pointer
        Tessellation::from_table(&self.query(|a, p, out| unsafe {
            (a.hb_model_tessellation)(p, opt_ptr(&role), divisions_u, divisions_v, r, out)
        })?)
    }

    /// The tessellation of the elements of a body, as `tessellation`.
    pub fn body_tessellation(
        &mut self,
        body: ObjectId,
        role: Option<&str>,
        include_reflections: bool,
        divisions_u: i32,
        divisions_v: i32,
    ) -> Result<Tessellation> {
        let role = Model::role(role)?;
        let r = i32::from(include_reflections);
        // SAFETY: as tessellation
        Tessellation::from_table(&self.query(|a, p, out| unsafe {
            (a.hb_body_tessellation)(p, body.0, opt_ptr(&role), divisions_u, divisions_v, r, out)
        })?)
    }

    /// The tessellation of a mesh alone, in mesh coordinates (role, body, tank and owner none).
    pub fn mesh_tessellation(
        &self,
        mesh: ObjectId,
        include_reflections: bool,
        divisions_u: i32,
        divisions_v: i32,
    ) -> Result<Tessellation> {
        let r = i32::from(include_reflections);
        // SAFETY: as tessellation
        Tessellation::from_table(
            &self
                .query(|a, p, out| unsafe { (a.hb_mesh_tessellation)(p, mesh.0, divisions_u, divisions_v, r, out) })?,
        )
    }

    /// The quality of the meshes of every body.
    pub fn mesh_quality(&mut self) -> Result<MeshQuality> {
        // SAFETY: a model of the library, a writable pointer
        MeshQuality::from_table(&self.query(|a, p, out| unsafe { (a.hb_model_mesh_quality)(p, out) })?)
    }

    /// The quality of the mesh of a body.
    pub fn body_mesh_quality(&mut self, body: ObjectId) -> Result<MeshQuality> {
        // SAFETY: a model of the library, a writable pointer
        MeshQuality::from_table(&self.query(|a, p, out| unsafe { (a.hb_body_mesh_quality)(p, body.0, out) })?)
    }

    /// The geometric quality of a mesh alone (no wavelength).
    pub fn mesh_geometry_quality(&self, mesh: ObjectId) -> Result<MeshQuality> {
        // SAFETY: a model of the library, a writable pointer
        MeshQuality::from_table(&self.query(|a, p, out| unsafe { (a.hb_mesh_geometry_quality)(p, mesh.0, out) })?)
    }

    /// The hydrostatics of a body after its trim and placement.
    pub fn body_hydrostatics(&mut self, body: ObjectId) -> Result<Hydrostatics> {
        // SAFETY: a model of the library, a writable pointer
        Hydrostatics::from_table(&self.query(|a, p, out| unsafe { (a.hb_body_hydrostatics)(p, body.0, out) })?)
    }

    /// The displacements of every coordinate at points of the bodies (global coordinates, the
    /// zero-based body, the one-based stored element): the values [n][d][3] and d, the coordinates
    /// (from the engine's shape).
    pub fn mode_displacements(
        &mut self,
        bodies: &[i32],
        elements: &[i32],
        points: &[[f64; 3]],
    ) -> Result<(Vec<f64>, usize)> {
        let n = points.len();
        if bodies.len() != n || elements.len() != n {
            return Err(Error::new(ErrorKind::Argument, "mode displacements: one body and element per point"));
        }
        let flat = points.as_flattened();
        // SAFETY: a model of the library, n points, bodies and elements, a writable pointer
        let t = self.query(|a, p, out| unsafe {
            (a.hb_model_mode_displacements)(p, flat.as_ptr(), bodies.as_ptr(), elements.as_ptr(), n, out)
        })?;
        let shape = t.shape("values")?;
        let coordinates = match shape[..] {
            [_, d, 3] => d,
            _ => return Err(Error::new(ErrorKind::Internal, format!("mode displacements of shape {shape:?}"))),
        };
        Ok((t.f64s("values")?, coordinates))
    }

    /// The memory of a solve in bytes.
    pub fn memory_estimate(&mut self) -> Result<MemoryEstimate> {
        // SAFETY: a model of the library, a writable pointer
        let t = self.query(|a, p, out| unsafe { (a.hb_model_memory_estimate)(p, out) })?;
        Ok(MemoryEstimate {
            statics: t.i64("statics")?,
            per_frequency: t.i64("per_frequency")?,
            outputs: t.i64("outputs")?,
            peak: t.i64("peak")?,
            commit: t.i64("commit")?,
        })
    }

    /// The verdict of the field-point rules on points: in the sea (`tank` None, global
    /// coordinates) or inside a tank of the model (body coordinates).
    pub fn classify_points(&mut self, points: &[[f64; 3]], tank: Option<ObjectId>) -> Result<Vec<PointClass>> {
        let n = points.len();
        let tank = tank.map_or(0, |t| t.0);
        let flat = points.as_flattened();
        // SAFETY: a model of the library, n points, a writable pointer
        let t = self.query(|a, p, out| unsafe { (a.hb_model_classify_points)(p, flat.as_ptr(), n, tank, out) })?;
        let (status, body, lid, tank) = (t.texts("status")?, t.i64s("body")?, t.i64s("lid")?, t.i64s("tank")?);
        let at = |v: &[i64], k: usize| v.get(k).and_then(|&i| usize::try_from(i).ok());
        status
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let status = PointStatus::from_name(s)
                    .ok_or_else(|| Error::new(ErrorKind::Internal, format!("point status {s}")))?;
                Ok(PointClass { status, body: at(&body, k), lid: at(&lid, k), tank: at(&tank, k) })
            })
            .collect()
    }

    /// The geometry of a mesh alone, in its own coordinates.
    pub fn mesh_geometry(&self, mesh: ObjectId) -> Result<MeshGeometry> {
        // SAFETY: a model of the library, a writable pointer
        MeshGeometry::from_table(&self.query(|a, p, out| unsafe { (a.hb_mesh_geometry)(p, mesh.0, out) })?)
    }

    /// The stored elements of a mesh.
    pub fn mesh_elements(&self, mesh: ObjectId) -> Result<MeshElements> {
        // SAFETY: a model of the library, a writable pointer
        MeshElements::from_table(&self.query(|a, p, out| unsafe { (a.hb_mesh_elements)(p, mesh.0, out) })?)
    }

    /// Points and unit normals (into the fluid) of a patch (its one-based element) at parameters
    /// uv in [-1, 1]².
    pub fn patch_points(&self, mesh: ObjectId, element: i32, uv: &[[f64; 2]]) -> Result<PatchPoints> {
        let flat = uv.as_flattened();
        // SAFETY: a model of the library, n parameter pairs, a writable pointer
        let t = self
            .query(|a, p, out| unsafe { (a.hb_mesh_patch_points)(p, mesh.0, element, flat.as_ptr(), uv.len(), out) })?;
        Ok(PatchPoints { points: t.rows::<3>("points")?, normals: t.rows::<3>("normals")? })
    }
}
