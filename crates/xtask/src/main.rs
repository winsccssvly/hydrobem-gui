//! Development tasks:
//! - `cargo xtask testdata` writes the models the tests read (`crates/*/tests/data`) from those
//!   `scripts/make_testdata.py` builds of HydroBEM's example cases;
//! - `cargo xtask wamit <folder>` writes HydroBEM models (`.hbm`) of WAMIT's v7.5 demonstration
//!   runs into a folder that holds WAMIT's demonstration meshes (not in this repository: their terms
//!   do not allow redistribution);
//! - `cargo xtask licenses` writes THIRD_PARTY_NOTICES.md from the crates HydroBEM.exe links;
//! - `cargo xtask dist` stages `dist/` from a release build (HydroBEM.exe, hydrobem.dll, the
//!   examples (HydroBEM's Python scripts), the licence texts of those crates), which `scripts/build_wheel.py` packs into the
//!   wheel.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gui_core::engine_model::move_inertia;
use gui_core::field_layout::GridLayout;
use gui_core::files::{load_project, save_project};
use gui_core::mesh::Mesh;
use gui_core::project::{
    Body, DepthType, Domain, FieldPoints, Formulation, MassCalculation, PanelSurface, Project, SurfaceSource,
    WaveFrequencies, WaveHeadings, Waves,
};
use hydrobem_engine::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repository root")
}

fn main() {
    let task = std::env::args().nth(1).unwrap_or_default();
    let outcome = match task.as_str() {
        "testdata" => testdata(),
        "wamit" => match std::env::args().nth(2) {
            Some(folder) => {
                WAMIT_FOLDER.get_or_init(|| PathBuf::from(folder));
                wamit()
            }
            None => Err("usage: cargo xtask wamit <folder of WAMIT's demonstration meshes>".into()),
        },
        "licenses" => licenses().map(|_| ()),
        "dist" => dist(),
        _ => Err(format!("unknown task {task:?}; usage: cargo xtask <testdata|wamit FOLDER|licenses|dist>")),
    };
    if let Err(e) = outcome {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

// ---- Test models -----------------------------------------------------------------------------

/// The test data of gui-core (every model, for its file tests) and of the GUI's tests.
const GUI_CORE: &str = "crates/gui-core/tests/data";
const GUI: &str = "crates/hydrobem-gui/tests/data";

/// The models `scripts/make_testdata.py` builds from HydroBEM's example cases (in target/testdata),
/// with the crates whose tests read each.
const MODELS: &[(&str, &[&str])] = &[
    ("cylinder.hbm", &[GUI_CORE, GUI]),
    ("cylinder_wall.hbm", &[GUI_CORE, GUI]),
    ("cylinder_channel.hbm", &[GUI_CORE, GUI]),
    ("two_truncated_cylinders.hbm", &[GUI_CORE, GUI]),
    ("truncated_cylinder_patches.hbm", &[GUI_CORE, GUI]),
    ("floating_owc_chamber.hbm", &[GUI_CORE, GUI]),
    ("floating_owc_chamber_lid.hbm", &[GUI_CORE, GUI]),
    ("cylinder_with_heave_plate.hbm", &[GUI_CORE, GUI]),
    ("three_truncated_cylinders.hbm", &[GUI_CORE, GUI]),
    ("three_truncated_cylinders_patches.hbm", &[GUI_CORE, GUI]),
    ("three_columns.hbm", &[GUI_CORE, GUI]),
    ("dock.hbm", &[GUI_CORE]),
    ("fish_tank_cylinders.hbm", &[GUI_CORE]),
    ("fish_tank_covered.hbm", &[GUI_CORE, GUI]),
    ("trimmed_moonpool_cover.hbm", &[GUI_CORE, GUI]),
];

/// The mesh files `scripts/make_testdata.py` writes, with the crates whose tests read them.
const MESH_FILES: &[(&str, &[&str])] = &[
    ("cylinder.gdf", &[GUI_CORE, GUI]),
    ("two_truncated_cylinders.gdf", &[GUI]),
    ("truncated_cylinder_patches.gdf", &[GUI_CORE, GUI]),
    ("truncated_cylinder_patches.spl", &[GUI_CORE, GUI]),
];

/// Opens each model of `scripts/make_testdata.py` in the GUI and saves it with the GUI's state into
/// the tests' data of the crates that read it, printing what the GUI adapts (the tests open the
/// saved models without warnings); copies the mesh files; writes the model of the patch mesh read
/// from truncated_cylinder_patches.gdf and .spl.
fn testdata() -> Result<(), String> {
    let source = root().join("target/testdata");
    if !source.is_dir() {
        return Err("no target/testdata: run `python scripts/make_testdata.py` first".into());
    }
    for (name, dirs) in MODELS {
        let (mut project, warnings) = load_project(&source.join(name)).map_err(|e| format!("{name}: {e}"))?;
        for w in &warnings {
            println!("{name}: the GUI adapts: {w}");
        }
        gui_settings(name, &mut project);
        for dir in *dirs {
            let file = root().join(dir).join(name);
            save_project(&project, &file).map_err(|e| format!("{name}: {e}"))?;
            println!("wrote {dir}/{name}");
        }
    }
    for (name, dirs) in MESH_FILES {
        for dir in *dirs {
            copy(&source.join(name), &root().join(dir).join(name))?;
            println!("wrote {dir}/{name}");
        }
    }
    truncated_cylinder_gdf()
}

/// What only the GUI holds (its state): the body-fitted rings of the three cylinders, about the
/// convex outline of their sections from 2 to 60 m outside it and ten rings inside it for the
/// water between them.
fn gui_settings(name: &str, p: &mut Project) {
    if name == "three_truncated_cylinders.hbm" || name == "three_truncated_cylinders_patches.hbm" {
        p.field_points = rings();
        p.first_order.field_pressure = true;
        p.update_field_profile();
    }
}

fn rings() -> FieldPoints {
    FieldPoints {
        enabled: true,
        layout: GridLayout::BodyFitted,
        convex: true,
        inside_rings: 10,
        radial_start: 2.0,
        radial_end: 60.0,
        radial_count: 20,
        angle_count: 72,
        ..FieldPoints::default()
    }
}

/// The truncated cylinder's analytic patches read from truncated_cylinder_patches.gdf and .spl (a
/// patch mesh read from a file, its discretization from the project; Panel Size reads the file
/// again): the mesh named by its file name alone (no machine path in the model), the files beside
/// the model, as saved by a user who keeps them together.
fn truncated_cylinder_gdf() -> Result<(), String> {
    let dir = root().join(GUI_CORE);
    let loaded = Mesh::load(&dir.join("truncated_cylinder_patches.gdf"))?;
    let mut mesh = Mesh { source_file: "truncated_cylinder_patches.gdf".into(), ..(*loaded).clone() };
    if let Some(patch) = mesh.patch.as_mut() {
        for item in ["source_file", "source_spl"] {
            let file = patch
                .object
                .get(item)
                .and_then(|v| v.as_text())
                .map(|t| Path::new(t).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default());
            if let Some(file) = file {
                patch.object.set(item, Value::Text(file));
            }
        }
    }
    let mesh = Arc::new(mesh);
    let b = Body { name: "cylinder".into(), mesh: Some(mesh.clone()), ..Body::default() };
    let mut frequencies = WaveFrequencies { start: 0.5, end: 2.0, count: 4, ..WaveFrequencies::default() };
    frequencies.rebuild();
    let mut p = Project {
        bodies: vec![b],
        waves: Waves { frequencies, headings: WaveHeadings { start: 0.0, end: 0.0, count: 1 } },
        ..Project::default()
    };
    p.add_mesh("truncated_cylinder_patches", mesh);
    p.renumber();
    p.update_mesh_properties();
    for d in [GUI_CORE, GUI] {
        let name = "truncated_cylinder_gdf.hbm";
        save_project(&p, &root().join(d).join(name)).map_err(|e| format!("{name}: {e}"))?;
        println!("wrote {d}/{name}");
    }
    Ok(())
}

// ---- Internal WAMIT demonstration models -------------------------------------------------------

/// A body with the displaced mass, G and the inertia about R.
fn body(name: &str, mesh: &Arc<Mesh>, g: [f64; 3], inertia: [f64; 3]) -> Body {
    Body {
        name: name.into(),
        mesh: Some(mesh.clone()),
        mass_calculation: MassCalculation::FromMesh,
        center_of_gravity: g,
        inertia: [inertia[0], 0.0, 0.0, inertia[1], 0.0, inertia[2]],
        ..Body::default()
    }
}

fn project(bodies: Vec<Body>, frequencies: WaveFrequencies, headings: WaveHeadings, density: f64) -> Project {
    let mut p = Project { bodies, waves: Waves { frequencies, headings }, ..Project::default() };
    p.environment.density = density;
    p.first_order.motion = true;
    p
}

/// Puts every mesh the project uses into its library, named after its file.
fn library(p: &mut Project) {
    let mut meshes: Vec<Arc<Mesh>> = Vec::new();
    for b in &p.bodies {
        meshes.extend(b.mesh.iter().cloned());
        meshes.extend(b.dipoles.iter().chain(&b.pressure_surfaces).filter_map(|s| s.mesh.clone()));
    }
    meshes.extend(p.lids.iter().filter_map(|l| l.mesh.clone()));
    for mesh in meshes {
        let stem =
            Path::new(&mesh.source_file).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        p.add_mesh(&stem, mesh);
    }
}

/// The folder of WAMIT's demonstration meshes, where `cargo xtask wamit` writes its models.
static WAMIT_FOLDER: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

fn wamit_folder() -> &'static Path {
    WAMIT_FOLDER.get().expect("the folder of cargo xtask wamit")
}

fn wamit_mesh(name: &str) -> Result<Arc<Mesh>, String> {
    let loaded = Mesh::load(&wamit_folder().join(name))?;
    Ok(Arc::new(Mesh { source_file: name.into(), ..(*loaded).clone() }))
}

/// Frequencies as listed (uneven lists are edited in the table).
fn listed(omegas: &[f64]) -> WaveFrequencies {
    let inputs = omegas.to_vec();
    let edit_in_table = !gui_core::engine_model::evenly_spaced(&inputs);
    WaveFrequencies {
        start: inputs[0],
        end: inputs[inputs.len() - 1],
        count: inputs.len() as i64,
        edit_in_table,
        inputs,
        ..WaveFrequencies::default()
    }
}

fn heading(h: f64) -> WaveHeadings {
    WaveHeadings { start: h, end: h, count: 1 }
}

fn modes(names: &[&str]) -> [bool; 6] {
    std::array::from_fn(|i| names.contains(&gui_core::project::MODE_NAMES[i]))
}

fn wamit_save(p: &mut Project, name: &str) -> Result<(), String> {
    library(p);
    p.renumber();
    p.update_mesh_properties();
    let file = wamit_folder().join(name);
    save_project(p, &file).map_err(|e| format!("{name}: {e}"))?;
    println!("wrote {}", file.display());
    Ok(())
}

/// The problems with which HydroBEM solves these test runs, adapted to what the GUI represents:
/// radii of gyration as inertia components I = m r^2 with the displaced mass, the exciting force of
/// the motions by pressure integration (the demonstrations use Haskind).
fn wamit() -> Result<(), String> {
    let total = |p: &mut Project| p.first_order.diffraction_formulation = Formulation::Total;
    let fixed = |b: Body| Body { mass_calculation: MassCalculation::FromMesh, ..b };

    // Test 1: a floating cylinder, deep water, motions (periods 8.971402 s and 2.006403 s).
    let b = body("body1", &wamit_mesh("test01.gdf")?, [0.0; 3], [1568.27, 1568.27, 1568.27]);
    let mut p = project(vec![b], listed(&[0.7003571245, 3.131566942]), heading(0.0), 1000.0);
    total(&mut p);
    wamit_save(&mut p, "test01.hbm")?;

    // Test 2: a cylinder with a moonpool (pumping resonance near KL = 0.865); I about G.
    let mut b = body("moonpool", &wamit_mesh("test02.gdf")?, [0.0; 3], [147.017, 147.017, 147.017]);
    b.modes = modes(&["surge", "heave", "pitch"]);
    let omegas = [
        1.400474919,
        1.715224475,
        1.980570625,
        2.214345276,
        2.425693715,
        2.620048664,
        2.800949839,
        2.970855937,
        3.131557121,
        3.284404817,
        3.43044895,
    ];
    let mut p = project(vec![b], listed(&omegas), heading(180.0), 1000.0);
    total(&mut p);
    p.update_mesh_properties();
    let g = p.bodies[0].center_of_gravity;
    move_inertia(&mut p.bodies[0], g);
    wamit_save(&mut p, "test02.hbm")?;

    // Test 3: two barges alongside, coefficients and wave loads (periods 4.48 ... 5.18 s).
    let b1 =
        Body { position: [0.0, -11.0, 0.0], ..fixed(body("body1", &wamit_mesh("test03b1.gdf")?, [0.0; 3], [0.0; 3])) };
    let b2 =
        Body { position: [0.0, 6.0, 0.0], ..fixed(body("body2", &wamit_mesh("test03b2.gdf")?, [0.0; 3], [0.0; 3])) };
    let omegas =
        [1.40249672, 1.371874521, 1.342560963, 1.314473914, 1.287537973, 1.261683797, 1.236847501, 1.212970137];
    let mut p = project(vec![b1, b2], listed(&omegas), heading(270.0), 1000.0);
    p.first_order.motion = false;
    total(&mut p);
    wamit_save(&mut p, "test03.hbm")?;

    // Test 4 and 4a: a barge beside the wall y = 0, and in a channel 80 m wide, 20 m deep.
    let barge = wamit_mesh("test04.gdf")?;
    let test04 = |y: f64| {
        let mut b = body("body1", &barge, [0.0, 0.0, 3.0], [6.4e9, 4e8, 6.4e9]);
        b.position = [0.0, y, 0.0];
        b.modes = modes(&["surge", "heave", "pitch"]);
        b
    };
    let periods = [6.0, 7.0, 8.0].map(|t: f64| std::f64::consts::TAU / t); // periods 6, 7, 8 s
    let mut p = project(vec![test04(12.0)], listed(&periods), heading(0.0), 1000.0);
    total(&mut p);
    p.environment.domain = Domain::YWall;
    wamit_save(&mut p, "test04.hbm")?;
    let mut p = project(vec![test04(10.0)], listed(&periods), heading(0.0), 1000.0);
    total(&mut p);
    p.environment.depth_type = DepthType::Finite;
    p.environment.depth = 20.0;
    p.environment.domain = Domain::Channel;
    p.environment.channel_width = 80.0;
    wamit_save(&mut p, "test04a.hbm")?;

    // Test 5: a cylinder and a spheroid, interaction (periods 1.5 and 2 s).
    let c =
        Body { position: [1.25, 0.0, 0.0], ..fixed(body("cylinder", &wamit_mesh("test05c.gdf")?, [0.0; 3], [0.0; 3])) };
    let s =
        Body { position: [-0.5, 0.0, 0.0], ..fixed(body("spheroid", &wamit_mesh("test05s.gdf")?, [0.0; 3], [0.0; 3])) };
    let mut p = project(vec![c, s], listed(&[1.5, 2.0].map(|t: f64| std::f64::consts::TAU / t)), heading(0.0), 1000.0);
    p.first_order.motion = false;
    total(&mut p);
    wamit_save(&mut p, "test05.hbm")?;

    // Test 6: the ISSC tension-leg platform (coarse), 450 m deep (periods 5, 10, 15 s).
    let mut t = fixed(body("TLP", &wamit_mesh("test06.gdf")?, [0.0; 3], [0.0; 3]));
    t.modes = modes(&["surge", "heave", "pitch"]);
    let mut p = project(vec![t], listed(&[1.256637061, 0.6283185307, 0.4188790205]), heading(0.0), 1000.0);
    p.first_order.motion = false;
    p.environment.depth_type = DepthType::Finite;
    p.environment.depth = 450.0;
    wamit_save(&mut p, "test06.hbm")?;

    // Test 9: a spar with three thin strakes (dipole panels 673-960), KL = 0.1, 0.5, 1 (L = 18 m).
    let mut s = body("spar", &wamit_mesh("test09.gdf")?, [0.0; 3], [1.97829e8, 1.97829e8, 1.97829e8]);
    s.dipoles = vec![PanelSurface {
        source: SurfaceSource::PanelRange,
        first_panel: 673,
        last_panel: 960,
        ..PanelSurface::default()
    }];
    let mut p = project(
        vec![s],
        listed(&[0.2334124866, 0.5219261868, 0.7381150919]),
        WaveHeadings { start: 0.0, end: 45.0, count: 2 },
        1000.0,
    );
    wamit_save(&mut p, "test09.hbm")?;
    Ok(())
}

// ---- Licences ---------------------------------------------------------------------------------

/// A crate HydroBEM.exe links: its name, version, licence, authors, source and folder.
struct Linked {
    name: String,
    version: String,
    license: String,
    authors: Vec<String>,
    repository: String,
    dir: PathBuf,
}

/// The crates of the normal (run-time) dependency graph of HydroBEM.exe on Windows x64, from
/// `cargo metadata`; the workspace's own crates are left out.
fn linked_crates() -> Result<Vec<Linked>, String> {
    let output = std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--locked", "--filter-platform", "x86_64-pc-windows-msvc"])
        .current_dir(root())
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let meta: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let packages = meta["packages"].as_array().ok_or("no packages")?;
    let nodes = meta["resolve"]["nodes"].as_array().ok_or("no resolve graph")?;
    let start = packages
        .iter()
        .find(|p| p["name"] == "hydrobem-gui")
        .and_then(|p| p["id"].as_str())
        .ok_or("no hydrobem-gui package")?
        .to_string();
    let mut seen = std::collections::BTreeSet::new();
    let mut queue = vec![start];
    while let Some(id) = queue.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(node) = nodes.iter().find(|n| n["id"] == id.as_str()) else { continue };
        for dep in node["deps"].as_array().into_iter().flatten() {
            let normal = dep["dep_kinds"].as_array().into_iter().flatten().any(|k| k["kind"].is_null());
            if normal && let Some(pkg) = dep["pkg"].as_str() {
                queue.push(pkg.to_string());
            }
        }
    }
    let mut linked: Vec<Linked> = packages
        .iter()
        .filter(|p| seen.contains(p["id"].as_str().unwrap_or_default()))
        .filter(|p| p["source"].is_string()) // registry crates: not the workspace
        .map(|p| Linked {
            name: p["name"].as_str().unwrap_or_default().into(),
            version: p["version"].as_str().unwrap_or_default().into(),
            license: p["license"].as_str().unwrap_or("see its licence files").into(),
            authors: p["authors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| a.as_str())
                .map(|a| a.split(" <").next().unwrap_or(a).to_string())
                .collect(),
            repository: p["repository"].as_str().unwrap_or_default().into(),
            dir: Path::new(p["manifest_path"].as_str().unwrap_or_default())
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
        })
        .collect();
    linked.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(linked)
}

fn licenses() -> Result<Vec<Linked>, String> {
    let linked = linked_crates()?;
    let mut text = String::from(concat!(
        "# Third-party notices\n\n",
        "The wheel `hydrobem-gui` holds the Python package `hydrobem_gui` and, in its `bin` folder,\n",
        "`HydroBEM.exe` (the GUI with the Rust crates below compiled into it, statically linked) and the\n",
        "HydroBEM engine `hydrobem.dll` (with the crates compiled into it: `licenses/hydrobem/`). numpy is\n",
        "a dependency installed by pip, not shipped. Each keeps its own licence; `cargo xtask dist` copies\n",
        "their licence files to `dist/licenses/crates/<crate>-<version>/`, and the wheel carries them in\n",
        "its `.dist-info/licenses/`. This file is written by\n",
        "`cargo xtask licenses` from the run-time dependency graph (`cargo metadata`, Windows x64);\n",
        "rerun it after a dependency changes.\n\n",
        "| Component | Version | Licence | Use |\n|---|---|---|---|\n",
    ));
    let import = gui_core::engine_info::import();
    text.push_str(&format!(
        "| HydroBEM engine `hydrobem.dll` | ABI {} (`{}`) | Apache-2.0 (Copyright 2026 Sanghun Lee) | Built by HydroBEM, imported into `engine/` by `scripts/import-engine.ps1`, installed beside `HydroBEM.exe`; its `LICENSE`, `NOTICE` and third-party notices (with the crates compiled into it) are installed to `licenses/hydrobem/` |\n",
        import.abi_version, import.source_describe
    ));
    text.push_str(
        "| Visual C++ runtime (`vcruntime140.dll`), Universal C Runtime | system | Microsoft | Linked as DLLs, not distributed: Python's (it puts them first on the GUI's PATH) and Windows' |\n",
    );
    text.push_str(
        "| Segoe UI, Consolas, Segoe UI Symbol, Segoe UI Emoji, Malgun Gothic | system | Microsoft (Windows fonts) | Read from the Windows fonts folder at run time, not distributed: HydroBEM.exe carries no font |\n",
    );
    text.push_str("| HydroBEM user documentation (`docs/python/`) | HydroBEM `ee6c71b` | Apache-2.0 (Copyright 2026 Sanghun Lee) | Copied from HydroBEM's `docs/guide`, `docs/reference` and `docs/references.md` by `scripts/copy_hydrobem_docs.py` (the package renamed, this wheel's installation); not in the wheel |
");
    text.push_str("| HydroBEM examples (`examples/`) | HydroBEM `ee6c71b` | Apache-2.0 (Copyright 2026 Sanghun Lee) | Copied from HydroBEM's `examples/` (importing `hydrobem_gui`); installed into `dist/examples/` and the wheel |\n");
    text.push_str("| HydroBEM Python package and tests (`python/hydrobem_gui`, `python/tests`, `scripts/gen_model_classes.py`, `scripts/gen_cylinder_reference.py`, `scripts/gen_cylinder_array_reference.py`, `scripts/gen_sphere_array_reference.py`) | HydroBEM `ee6c71b` (first copied at `34adff1`) | Apache-2.0 (Copyright 2026 Sanghun Lee) | Copied from HydroBEM's `python/hydrobem`, `tests/python`, `tests/data`; the Python API of the wheel |\n");
    text.push_str("| Hulme (1982), J. Fluid Mech. 121, tables 1 and 2 (`python/tests/data/hulme1982*.txt`) | 1982 | Published values of their authors, not licensed by this project (`NOTICE`) | Reference values of the Python tests; not in the wheel |\n");
    text.push_str("| Kim and Yue (1990), J. Fluid Mech. 211, table 3 (`python/tests/data/kim_yue1990.txt`) | 1990 | Published values of their authors, not licensed by this project (`NOTICE`) | Reference values of the Python tests; not in the wheel |\n");
    for c in &linked {
        let name =
            if c.repository.is_empty() { format!("`{}`", c.name) } else { format!("[`{}`]({})", c.name, c.repository) };
        let license = c.license.replace('|', "\\|");
        text.push_str(&format!("| {name} | {} | {license} | Compiled in |\n", c.version));
    }
    std::fs::write(root().join("THIRD_PARTY_NOTICES.md"), text).map_err(|e| e.to_string())?;
    println!("wrote THIRD_PARTY_NOTICES.md ({} crates)", linked.len());
    Ok(linked)
}

// ---- Distribution ----------------------------------------------------------------------------

/// The MIT licence; `{holders}` is replaced by the copyright holders.
const MIT: &str = "MIT License

Copyright (c) {holders}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
";

/// The Boost Software License 1.0.
const BSL: &str = "Boost Software License - Version 1.0 - August 17th, 2003

Permission is hereby granted, free of charge, to any person or organization
obtaining a copy of the software and accompanying documentation covered by
this license (the \"Software\") to use, reproduce, display, distribute,
execute, and transmit the Software, and to prepare derivative works of the
Software, and to permit third-parties to whom the Software is furnished to
do so, all subject to the following:

The copyright notices in the Software and this entire statement, including
the above license grant, this restriction and the following disclaimer,
must be included in all copies of the Software, in whole or in part, and
all derivative works of the Software, unless such copies or derivative
works are solely in the form of machine-executable object code generated by
a source language processor.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE, TITLE AND NON-INFRINGEMENT. IN NO EVENT
SHALL THE COPYRIGHT HOLDERS OR ANYONE DISTRIBUTING THE SOFTWARE BE LIABLE
FOR ANY DAMAGES OR OTHER LIABILITY, WHETHER IN CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
";

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::copy(from, to).map(|_| ()).map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            copy(&entry.path(), &target)?;
        }
    }
    Ok(())
}

fn dist() -> Result<(), String> {
    let root = root();
    let exe = root.join("target/release/HydroBEM.exe");
    if !exe.is_file() {
        return Err("build first: cargo build --release".into());
    }
    let (dll, legal) = (root.join("engine/hydrobem.dll"), root.join("engine/legal"));
    if !dll.is_file() || !legal.is_dir() {
        return Err("import the engine first: scripts/import-engine.ps1".into());
    }
    // the notices first, so that dist/ gets the present ones
    let linked = licenses()?;
    let dist = root.join("dist");
    if dist.exists() {
        std::fs::remove_dir_all(&dist).map_err(|e| format!("{}: {e}", dist.display()))?;
    }
    copy(&exe, &dist.join("HydroBEM.exe"))?;
    copy(&dll, &dist.join("hydrobem.dll"))?;
    // the examples in their folders: their Python scripts and the README
    fn examples_into(from: &Path, to: &Path) -> Result<(), String> {
        for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let target = to.join(path.file_name().unwrap_or_default());
            if path.is_dir() {
                examples_into(&path, &target)?;
            } else if matches!(path.extension().and_then(|e| e.to_str()), Some("md" | "py")) {
                copy(&path, &target)?;
            }
        }
        Ok(())
    }
    examples_into(&root.join("examples"), &dist.join("examples"))?;
    copy_dir(&legal, &dist.join("licenses/hydrobem"))?;
    for name in ["THIRD_PARTY_NOTICES.md", "LICENSE", "NOTICE"] {
        if root.join(name).is_file() {
            copy(&root.join(name), &dist.join(name))?;
        }
    }
    // the licence texts of every crate compiled in
    for c in linked {
        let target = dist.join("licenses/crates").join(format!("{}-{}", c.name, c.version));
        let mut found = false;
        for entry in std::fs::read_dir(&c.dir).map_err(|e| format!("{}: {e}", c.dir.display()))? {
            let path = entry.map_err(|e| e.to_string())?.path();
            let name = path.file_name().map(|n| n.to_string_lossy().to_uppercase()).unwrap_or_default();
            let licence = ["LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT"].iter().any(|p| name.starts_with(p));
            if path.is_file() && licence {
                copy(&path, &target.join(path.file_name().unwrap_or_default()))?;
                found = true;
            }
        }
        // licence texts in a folder of their own: those of the fonts a crate compiles in
        let mut font_texts = 0;
        for folder in ["fonts", "LICENSES"] {
            let Ok(entries) = std::fs::read_dir(c.dir.join(folder)) else { continue };
            for entry in entries {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) {
                    copy(&path, &target.join(folder).join(path.file_name().unwrap_or_default()))?;
                    font_texts += 1;
                }
            }
        }
        // a font licence comes only with the fonts' own texts: refuse a crate that moved them
        if ["OFL-1.1", "Ubuntu-font-1.0"].iter().any(|id| c.license.contains(id)) && font_texts == 0 {
            return Err(format!("{} {}: {} but no fonts/*.txt licence text", c.name, c.version, c.license));
        }
        if !found {
            // the crate ships no licence file: the standard texts its licence expression names,
            // with its authors as the copyright holders
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            let holders = if c.authors.is_empty() { format!("the {} authors", c.name) } else { c.authors.join(", ") };
            let note = format!(
                "{} {}: {}\nThe crate ships no licence file; the licence texts beside this file are the\nstandard texts of that expression, with the crate's authors ({holders}) as the\ncopyright holders.\n",
                c.name, c.version, c.license
            );
            std::fs::write(target.join("LICENSE-SPDX.txt"), note).map_err(|e| e.to_string())?;
            let ids: Vec<&str> =
                c.license.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-' || ch == '.')).collect();
            if ids.contains(&"MIT") {
                std::fs::write(target.join("LICENSE-MIT"), MIT.replace("{holders}", &holders))
                    .map_err(|e| e.to_string())?;
            }
            if ids.contains(&"Apache-2.0") {
                copy(&root.join("LICENSE"), &target.join("LICENSE-APACHE"))?;
            }
            if ids.contains(&"BSL-1.0") {
                std::fs::write(target.join("LICENSE-BSL"), BSL).map_err(|e| e.to_string())?;
            }
            // OFL-1.1 and Ubuntu-font-1.0 come with the fonts (fonts/*.txt above)
            let known = ["MIT", "Apache-2.0", "BSL-1.0", "OR", "AND", "OFL-1.1", "Ubuntu-font-1.0", ""];
            if let Some(other) = ids.iter().find(|id| !known.contains(id)) {
                return Err(format!("{} {}: no standard text for {other}; add it to xtask", c.name, c.version));
            }
        }
    }
    println!("assembled {}", dist.display());
    Ok(())
}
