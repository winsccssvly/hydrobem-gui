//! Puts the imported `engine/hydrobem.dll` beside the programs of the build (`target/<profile>/`),
//! where the binding finds it at run time; the test programs in `deps/` find it one folder up.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let dll = manifest.join("../../engine/hydrobem.dll");
    println!("cargo:rerun-if-changed={}", dll.display());
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo"));
    // OUT_DIR is target/<profile>/build/<package>/out
    let Some(profile) = out.ancestors().nth(3) else { return };
    if !dll.is_file() {
        println!("cargo:warning=engine/hydrobem.dll is missing: run scripts/import-engine.ps1");
        return;
    }
    let target = profile.join("hydrobem.dll");
    let same = std::fs::metadata(&dll).ok().map(|m| (m.len(), m.modified().ok()))
        == std::fs::metadata(&target).ok().map(|m| (m.len(), m.modified().ok()));
    if !same && let Err(e) = std::fs::copy(&dll, &target) {
        // a running HydroBEM.exe holds the old copy
        println!("cargo:warning=hydrobem.dll not copied to {}: {e}", target.display());
    }
}
