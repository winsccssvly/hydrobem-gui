//! The engine DLL the application loads and where it was imported from.

use serde::Deserialize;

/// `engine/IMPORT.json`, written by `scripts/import-engine.ps1`.
const IMPORT_JSON: &str = include_str!("../../../engine/IMPORT.json");

#[derive(Clone, Debug, Deserialize)]
pub struct Import {
    pub abi_version: u32,
    pub source_commit: String,
    pub source_describe: String,
    pub source_dirty: bool,
    pub dll_built_utc: String,
    pub imported_utc: String,
}

/// The import record of the engine DLL this build was made with.
pub fn import() -> Import {
    serde_json::from_str(IMPORT_JSON).expect("engine/IMPORT.json is written by the import script")
}

/// "0.2.0 (ccd445b0f4a1)": the version and commit the loaded DLL reports of its own build.
pub fn engine_label() -> String {
    let b = hydrobem_engine::build_info();
    if b.version.is_empty() {
        return "not loaded".into();
    }
    format!("{} ({})", b.version, b.commit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_import_record_matches_the_loaded_engine() {
        let i = import();
        hydrobem_engine::load().expect("engine/hydrobem.dll beside the test programs");
        assert_eq!(i.abi_version, hydrobem_engine::ABI_VERSION);
        assert!(hydrobem_engine::build_info().avx2);
        assert_eq!(i.source_commit.len(), 40);
        assert!(!i.imported_utc.is_empty() && !i.dll_built_utc.is_empty());
    }
}
