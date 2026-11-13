//! Project files (`.hbm`, the engine's model file) and results files (`.hbr`), docs/FILES.md,, read and written by the engine. Saving writes a temporary file beside
//! the target and renames it, so a failed save leaves the old file as it was.

use std::path::{Path, PathBuf};

use hydrobem_engine::Model;
use serde::Serialize;

use crate::engine_model::{BuildOptions, build_model, read_model};
use crate::project::Project;
use crate::results::SolveResult;

fn temporary(target: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    target.with_file_name(format!(".{name}.saving-{stamp}"))
}

fn replace(temp: &Path, target: &Path) -> Result<(), String> {
    std::fs::rename(temp, target).map_err(|e| {
        let _ = std::fs::remove_file(temp);
        format!("cannot replace {}: {e}", target.display())
    })
}

/// Saves the project as a model file.
pub fn save_project(project: &Project, file: &Path) -> Result<(), String> {
    let built = build_model(project, &BuildOptions::default()).map_err(|e| e.message().to_string())?;
    let temp = temporary(file);
    built.model.save(&temp, true).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        e.message().to_string()
    })?;
    replace(&temp, file)
}

/// Opens a model file (or the inputs of a results file). Returns the project and the warnings:
/// inputs the GUI had to adapt.
pub fn load_project(file: &Path) -> Result<(Project, Vec<String>), String> {
    let model = Model::load(file).map_err(|e| e.message().to_string())?;
    read_model(&model)
}

/// Saves the results of a solve (with the model solved inside) as a results file.
pub fn save_results(result: &SolveResult, file: &Path) -> Result<(), String> {
    let temp = temporary(file);
    result.results.save(&temp, true).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        e.message().to_string()
    })?;
    replace(&temp, file)
}

/// Exports the results of a solve as WAMIT-format files, written by the engine: the numeric files
/// (`STEM.1`, `.2`, `.3`, ...) and the summary `STEM.out`, where `file` is the `.out` file (or any
/// file of the stem). The first column is the wave frequency in rad/s (WAMIT's IPERIN = 2), not the
/// period, when `omega`, else the period in s (IPERIN = 1). Files of the stem already there are
/// replaced.
pub fn export_wamit(result: &SolveResult, file: &Path, omega: bool) -> Result<(), String> {
    let stem = file.with_extension("");
    result.results.write_wamit(&stem, true, omega).map_err(|e| e.message().to_string())
}

/// The version of the `meta` array of the NumPy export.
pub const NPZ_VERSION: u32 = 1;

/// Exports the results of a solve as a NumPy archive (`.npz`, `docs/FILES.md`): every quantity of
/// the results under its engine name, in the engine's type and dimensions (`Results::shape`, the
/// documented F/H/D/K/P order); `bodies`, the solved bodies' names (the B axis); `meta`, a JSON text
/// of the run, the warnings and each quantity's axes, unit and description. Written to a temporary
/// name beside the target and renamed.
pub fn export_npz(result: &SolveResult, file: &Path) -> Result<(), String> {
    use crate::npz::{self, Array};
    let r = &result.results;
    let mut arrays = Vec::new();
    let mut quantities = serde_json::Map::new();
    for q in r.available() {
        let Some(shape) = r.shape(q) else { continue };
        let Some(descr) = npz::descr(shape.element_type) else { continue };
        let data = r.bytes(q).map_err(|e| e.message().to_string())?;
        arrays.push((q.name().to_string(), Array { descr: descr.into(), shape: shape.dims, data: data.into() }));
        quantities.insert(
            q.name().into(),
            serde_json::json!({ "dims": q.dims(), "unit": q.unit(), "description": q.description() }),
        );
    }
    let names: Vec<&str> = result.bodies.iter().map(|b| b.name.as_str()).collect();
    arrays.push(("bodies".into(), Array::texts(&names)));

    // what an independent calculation needs besides the quantities: the motion equation and the
    // placement of each body, from the model as it was solved (its bodies are the B axis)
    let engine = |e: hydrobem_engine::Error| e.message().to_string();
    let mut solved = r.solved_model().map_err(engine)?;
    let reals = |a: Vec<f64>, shape: Vec<usize>| Array {
        descr: "<f8".into(),
        shape,
        data: a.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>().into(),
    };
    let motion = solved.motion_equation().map_err(engine)?;
    if let Some(me) = &motion {
        for (key, m) in [("inertia", &me.inertia), ("stiffness", &me.stiffness), ("damping", &me.damping)] {
            arrays.push((format!("motion_equation_{key}"), reals(m.clone(), vec![me.size, me.size])));
        }
    }
    let bodies = solved.children(solved.root(), "bodies").map_err(engine)?.to_vec();
    // an item of n reals; an unset trim is none (zeros), any other item missing is an error
    let item = |b, name: &str, n: usize| -> Result<Vec<f64>, String> {
        let v = solved.get(b, name).map_err(engine)?;
        let v = v.as_ref().and_then(|v| v.as_reals().map(<[f64]>::to_vec).or_else(|| v.as_real().map(|x| vec![x])));
        match v {
            Some(v) if v.len() == n => Ok(v),
            None if name == "trim" => Ok(vec![0.0; n]),
            v => Err(format!("the solved model's {name} of a body is {v:?}, not {n} numbers")),
        }
    };
    let (mut position, mut yaw, mut centre, mut trim) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for &b in &bodies {
        position.extend(item(b, "position", 3)?);
        yaw.extend(item(b, "yaw", 1)?);
        centre.extend(item(b, "rotation_center", 3)?);
        trim.extend(item(b, "trim", 3)?);
    }
    let count = bodies.len();
    arrays.push(("body_position".into(), reals(position, vec![count, 3])));
    arrays.push(("body_yaw".into(), reals(yaw, vec![count])));
    arrays.push(("body_rotation_center".into(), reals(centre, vec![count, 3])));
    arrays.push(("body_trim".into(), reals(trim, vec![count, 3])));
    let mut model_keys = serde_json::Map::new();
    let mut describe = |key: &str, dims: &str, unit: &str, description: &str| {
        model_keys.insert(key.into(), serde_json::json!({ "dims": dims, "unit": unit, "description": description }));
    };
    if motion.is_some() {
        let axes = "rigid coordinate (6 per body: surge, sway, heave, roll, pitch, yaw), rigid coordinate";
        describe(
            "motion_equation_inertia",
            axes,
            "kg, kg m, kg m^2",
            "the rigid-body mass and inertia matrix of the motion equation, body axes about each rotation centre R",
        );
        describe(
            "motion_equation_stiffness",
            axes,
            "N/m, N, N m",
            "the stiffness of the motion equation: hydrostatic (rho g), weight and additional, body axes about each R",
        );
        describe(
            "motion_equation_damping",
            axes,
            "N s/m, N s, N m s",
            "the additional (external) damping of the motion equation, body axes about each R",
        );
    }
    describe("body_position", "body,xyz", "m", "the global position of the body origin");
    describe(
        "body_yaw",
        "body",
        "deg",
        "the rotation of the body about the global z axis, applied before the translation",
    );
    describe(
        "body_rotation_center",
        "body,xyz",
        "m",
        "the motion reference point R, body-local: forces, matrices and motions are about R",
    );
    describe(
        "body_trim",
        "body,elevation_pitch_roll",
        "m, deg, deg",
        "the trim of the hull in body coordinates (zero: none)",
    );
    let m = r.meta();
    let warnings: Vec<_> = r
        .diagnostics()
        .iter()
        .map(|d| serde_json::json!({ "severity": format!("{:?}", d.severity).to_lowercase(), "code": d.code, "message": d.message }))
        .collect();
    let meta = serde_json::json!({
        "format": "hydrobem-gui npz",
        "version": NPZ_VERSION,
        "route": r.route(),
        "run": {
            "hydrobem_version": m.hydrobem_version, "commit": m.commit, "compiler": m.compiler,
            "linear_algebra": m.linear_algebra, "build_type": m.build_type, "time": m.time,
            "api_version": m.api_version, "frequency_threads": m.frequency_threads,
            "inner_threads": m.inner_threads, "engine_seconds": m.engine_seconds,
        },
        "warnings": warnings,
        "quantities": quantities,
        "motion_equation_ready": motion.is_some(),
        "second_order_motions": "the drifts and QTFs of a moving body are of its first-order motions `motion`, the RAOs the engine solved with motion_equation_* (and, for pressure and generalized coordinates, mode_restoring and pressure_mode_damping); a calculation with other stiffness or damping moves differently from what they assume. A fixed body has no motions in them",
        "model": model_keys,
    });
    arrays.push(("meta".into(), Array::text(&meta.to_string())));

    let temp = temporary(file);
    let written = std::fs::File::create(&temp)
        .and_then(|f| npz::write(std::io::BufWriter::new(f), &arrays).map(drop))
        .map_err(|e| format!("cannot write {}: {e}", file.display()));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    replace(&temp, file)
}

/// A results file opened as the project solved and its result.
pub struct OpenedResults {
    pub project: Project,
    pub result: SolveResult,
    pub warnings: Vec<String>,
}

pub fn open_results(file: &Path) -> Result<OpenedResults, String> {
    let model = Model::load_results(file).map_err(|e| e.message().to_string())?;
    let (project, warnings) = read_model(&model)?;
    let results = model.results().map_err(|e| e.message().to_string())?.clone();
    let body_names: Vec<String> = project.active_bodies().map(|b| b.name.clone()).collect();
    let (points, sources) = crate::field_points::model_field_points(&model, &project);
    let mut result = SolveResult::new(results, &body_names, points, sources);
    result.bidirectional_drift = crate::results::asks_bidirectional_drift(&model);
    result.field_grid = project.field_points.clone();
    result.model_bodies = project.bodies.iter().map(|b| b.name.clone()).collect();
    // the faces of the body outputs: the model of the solve, as solved
    let options = crate::engine_model::BuildOptions { field_points: false, gui_state: false, ..Default::default() };
    if let Ok(mut built) = crate::engine_model::build_model(&project, &options) {
        result.panels = crate::results::solved_panels(&mut built.model);
        result.surface = crate::results::solved_surface(&mut built.model);
        result.planes = crate::results::solved_planes(&mut built.model);
    }
    result.generated_field_points = project.field_points.generate().len();
    Ok(OpenedResults { project, result, warnings })
}

/// A text that changes whenever the saved model would: every input and GUI state, each mesh as a
/// checksum. For finding unsaved changes.
pub fn project_fingerprint(project: &Project) -> String {
    #[derive(Serialize)]
    struct Snapshot<'a> {
        meshes: Vec<(&'a str, u64)>,
        bodies: Vec<(&'a crate::project::Body, Vec<Option<u64>>)>,
        lids: Vec<(&'a crate::project::DampingLid, Option<u64>)>,
        dipole_dampers: Vec<(&'a crate::project::PanelSurface, Option<u64>)>,
        environment: &'a crate::project::Environment,
        options: &'a crate::project::Options,
        first_order: &'a crate::project::FirstOrder,
        second_order: &'a crate::project::SecondOrder,
        free_surface_mesh: Option<u64>,
        field_points: &'a crate::project::FieldPoints,
        waves: &'a crate::project::Waves,
    }
    let sum = |m: &Option<std::sync::Arc<crate::mesh::Mesh>>| m.as_ref().map(|m| m.checksum());
    let snapshot = Snapshot {
        meshes: project.meshes.iter().map(|e| (e.name.as_str(), e.mesh.checksum())).collect(),
        bodies: project
            .bodies
            .iter()
            .map(|b| {
                // every mesh a body refers to (the meshes are not serialized with the body)
                let surfaces = b
                    .dipoles
                    .iter()
                    .chain(&b.pressure_surfaces)
                    .chain(&b.dipole_dampers)
                    .chain(&b.free_surface_plates)
                    .chain(b.tanks.iter().flat_map(|t| &t.free_surface_plates))
                    .map(|s| sum(&s.mesh));
                let others = [sum(&b.interior.mesh), b.control_surface.as_ref().and_then(|c| sum(&c.mesh))];
                (b, std::iter::once(sum(&b.mesh)).chain(surfaces).chain(others).collect())
            })
            .collect(),
        lids: project.lids.iter().map(|l| (l, sum(&l.mesh))).collect(),
        dipole_dampers: project.fixed_dampers.iter().map(|s| (s, sum(&s.mesh))).collect(),
        environment: &project.environment,
        options: &project.options,
        first_order: &project.first_order,
        second_order: &project.second_order,
        free_surface_mesh: sum(&project.second_order.free_surface_mesh),
        field_points: &project.field_points,
        waves: &project.waves,
    };
    serde_json::to_string(&snapshot).unwrap_or_default()
}
