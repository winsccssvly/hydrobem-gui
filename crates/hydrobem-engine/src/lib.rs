//! The HydroBEM engine for the GUI: `hydrobem.dll`, the engine's C ABI (`engine/hydrobem.h`),
//! loaded at run time and wrapped in safe types. The names follow the engine's Rust API
//! (`Model`, `Value`, `Results`, ...); every answer comes from the tables of the DLL.

mod codes;
mod error;
mod ffi;
mod model;
mod results;
mod table;
mod types;

pub use codes::{
    ElementType, ModelState, PanelRole, Phase, PointStatus, QualityFlags, QuantityId, SecondOrderStep, Severity, Stage,
    Warning,
};
pub use error::{Diagnostic, Error, ErrorKind, Location, Result};
pub use ffi::ABI_VERSION;
pub use model::{CancelToken, Model, wavenumber};
pub use num_complex::{Complex, Complex32, Complex64};
pub use results::{QuantityShape, Results, RunMeta};
pub use types::{
    AnalyticShape, BuildInfo, Counts, FrequencyProgress, Hydrostatics, ItemDef, ItemInfo, LimitProgress, ListDef,
    MemoryEstimate, MeshElements, MeshFile, MeshGeometry, MeshQuality, MotionEquation, ObjectId, ObjectInfo, PanelKey,
    Panels, PatchPoints, PointClass, Progress, ProgressAt, RunState, SecondOrderProgress, SolveProgress, Tessellation,
    TypeDef, Value, WorkerProgress, WorkerState, WorkerTask, build_info, schema,
};

/// Loads hydrobem.dll (once) and checks it: the message says why it cannot be used (not found,
/// not loadable, another ABI, other result quantities).
pub fn load() -> std::result::Result<(), String> {
    ffi::api().map_err(str::to_string)?;
    #[allow(unsafe_code)]
    // SAFETY: a writable pointer
    let names = table::table(|a, out| unsafe { (a.hb_quantities)(out) })
        .and_then(|t| t.texts("name"))
        .map_err(|e| e.message().to_string())?;
    if !names.iter().map(String::as_str).eq(QuantityId::ALL.iter().map(|q| q.name())) {
        return Err("hydrobem.dll has other result quantities than this HydroBEM knows.".into());
    }
    #[allow(unsafe_code)]
    // SAFETY: a writable pointer
    let roles = table::table(|a, out| unsafe { (a.hb_panel_roles)(out) })
        .and_then(|t| t.texts("names"))
        .map_err(|e| e.message().to_string())?;
    let known = (0..).map_while(PanelRole::from_value).map(PanelRole::name);
    if !roles.iter().map(String::as_str).eq(known) {
        return Err("hydrobem.dll has other element roles than this HydroBEM knows.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_binding_follows_the_imported_header_and_dll() {
        let header = include_str!("../../../engine/hydrobem.h");
        assert!(header.contains(&format!("#define HB_ABI_VERSION {ABI_VERSION}\n")), "engine/hydrobem.h");
        load().expect("engine/hydrobem.dll beside the test programs");
        assert!(build_info().avx2 && !build_info().version.is_empty());
    }

    #[test]
    fn objects_have_their_paths() {
        let mut m = Model::new();
        let body = m.add(m.root(), "bodies", None, "b", None).unwrap();
        assert_eq!(m.object_info(body).unwrap().path, "bodies/b");
        assert_eq!(m.results().unwrap_err().kind(), ErrorKind::State);
    }

    #[test]
    fn items_and_values_go_both_ways() {
        let mut m = Model::new();
        let env = m.object("environment").unwrap();
        m.set(env, "depth", 50.0).unwrap();
        assert_eq!(m.get(env, "depth").unwrap(), Some(Value::Real(50.0)));
        let e = m.set(env, "depth", -1.0).unwrap_err();
        assert_eq!(e.kind(), ErrorKind::Value);
        assert!(!e.message().is_empty());
        assert!(schema().iter().any(|t| t.name == "Body" && t.lists.iter().any(|l| l.name == "tanks")));
        assert!((wavenumber(1.0, f64::INFINITY, 9.80665).unwrap() - 1.0 / 9.80665).abs() < 1e-12);
    }
}
