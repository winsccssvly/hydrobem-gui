//! The results of a solve: views of the quantities the DLL holds, their meta data and
//! diagnostics.
#![allow(unsafe_code)] // the calls of the DLL and the views of its elements

use std::sync::Arc;

use num_complex::{Complex32, Complex64};

use crate::codes::{ElementType, QuantityId};
use crate::error::{Diagnostic, Error, ErrorKind, Result};
use crate::ffi::{self, Api, HbColumn};
use crate::table::{Column, c, c_path, check, owned_text, table};

/// The meta data of a solve.
#[derive(Clone, Debug, Default)]
pub struct RunMeta {
    /// false for results not from a solve
    pub present: bool,
    pub hydrobem_version: String,
    pub commit: String,
    pub compiler: String,
    pub linear_algebra: String,
    pub build_type: String,
    pub time: String,
    pub route: String,
    pub api_version: i32,
    pub frequency_threads: i32,
    pub inner_threads: i32,
    pub engine_seconds: Vec<f64>,
}

/// The element type and dimensions of a quantity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuantityShape {
    pub element_type: ElementType,
    pub dims: Vec<usize>,
    /// the size of its data
    pub bytes: usize,
}

struct Handle {
    a: &'static Api,
    p: *mut ffi::Results,
}

// SAFETY: the results of the DLL are only read here (no `hb_results_put`) and freed once, by the
// last owner; the DLL guards its results in each call (HydroBEM's hydrobem-c holds them behind a
// mutex), so reads from several threads are safe
unsafe impl Send for Handle {}
// SAFETY: as Send; reads only
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: results of the library, freed once
        unsafe { (self.a.hb_results_free)(self.p) };
    }
}

/// The results of a solve, or read from a results file (cloned cheaply: one set of results).
#[derive(Clone)]
pub struct Results {
    h: Arc<Handle>,
    route: String,
    diagnostics: Vec<Diagnostic>,
    meta: RunMeta,
}

impl std::fmt::Debug for Results {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Results").field("route", &self.route).field("meta", &self.meta).finish_non_exhaustive()
    }
}

impl Results {
    /// Takes results the library handed out.
    pub(crate) fn take(a: &'static Api, p: *mut ffi::Results) -> Result<Results> {
        if p.is_null() {
            return Err(Error::new(ErrorKind::Internal, "hydrobem.dll handed out no results"));
        }
        let h = Arc::new(Handle { a, p });
        let mut route = std::ptr::null_mut();
        // SAFETY: results of the library, a writable pointer
        check(a, unsafe { (a.hb_results_route)(p, &mut route) })?;
        // SAFETY: a text of the library
        let route = unsafe { owned_text(a, route) };
        // SAFETY: as above
        let diagnostics = Diagnostic::from_table(&table(|a, out| unsafe { (a.hb_results_diagnostics)(p, out) })?)?;
        // SAFETY: as above
        let m = table(|a, out| unsafe { (a.hb_results_meta)(p, out) })?;
        let meta = RunMeta {
            present: m.bool("present")?,
            hydrobem_version: m.text("hydrobem_version")?,
            commit: m.text("commit")?,
            compiler: m.text("compiler")?,
            linear_algebra: m.text("linear_algebra")?,
            build_type: m.text("build_type")?,
            time: m.text("time")?,
            route: m.text("route")?,
            api_version: m.i32("api_version")?,
            frequency_threads: m.i32("frequency_threads")?,
            inner_threads: m.i32("inner_threads")?,
            engine_seconds: m.f64s("engine_seconds")?,
        };
        Ok(Results { h, route, diagnostics, meta })
    }

    /// The route of the solve ("given" for results made by a client).
    pub fn route(&self) -> &str {
        &self.route
    }

    /// The warnings of the model, then of the solve.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn meta(&self) -> &RunMeta {
        &self.meta
    }

    pub fn has(&self, q: QuantityId) -> bool {
        let Ok(name) = c(q.name()) else { return false };
        let mut has = 0;
        // SAFETY: results of the library, a NUL-terminated name, a writable flag
        let status = unsafe { (self.h.a.hb_results_has)(self.h.p, name.as_ptr(), &mut has) };
        status == 0 && has != 0
    }

    /// The quantities the results have, in the order of the quantity table.
    pub fn available(&self) -> Vec<QuantityId> {
        let p = self.h.p;
        // SAFETY: results of the library, a writable pointer
        table(|a, out| unsafe { (a.hb_results_available)(p, out) })
            .and_then(|t| t.texts("names"))
            .map(|names| names.iter().filter_map(|n| QuantityId::from_name(n)).collect())
            .unwrap_or_default()
    }

    /// The view of a quantity: valid while the results live.
    fn view(&self, q: QuantityId) -> Result<Column<'_>> {
        let name = c(q.name())?;
        let mut col = HbColumn::default();
        // SAFETY: results of the library, a NUL-terminated name, a writable column
        check(self.h.a, unsafe { (self.h.a.hb_results_quantity)(self.h.p, name.as_ptr(), &mut col) })?;
        // SAFETY: the view lives as long as the results (never changed here)
        Ok(unsafe { Column::new(col) })
    }

    /// The element type and dimensions of quantity q (None when the results do not have it).
    pub fn shape(&self, q: QuantityId) -> Option<QuantityShape> {
        // a quantity the results do not have is an error of the view
        let v = self.view(q).ok()?;
        let element_type = ElementType::from_dtype(v.dtype())?;
        let dims = v.shape().to_vec();
        let bytes = dims.iter().product::<usize>() * element_type.size();
        Some(QuantityShape { element_type, dims, bytes })
    }

    /// The elements of a float64 quantity.
    pub fn f64s(&self, q: QuantityId) -> Result<&[f64]> {
        self.view(q)?.elements()
    }

    /// The elements of a float32 quantity.
    pub fn f32s(&self, q: QuantityId) -> Result<&[f32]> {
        self.view(q)?.elements()
    }

    /// The elements of a complex64 quantity (complex float32).
    pub fn complex32s(&self, q: QuantityId) -> Result<&[Complex32]> {
        self.view(q)?.elements()
    }

    /// The elements of a complex128 quantity (complex float64).
    pub fn complex64s(&self, q: QuantityId) -> Result<&[Complex64]> {
        self.view(q)?.elements()
    }

    /// The elements of an int32 quantity.
    pub fn i32s(&self, q: QuantityId) -> Result<&[i32]> {
        self.view(q)?.elements()
    }

    /// The elements of an int64 quantity.
    pub fn i64s(&self, q: QuantityId) -> Result<&[i64]> {
        self.view(q)?.elements()
    }

    /// The elements of a numeric quantity of any element type as bytes, in the engine's layout
    /// (row-major, little-endian on Windows x64, a complex as its real then imaginary part); an
    /// error for a text quantity.
    pub fn bytes(&self, q: QuantityId) -> Result<&[u8]> {
        self.view(q)?.bytes()
    }

    /// The part of a complex64 quantity (complex float32) the view of `path` selects for an owner
    /// (`hb_results_select`, e.g. "tanks.pressure_radiation"): `body` the zero-based solved body,
    /// `owner` the zero-based tank (lid, pressure surface) of the body; its dimensions and values.
    /// None when the results have no part of it.
    pub fn select_complex32(
        &self,
        path: &str,
        body: usize,
        owner: usize,
    ) -> Result<Option<(Vec<usize>, Vec<Complex32>)>> {
        let path = c(path)?;
        let (p, body, owner) = (self.h.p, body as i64, owner as i64);
        // SAFETY: results of the library, a NUL-terminated path, a writable pointer
        let t = table(|a, out| unsafe { (a.hb_results_select)(p, path.as_ptr(), body, owner, out) })?;
        let Ok(values) = t.column("values") else { return Ok(None) };
        Ok(Some((values.shape().to_vec(), values.elements::<Complex32>()?.to_vec())))
    }

    /// A new model of the model as it was solved (`hb_results_solved_model`): the enabled objects
    /// of the inputs, whose bodies, lids and field points the quantities number.
    pub fn solved_model(&self) -> Result<crate::Model> {
        let p = self.h.p;
        // SAFETY: results of the library, a writable pointer
        crate::Model::make(|a, out| unsafe { (a.hb_results_solved_model)(p, out) })
    }

    /// Writes the results file (.hbr).
    pub fn save(&self, path: &std::path::Path, overwrite: bool) -> Result<()> {
        let path = c_path(path)?;
        // SAFETY: results of the library, a NUL-terminated path
        check(self.h.a, unsafe { (self.h.a.hb_results_save)(self.h.p, path.as_ptr(), i32::from(overwrite)) })
    }

    /// Writes the WAMIT-format numeric files (.1, .2, .3, ...) and the .out summary of `stem` (a
    /// path without extension): the first column the frequencies in rad/s when `omega`, else the
    /// periods; the headers dated now.
    pub fn write_wamit(&self, stem: &std::path::Path, overwrite: bool, omega: bool) -> Result<()> {
        let stem = c_path(stem)?;
        // SAFETY: results of the library, a NUL-terminated path, no date (null: now)
        check(self.h.a, unsafe {
            (self.h.a.hb_results_write_wamit)(
                self.h.p,
                stem.as_ptr(),
                i32::from(overwrite),
                i32::from(omega),
                std::ptr::null(),
            )
        })
    }
}
