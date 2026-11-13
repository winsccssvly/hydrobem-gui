//! The C ABI of `hydrobem.dll` (`engine/hydrobem.h`, ABI `ABI_VERSION`): its structs, and its
//! functions as one table of pointers, loaded once from the DLL at run time.
#![allow(unsafe_code)] // the calls of the engine's C functions and of the Windows loader

use std::ffi::{c_char, c_void};
use std::path::PathBuf;
use std::sync::OnceLock;

/// The version of the ABI this binding is written for (`HB_ABI_VERSION` of `engine/hydrobem.h`).
pub const ABI_VERSION: u32 = 8;

pub type Status = i32;
pub type Table = c_void;
pub type Model = c_void;
pub type Results = c_void;

pub const HB_F64: i32 = 1;
pub const HB_F32: i32 = 2;
pub const HB_I32: i32 = 3;
pub const HB_I64: i32 = 4;
pub const HB_U64: i32 = 5;
pub const HB_C64: i32 = 6;
pub const HB_C128: i32 = 7;
pub const HB_TEXT: i32 = 8;
pub const HB_NO_OBJECT: u64 = u64::MAX;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HbLocation {
    pub body: i64,
    pub mesh: i64,
    pub element: i64,
    pub field_point: i64,
    pub lid: i64,
    pub frequency: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HbColumn {
    pub name: *const c_char,
    pub dtype: i32,
    pub ndim: usize,
    pub shape: *const usize,
    pub count: usize,
    pub data: *const c_void,
}

impl Default for HbColumn {
    fn default() -> HbColumn {
        HbColumn {
            name: std::ptr::null(),
            dtype: 0,
            ndim: 0,
            shape: std::ptr::null(),
            count: 0,
            data: std::ptr::null(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HbProgress {
    pub stage: i32,
    pub frequency: i64,
    pub stage_done: u64,
    pub stage_total: u64,
    pub frequencies_done: u64,
    pub frequencies_total: u64,
    pub seconds: f64,
    pub worker: i32,
    pub phase: i32,
    pub step: i32,
    pub item: i64,
}

pub type HbProgressFn = Option<unsafe extern "C" fn(progress: *const HbProgress, user: *mut c_void) -> i32>;

#[repr(C)]
pub struct HbMeshOptions {
    pub higher_order: i32,
    pub zero_trailer: i32,
    pub spl_path: *const c_char,
    pub discretization: *const i32,
    pub discretization_rows: usize,
    pub panel_size: f64,
    pub spline_order: i32,
    pub outer_quadrature: i32,
    pub inner_quadrature: i32,
}

#[repr(C)]
pub struct HbAnalyticShape {
    pub shape: *const c_char,
    pub dimension_names: *const *const c_char,
    pub dimension_values: *const f64,
    pub dimensions: usize,
    pub discs: *const f64,
    pub disc_count: usize,
    pub parts: *const *const c_char,
    pub part_count: usize,
    pub mapping: *const c_char,
    pub discretization: *const i32,
    pub discretization_rows: usize,
    pub reference_length: f64,
    pub symmetry: *const c_char,
    pub label: *const c_char,
}

type Farproc = unsafe extern "system" fn() -> isize;

macro_rules! api {
    ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        /// The functions of the DLL.
        pub struct Api {
            $(pub $name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
        }

        impl Api {
            /// # Safety
            /// `lib` is a loaded hydrobem.dll that is never unloaded.
            unsafe fn bind(lib: windows_sys::Win32::Foundation::HMODULE) -> Result<Api, String> {
                Ok(Api {
                    $($name: {
                        // SAFETY: a NUL-terminated name of a module that stays loaded
                        let f = unsafe {
                            windows_sys::Win32::System::LibraryLoader::GetProcAddress(
                                lib,
                                concat!(stringify!($name), "\0").as_ptr(),
                            )
                        };
                        let Some(f) = f else {
                            return Err(format!("hydrobem.dll has no function {}", stringify!($name)));
                        };
                        // SAFETY: the function of that name has this signature in hydrobem.h
                        unsafe { std::mem::transmute::<Farproc, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(f) }
                    },)*
                })
            }
        }
    };
}

api! {
    hb_abi_version: fn() -> u32;
    hb_string_free: fn(*mut c_char);
    hb_last_error_message: fn() -> *const c_char;
    hb_last_error_location: fn(*mut HbLocation) -> Status;
    hb_build_info: fn(*mut *mut Table) -> Status;
    hb_schema_items: fn(*mut *mut Table) -> Status;
    hb_schema_lists: fn(*mut *mut Table) -> Status;
    hb_quantities: fn(*mut *mut Table) -> Status;
    hb_panel_roles: fn(*mut *mut Table) -> Status;
    hb_wavenumber: fn(f64, f64, f64, *mut f64) -> Status;
    hb_table_find: fn(*const Table, *const c_char, *mut HbColumn) -> Status;
    hb_table_free: fn(*mut Table);
    hb_model_new: fn(*mut *mut Model) -> Status;
    hb_model_load: fn(*const c_char, *mut *mut Model) -> Status;
    hb_model_load_results: fn(*const c_char, *mut *mut Model) -> Status;
    hb_model_save: fn(*mut Model, *const c_char, i32) -> Status;
    hb_model_save_results: fn(*mut Model, *const c_char, i32) -> Status;
    hb_model_copy: fn(*mut Model, *mut *mut Model) -> Status;
    hb_model_free: fn(*mut Model) -> Status;
    hb_model_state: fn(*mut Model, *mut i32) -> Status;
    hb_model_cancel: fn(*mut Model) -> Status;
    hb_model_progress: fn(*mut Model, *mut *mut Table) -> Status;
    hb_model_reset: fn(*mut Model) -> Status;
    hb_model_clear: fn(*mut Model) -> Status;
    hb_model_check: fn(*mut Model, *mut *mut Table) -> Status;
    hb_model_diagnostics: fn(*mut Model, *mut *mut Table) -> Status;
    hb_model_solve: fn(*mut Model, HbProgressFn, *mut c_void, *mut *mut Results) -> Status;
    hb_model_results: fn(*mut Model, *mut *mut Results) -> Status;
    hb_object: fn(*mut Model, *const c_char, *mut u64) -> Status;
    hb_object_info: fn(*mut Model, u64, *mut *mut Table) -> Status;
    hb_children: fn(*mut Model, u64, *const c_char, *mut *mut Table) -> Status;
    hb_add: fn(*mut Model, u64, *const c_char, *const c_char, *const c_char, i64, *mut u64) -> Status;
    hb_remove: fn(*mut Model, u64) -> Status;
    hb_rename: fn(*mut Model, u64, *const c_char) -> Status;
    hb_move: fn(*mut Model, u64, i64) -> Status;
    hb_clear: fn(*mut Model, u64, *const c_char) -> Status;
    hb_item_info: fn(*mut Model, u64, *const c_char, *mut *mut Table) -> Status;
    hb_get: fn(*mut Model, u64, *const c_char, *mut *mut Table) -> Status;
    hb_set_bool: fn(*mut Model, u64, *const c_char, i32) -> Status;
    hb_set_int: fn(*mut Model, u64, *const c_char, i64) -> Status;
    hb_set_real: fn(*mut Model, u64, *const c_char, f64) -> Status;
    hb_set_text: fn(*mut Model, u64, *const c_char, *const c_char) -> Status;
    hb_set_texts: fn(*mut Model, u64, *const c_char, *const *const c_char, usize) -> Status;
    hb_set_reals: fn(*mut Model, u64, *const c_char, *const f64, *const usize, usize) -> Status;
    hb_set_ints: fn(*mut Model, u64, *const c_char, *const i64, *const usize, usize) -> Status;
    hb_reset: fn(*mut Model, u64, *const c_char) -> Status;
    hb_model_route: fn(*mut Model, *mut *mut c_char) -> Status;
    hb_model_load_mesh: fn(*mut Model, *const c_char, *const c_char, *const HbMeshOptions, *mut u64) -> Status;
    hb_model_mesh_from_hbg: fn(*mut Model, *const c_char, *const c_char, *mut u64) -> Status;
    hb_mesh_hbg: fn(*mut Model, u64, *mut *mut c_char) -> Status;
    hb_model_analytic_mesh: fn(*mut Model, *const c_char, *const HbAnalyticShape, *mut u64) -> Status;
    hb_mesh_write: fn(*mut Model, u64, *const c_char, *const c_char, i32) -> Status;
    hb_model_counts: fn(*mut Model, *mut *mut Table) -> Status;
    hb_model_motion_equation: fn(*mut Model, *mut *mut Table) -> Status;
    hb_model_panels: fn(*mut Model, *const c_char, i32, *mut *mut Table) -> Status;
    hb_model_tessellation: fn(*mut Model, *const c_char, i32, i32, i32, *mut *mut Table) -> Status;
    hb_model_mesh_quality: fn(*mut Model, *mut *mut Table) -> Status;
    hb_model_mode_displacements: fn(*mut Model, *const f64, *const i32, *const i32, usize, *mut *mut Table) -> Status;
    hb_model_memory_estimate: fn(*mut Model, *mut *mut Table) -> Status;
    hb_model_classify_points: fn(*mut Model, *const f64, usize, u64, *mut *mut Table) -> Status;
    hb_body_hydrostatics: fn(*mut Model, u64, *mut *mut Table) -> Status;
    hb_body_panels: fn(*mut Model, u64, *const c_char, i32, *mut *mut Table) -> Status;
    hb_body_mesh_quality: fn(*mut Model, u64, *mut *mut Table) -> Status;
    hb_body_tessellation: fn(*mut Model, u64, *const c_char, i32, i32, i32, *mut *mut Table) -> Status;
    hb_mesh_geometry: fn(*mut Model, u64, *mut *mut Table) -> Status;
    hb_mesh_elements: fn(*mut Model, u64, *mut *mut Table) -> Status;
    hb_mesh_tessellation: fn(*mut Model, u64, i32, i32, i32, *mut *mut Table) -> Status;
    hb_mesh_geometry_quality: fn(*mut Model, u64, *mut *mut Table) -> Status;
    hb_mesh_patch_points: fn(*mut Model, u64, i32, *const f64, usize, *mut *mut Table) -> Status;
    hb_results_free: fn(*mut Results);
    hb_results_has: fn(*mut Results, *const c_char, *mut i32) -> Status;
    hb_results_quantity: fn(*mut Results, *const c_char, *mut HbColumn) -> Status;
    hb_results_select: fn(*mut Results, *const c_char, i64, i64, *mut *mut Table) -> Status;
    hb_results_available: fn(*mut Results, *mut *mut Table) -> Status;
    hb_results_route: fn(*mut Results, *mut *mut c_char) -> Status;
    hb_results_diagnostics: fn(*mut Results, *mut *mut Table) -> Status;
    hb_results_meta: fn(*mut Results, *mut *mut Table) -> Status;
    hb_results_solved_model: fn(*mut Results, *mut *mut Model) -> Status;
    hb_results_save: fn(*mut Results, *const c_char, i32) -> Status;
    hb_results_write_wamit: fn(*mut Results, *const c_char, i32, i32, *const c_char) -> Status;
}

/// Where the DLL is looked for: `HYDROBEM_DLL` (a path), else beside the program; in a debug
/// build also one folder up (cargo's test programs in `target/<profile>/deps`), never in a release.
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = std::env::var_os("HYDROBEM_DLL") {
        out.push(PathBuf::from(p));
    }
    if let Ok(exe) = std::env::current_exe() {
        let folders = if cfg!(debug_assertions) { 2 } else { 1 };
        for dir in exe.ancestors().skip(1).take(folders) {
            out.push(dir.join("hydrobem.dll"));
        }
    }
    out
}

fn load() -> Result<Api, String> {
    let Some(path) = candidates().into_iter().find(|p| p.is_file()) else {
        return Err("hydrobem.dll was not found beside HydroBEM.exe.".into());
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide_nul();
    // SAFETY: a NUL-terminated wide path; the library is never freed
    let lib = unsafe { windows_sys::Win32::System::LibraryLoader::LoadLibraryW(wide.as_ptr()) };
    if lib.is_null() {
        return Err(format!("{} could not be loaded ({}).", path.display(), std::io::Error::last_os_error()));
    }
    // SAFETY: the loaded hydrobem.dll, never unloaded
    let api = unsafe { Api::bind(lib) }.map_err(|e| format!("{}: {e}", path.display()))?;
    // SAFETY: a function without arguments
    let abi = unsafe { (api.hb_abi_version)() };
    if abi != ABI_VERSION {
        return Err(format!(
            "{} has the C ABI {abi} of the engine; this HydroBEM needs ABI {ABI_VERSION}.",
            path.display()
        ));
    }
    Ok(api)
}

trait WideNul {
    fn encode_wide_nul(&self) -> Vec<u16>;
}

impl WideNul for std::ffi::OsStr {
    fn encode_wide_nul(&self) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        self.encode_wide().chain(std::iter::once(0)).collect()
    }
}

static API: OnceLock<Result<Api, String>> = OnceLock::new();

/// The functions of the DLL, loaded on first use; the message says why they are not there.
pub fn api() -> Result<&'static Api, &'static str> {
    API.get_or_init(load).as_ref().map_err(String::as_str)
}
