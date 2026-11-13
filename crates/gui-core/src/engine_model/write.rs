//! The project written as an engine model (`build_model`): the mesh objects, each body and its
//! surfaces, the QTFs, the GUI state, and the record ranges of a body's surfaces and wetted
//! surface.

use std::sync::Arc;

use hydrobem_engine::{Error, ErrorKind, Model, ObjectId, Value};

use super::read::tank_plate_mesh;
use super::{
    BuildOptions, BuiltModel, GUI_TOOL, GUI_TOOL_VERSION, GuiState, SavedBody, SavedLibrarySurface, SavedLid, WaveGui,
};
use crate::field_points::{FieldPointSelection, classify_grid};
use crate::mesh::Mesh;
use crate::project::{
    Body, ControlMethod, Domain, InteriorMethod, MODE_NAMES, MassCalculation, MatrixOrigin, PressureMode, Project,
    SurfaceSource, V3, WaveInputType, record_ranges,
};

/// The mesh objects of the model: one per mesh of the project (one GDF file read once is one
/// object), named after its first user.
struct MeshNames {
    entries: Vec<(u64, String)>,
}

impl MeshNames {
    /// The project's library, every mesh written as a mesh object under its name (used or not).
    fn library(m: &mut Model, project: &Project) -> hydrobem_engine::Result<MeshNames> {
        let mut names = MeshNames { entries: Vec::new() };
        let root = m.root();
        for entry in &project.meshes {
            let id = m.add(root, "meshes", None, &entry.name, None)?;
            entry.mesh.write_object(m, id)?;
            names.entries.push((entry.mesh.id, entry.name.clone()));
        }
        Ok(names)
    }

    /// The name of a mesh object; a mesh outside the library is added, named after `hint`.
    fn name(&mut self, m: &mut Model, mesh: &Option<Arc<Mesh>>, hint: &str) -> hydrobem_engine::Result<Option<String>> {
        let Some(mesh) = mesh else { return Ok(None) };
        if let Some((_, name)) = self.entries.iter().find(|(id, _)| *id == mesh.id) {
            return Ok(Some(name.clone()));
        }
        let base = crate::project::object_name(hint);
        let mut name = base.clone();
        let mut k = 2;
        while self.entries.iter().any(|(_, n)| *n == name) {
            name = format!("{base}_{k}");
            k += 1;
        }
        let root = m.root();
        let id = m.add(root, "meshes", None, &name, None)?;
        mesh.write_object(m, id)?;
        self.entries.push((mesh.id, name.clone()));
        Ok(Some(name))
    }
}

pub(super) fn vec3(v: V3) -> Value {
    Value::reals(&[3], v.to_vec())
}

fn write_body(m: &mut Model, b: ObjectId, body: &Body, names: &mut MeshNames) -> hydrobem_engine::Result<()> {
    let mesh = names.name(m, &body.mesh, &body.name)?;
    if let Some(name) = &mesh {
        // A record has one role in a body: the wetted surface is the body's range less what the
        // surfaces take.
        m.set_surface(b, "mesh", "ranges", name, &wetted_ranges(body).unwrap_or_default())?;
    }
    m.set(b, "position", vec3(body.position))?;
    m.set(b, "yaw", body.yaw)?;
    m.set(b, "rotation_center", vec3(body.reference))?;
    let modes: Vec<String> =
        MODE_NAMES.iter().zip(body.modes).filter(|(_, on)| *on).map(|(n, _)| n.to_string()).collect();
    m.set(b, "modes", modes)?;
    match body.mass_calculation {
        MassCalculation::FromMesh => m.set(b, "mass", "displaced")?,
        MassCalculation::Manual => m.set(b, "mass", body.mass)?,
    }
    m.set(b, "center_of_gravity", vec3(body.center_of_gravity))?;
    if let Some(trim) = body.trim {
        m.set(b, "trim", vec3(trim))?;
    }
    if body.haskind {
        m.set(b, "excitation_method", "haskind")?;
    }
    if let Some(points) = &body.body_points
        && !points.is_empty()
    {
        m.set(b, "body_points", Value::reals(&[points.len(), 3], points.iter().flatten().copied().collect()))?;
        if body.body_point_neighbors != 4 {
            m.set(b, "body_point_neighbors", body.body_point_neighbors.max(1))?;
        }
    }
    // The six components about R, or none (the engine's default: radii of gyration of zero);
    // zeros are the GUI's "not given".
    if body.inertia.iter().any(|v| *v != 0.0) {
        m.set(b, "inertia_input", "components")?;
        for (item, value) in ["Ixx", "Ixy", "Ixz", "Iyy", "Iyz", "Izz"].iter().zip(body.inertia) {
            m.set(b, item, value)?;
        }
        m.set(b, "inertia_origin", "rotation_center")?;
    }
    for (item, origin_item, matrix, origin) in [
        ("additional_stiffness", "stiffness_origin", &body.additional_stiffness, body.stiffness_origin),
        ("additional_damping", "damping_origin", &body.additional_damping, body.damping_origin),
    ] {
        if let Some(matrix) = matrix
            && !matrix.is_zero()
        {
            m.set(b, item, Value::reals(&[6, 6], matrix.full()))?;
            match origin {
                MatrixOrigin::RotationCenter => {}
                MatrixOrigin::CenterOfGravity => m.set(b, origin_item, "center_of_gravity")?,
                MatrixOrigin::Point(p) => m.set(b, origin_item, vec3(p))?,
            }
        }
    }
    // a surface: records of the body's mesh, or a mesh of the library (every record)
    let range = |first: i64, last: i64, more: &[[i64; 2]]| {
        mesh.as_ref().map(|name| (name.clone(), record_ranges(first, last, more)))
    };
    let interior = &body.interior;
    let mut interior_mesh = None;
    let method = match interior.method {
        InteriorMethod::PanelRange => {
            interior_mesh = range(interior.first_panel, interior.last_panel, &interior.more_ranges);
            if interior_mesh.is_some() { "mesh" } else { "none" }
        }
        InteriorMethod::Mesh => {
            interior_mesh = names.name(m, &interior.mesh, &format!("{}_interior", body.name))?.map(|n| (n, Vec::new()));
            if interior_mesh.is_some() { "mesh" } else { "none" }
        }
        method => method.engine_method().unwrap_or("none"),
    };
    m.set(b, "interior_surface", method)?;
    if let Some((name, ranges)) = &interior_mesh {
        m.set_surface(b, "interior_surface_mesh", "interior_surface_ranges", name, ranges)?;
    }
    for (k, dipole) in body.dipoles.iter().enumerate() {
        let value = match dipole.source {
            SurfaceSource::Generated => None,
            SurfaceSource::PanelRange => range(dipole.first_panel, dipole.last_panel, &dipole.more_ranges),
            SurfaceSource::ExternalMesh => {
                names.name(m, &dipole.mesh, &format!("{}_dipole_{}", body.name, k + 1))?.map(|n| (n, Vec::new()))
            }
        };
        if let Some((name, ranges)) = value {
            let d = m.add(b, "dipoles", None, "", None)?;
            m.set_surface(d, "mesh", "ranges", &name, &ranges)?;
            disable(m, d, dipole.enabled)?;
        }
    }
    for (k, damper) in body.dipole_dampers.iter().enumerate() {
        let value = match damper.source {
            SurfaceSource::Generated => None,
            SurfaceSource::PanelRange => range(damper.first_panel, damper.last_panel, &damper.more_ranges),
            SurfaceSource::ExternalMesh => {
                names.name(m, &damper.mesh, &format!("{}_damper_{}", body.name, k + 1))?.map(|n| (n, Vec::new()))
            }
        };
        if let Some((name, ranges)) = value {
            let d = m.add(b, "dipole_dampers", None, "", None)?;
            m.set_surface(d, "mesh", "ranges", &name, &ranges)?;
            m.set(d, "damping_factor", damper.damping_factor)?;
            disable(m, d, damper.enabled)?;
        }
    }
    for (k, tank) in body.tanks.iter().enumerate() {
        if let Some((name, ranges)) = range(tank.first_panel, tank.last_panel, &tank.more_ranges) {
            // named: the field points name their tank
            let t = m.add(b, "tanks", None, &tank.engine_name(k), None)?;
            m.set_surface(t, "mesh", "ranges", &name, &ranges)?;
            disable(m, t, tank.enabled)?;
            m.set(t, "density_ratio", tank.density_ratio)?;
            if let Some(z) = tank.surface_level {
                m.set(t, "surface_level", z)?;
            }
            for sheet in &tank.sheets {
                if let Some((name, ranges)) = range(sheet.first_panel, sheet.last_panel, &sheet.more_ranges) {
                    let s = m.add(t, if sheet.damper { "dipole_dampers" } else { "dipoles" }, None, "", None)?;
                    m.set_surface(s, "mesh", "ranges", &name, &ranges)?;
                    if sheet.damper {
                        m.set(s, "damping_factor", sheet.damping_factor)?;
                    }
                }
            }
            for (j, plate) in tank.free_surface_plates.iter().enumerate() {
                let value = match plate.source {
                    SurfaceSource::Generated => None,
                    SurfaceSource::PanelRange => range(plate.first_panel, plate.last_panel, &plate.more_ranges),
                    SurfaceSource::ExternalMesh => {
                        names.name(m, &plate.mesh, &tank_plate_mesh(body, tank, k, j))?.map(|n| (n, Vec::new()))
                    }
                };
                if let Some((name, ranges)) = value {
                    let p = m.add(t, "free_surface_plates", None, "", None)?;
                    m.set_surface(p, "mesh", "ranges", &name, &ranges)?;
                    disable(m, p, plate.enabled)?;
                }
            }
        }
    }
    if let Some(c) = &body.control_surface {
        match c.method.engine_name() {
            Some(method) => {
                m.set(b, "control_surface", method)?;
                m.set(b, "control_surface_panel_size", c.panel_size)?;
                match c.method {
                    ControlMethod::Cylinder => {
                        m.set(b, "control_surface_radius", c.radius)?;
                        m.set(b, "control_surface_depth", c.depth)?;
                        m.set(b, "control_surface_inner_radius", c.inner_radius)?;
                    }
                    ControlMethod::Ellipsoid => {
                        m.set(b, "control_surface_semi_axes", vec3(c.semi_axes))?;
                        m.set(b, "control_surface_inner_semi_axes", Value::reals(&[2], c.inner_semi_axes.to_vec()))?;
                    }
                    ControlMethod::Automatic => {
                        match &c.boundary {
                            Some(polygon) if polygon.len() >= 3 => {
                                m.set(b, "control_surface_outer", "polygon")?;
                                m.set(
                                    b,
                                    "control_surface_boundary",
                                    Value::reals(&[polygon.len(), 2], polygon.iter().flatten().copied().collect()),
                                )?
                            }
                            _ => m.set(b, "control_surface_radius", c.radius)?,
                        }
                        let lines: Vec<&Vec<[f64; 2]>> = c.partitions.iter().filter(|l| l.len() >= 2).collect();
                        if !lines.is_empty() {
                            // one polyline after the other, each ended by a row of NaN
                            let mut flat = Vec::new();
                            for line in &lines {
                                flat.extend(line.iter().flatten().copied());
                                flat.extend([f64::NAN, f64::NAN]);
                            }
                            m.set(b, "control_surface_partitions", Value::reals(&[flat.len() / 2, 2], flat))?;
                        }
                        m.set(b, "control_surface_depth", c.depth)?;
                    }
                    ControlMethod::Mesh => {}
                }
            }
            None => {
                if let Some(name) = names.name(m, &c.mesh, &format!("{}_control", body.name))? {
                    m.set(b, "control_surface", "mesh")?;
                    m.set(b, "control_surface_mesh", name.as_str())?;
                }
            }
        }
        if c.body_waterline_term && (c.method != ControlMethod::Mesh || c.mesh.is_some()) {
            m.set(b, "control_surface_waterline_term", "body")?;
        }
    }
    for g in &body.generalized_modes {
        let gm = m.add(b, "generalized_modes", None, &g.name, None)?;
        m.set(gm, "parity", MODE_NAMES[g.parity.min(5)])?;
        for piece in &g.pieces {
            let p = m.add(gm, "pieces", None, "", None)?;
            for (item, terms) in ["u", "v", "w"].iter().zip(&piece.terms) {
                if !terms.is_empty() {
                    m.set(p, item, Value::reals(&[terms.len(), 4], terms.iter().flatten().copied().collect()))?;
                }
            }
            if let (Some(range), Some(_)) = (piece.range, &mesh) {
                m.set(p, "ranges", Value::ranges(&record_ranges(range[0], range[1], &piece.more_ranges)))?;
            }
        }
    }
    if let Some(matrices) = &body.mode_matrices {
        let n = 6 + body.generalized_modes.len();
        for (item, values) in [
            ("mode_mass", &matrices.mass),
            ("mode_damping", &matrices.damping),
            ("mode_stiffness", &matrices.stiffness),
        ] {
            if values.len() == n * n && values.iter().any(|v| *v != 0.0) {
                m.set(b, item, Value::reals(&[n, n], values.clone()))?;
            }
        }
    }
    for (k, pressure) in body.pressure_surfaces.iter().enumerate() {
        let value = match pressure.source {
            SurfaceSource::Generated => None,
            SurfaceSource::PanelRange => range(pressure.first_panel, pressure.last_panel, &pressure.more_ranges),
            SurfaceSource::ExternalMesh => {
                names.name(m, &pressure.mesh, &format!("{}_pressure_{}", body.name, k + 1))?.map(|n| (n, Vec::new()))
            }
        };
        if let Some((name, ranges)) = value {
            // Mode names are unique among the body's modes.
            let s = m.add(b, "pressure_surfaces", None, "", None)?;
            m.set_surface(s, "mesh", "ranges", &name, &ranges)?;
            disable(m, s, pressure.enabled)?;
            m.set(s, "chamber_coupling", pressure.chamber_coupling)?;
            if pressure.numeric_convention {
                m.set(s, "response_convention", "numeric_compatibility")?;
            }
            let modes = if pressure.pressure_modes.is_empty() {
                vec![PressureMode::default()]
            } else {
                pressure.pressure_modes.clone()
            };
            for (j, mode) in modes.iter().enumerate() {
                let name = if modes.len() == 1 {
                    format!("Uniform_{}", k + 1)
                } else {
                    format!("Pressure_{}_{}", k + 1, j + 1)
                };
                let pm = m.add(s, "pressure_modes", None, &name, None)?;
                m.set(pm, "shape", mode.shape.engine_name())?;
                if mode.stiffness != 0.0 {
                    m.set(pm, "additional_stiffness", mode.stiffness)?;
                }
                if mode.damping != 0.0 {
                    m.set(pm, "additional_damping", mode.damping)?;
                }
            }
        }
    }
    for (k, plate) in body.free_surface_plates.iter().enumerate() {
        if plate.source == SurfaceSource::Generated {
            // the water of z = 0 inside a waterline loop of a trimmed hull, made by the engine
            let p = m.add(b, "free_surface_plates", None, "", None)?;
            m.set(p, "inside", Value::reals(&[2], plate.inside.to_vec()))?;
            if let Some(other) = plate.body.as_deref().filter(|o| !o.is_empty()) {
                m.set(p, "body", other)?;
            }
            disable(m, p, plate.enabled)?;
            continue;
        }
        let value = match plate.source {
            SurfaceSource::Generated => None,
            SurfaceSource::PanelRange => range(plate.first_panel, plate.last_panel, &plate.more_ranges),
            SurfaceSource::ExternalMesh => {
                names.name(m, &plate.mesh, &format!("{}_plate_{}", body.name, k + 1))?.map(|n| (n, Vec::new()))
            }
        };
        if let Some((name, ranges)) = value {
            let p = m.add(b, "free_surface_plates", None, "", None)?;
            m.set_surface(p, "mesh", "ranges", &name, &ranges)?;
            disable(m, p, plate.enabled)?;
        }
    }
    Ok(())
}

/// The QTFs and the free surface of the potential QTF (`second_order`); an `"auto"` is left unset.
fn write_qtf(
    m: &mut Model,
    second: ObjectId,
    s: &crate::project::SecondOrder,
    names: &mut MeshNames,
) -> hydrobem_engine::Result<()> {
    for (item, on) in [
        ("qtf_quadratic_pressure_integration", s.qtf_quadratic_pressure_integration),
        ("qtf_quadratic_control_surface", s.qtf_quadratic_control_surface),
        ("qtf_potential_direct", s.qtf_potential_direct),
        ("qtf_potential_indirect", s.qtf_potential_indirect),
        ("qtf_newman", s.qtf_newman),
        ("qtf_bidirectional", s.qtf_bidirectional),
        ("qtf_difference", s.qtf_difference),
        ("qtf_sum", s.qtf_sum),
    ] {
        m.set(second, item, on)?;
    }
    m.set(second, "qtf_newman_average", s.qtf_newman_average.engine_name())?;
    m.set(second, "free_surface_method", s.free_surface_method.engine_name())?;
    for (item, value) in [
        ("free_surface_panel_size", s.free_surface_panel_size),
        ("free_surface_inner_radius", s.free_surface_inner_radius),
        ("quadrature_radius_step", s.quadrature_radius_step),
    ] {
        if let Some(v) = value {
            m.set(second, item, v)?;
        }
    }
    for (item, value) in [
        ("quadrature_azimuthal_nodes", s.quadrature_azimuthal_nodes),
        ("outer_circle_segments", s.outer_circle_segments),
        ("expansion_order", s.expansion_order),
    ] {
        if let Some(v) = value {
            m.set(second, item, v)?;
        }
    }
    m.set(second, "free_surface_quadrature_points", s.free_surface_quadrature_points)?;
    m.set(second, "quadrature_annuli", s.quadrature_annuli)?;
    m.set(second, "quadrature_radial_nodes", s.quadrature_radial_nodes)?;
    // the mesh is kept (unused) while another method is chosen
    if let Some(name) = names.name(m, &s.free_surface_mesh, "free_surface")? {
        m.set(second, "free_surface_mesh", name.as_str())?;
    }
    Ok(())
}

/// `enabled` false for an object switched off (true is the engine's default, left unset).
fn disable(m: &mut Model, id: ObjectId, enabled: bool) -> hydrobem_engine::Result<()> {
    if !enabled {
        m.set(id, "enabled", false)?;
    }
    Ok(())
}

/// The records of the body's own mesh that its interior surface, dipoles, dampers, pressure
/// surfaces, tanks and placed lids take (one-based, inclusive), switched off or not: the body's
/// reference leaves them out either way, as in the engine, where a disabled object leaves its
/// records without a role.
pub fn surface_ranges(body: &Body) -> Vec<[i64; 2]> {
    let mut ranges = Vec::new();
    if body.interior.method == InteriorMethod::PanelRange {
        let i = &body.interior;
        ranges.extend(record_ranges(i.first_panel, i.last_panel, &i.more_ranges));
    }
    for s in
        body.dipoles.iter().chain(&body.pressure_surfaces).chain(&body.dipole_dampers).chain(&body.free_surface_plates)
    {
        if s.source == SurfaceSource::PanelRange {
            ranges.extend(record_ranges(s.first_panel, s.last_panel, &s.more_ranges));
        }
    }
    ranges.extend(body.placed_lids.iter().copied());
    for t in &body.tanks {
        ranges.extend(record_ranges(t.first_panel, t.last_panel, &t.more_ranges));
        ranges.extend(t.sheets.iter().flat_map(|s| record_ranges(s.first_panel, s.last_panel, &s.more_ranges)));
        ranges.extend(
            t.free_surface_plates
                .iter()
                .filter(|s| s.source == SurfaceSource::PanelRange)
                .flat_map(|s| record_ranges(s.first_panel, s.last_panel, &s.more_ranges)),
        );
    }
    ranges
}

/// The stored records of the body's mesh that form its wetted surface: its range less the
/// records its surfaces take. None: every record of the mesh (no range, no surface range).
pub fn wetted_ranges(body: &Body) -> Option<Vec<[i64; 2]>> {
    let mesh = body.mesh.as_ref()?;
    let n = mesh.panels.len() as i64;
    let mut taken = surface_ranges(body);
    taken.extend(body.mesh_gaps.iter().copied());
    if body.mesh_range.is_none() && taken.is_empty() {
        return None;
    }
    Some(remaining(body.mesh_range.unwrap_or([1, n]), n, &taken))
}

/// The records 1..=n outside `taken`, as ranges.
pub fn complement(n: i64, taken: &[[i64; 2]]) -> Vec<[i64; 2]> {
    remaining([1, n], n, taken)
}

/// The records of `range` (clamped to 1..=n) outside `taken`, as ranges.
pub fn remaining(range: [i64; 2], n: i64, taken: &[[i64; 2]]) -> Vec<[i64; 2]> {
    let mut free = vec![false; n.max(0) as usize];
    for r in range[0].max(1)..=range[1].min(n) {
        free[(r - 1) as usize] = true;
    }
    for [a, b] in taken {
        for r in (*a).max(1)..=(*b).min(n) {
            free[(r - 1) as usize] = false;
        }
    }
    let mut out: Vec<[i64; 2]> = Vec::new();
    for (i, f) in free.iter().enumerate() {
        let r = i as i64 + 1;
        if !*f {
            continue;
        }
        match out.last_mut() {
            Some(last) if last[1] == r - 1 => last[1] = r,
            _ => out.push([r, r]),
        }
    }
    out
}

fn saved_body(m: &mut Model, body: &Body, names: &mut MeshNames) -> hydrobem_engine::Result<SavedBody> {
    Ok(SavedBody {
        body: body.clone(),
        mesh: names.name(m, &body.mesh, &body.name)?,
        dipole_meshes: body
            .dipoles
            .iter()
            .enumerate()
            .map(|(k, s)| names.name(m, &s.mesh, &format!("{}_dipole_{}", body.name, k + 1)))
            .collect::<hydrobem_engine::Result<_>>()?,
        plate_meshes: body
            .free_surface_plates
            .iter()
            .enumerate()
            .map(|(k, s)| names.name(m, &s.mesh, &format!("{}_plate_{}", body.name, k + 1)))
            .collect::<hydrobem_engine::Result<_>>()?,
        tank_plate_meshes: body
            .tanks
            .iter()
            .enumerate()
            .map(|(k, t)| {
                t.free_surface_plates
                    .iter()
                    .enumerate()
                    .map(|(j, s)| names.name(m, &s.mesh, &tank_plate_mesh(body, t, k, j)))
                    .collect::<hydrobem_engine::Result<Vec<_>>>()
            })
            .collect::<hydrobem_engine::Result<_>>()?,
        pressure_meshes: body
            .pressure_surfaces
            .iter()
            .enumerate()
            .map(|(k, s)| names.name(m, &s.mesh, &format!("{}_pressure_{}", body.name, k + 1)))
            .collect::<hydrobem_engine::Result<_>>()?,
        damper_meshes: body
            .dipole_dampers
            .iter()
            .enumerate()
            .map(|(k, s)| names.name(m, &s.mesh, &format!("{}_damper_{}", body.name, k + 1)))
            .collect::<hydrobem_engine::Result<_>>()?,
        control_mesh: match &body.control_surface {
            Some(c) => names.name(m, &c.mesh, &format!("{}_control", body.name))?,
            None => None,
        },
        interior_mesh: names.name(m, &body.interior.mesh, &format!("{}_interior", body.name))?,
    })
}

/// The project as an engine model. Meshes shared by several users are one mesh object. The
/// error is the engine's, with the object path of the value it rejected.
pub fn build_model(project: &Project, options: &BuildOptions) -> hydrobem_engine::Result<BuiltModel> {
    let mut synced = project.clone();
    synced.sync_placed_lids();
    let project = &synced;
    let mut m = Model::new();
    let root = m.root();
    let mut names = MeshNames::library(&mut m, project)?;
    let env = &project.environment;

    let e = m.object("environment")?;
    m.set(e, "depth", env.engine_depth())?;
    m.set(e, "gravity", env.gravity)?;
    m.set(e, "density", env.density)?;
    m.set(e, "domain", env.domain.engine_name())?;
    if env.domain == Domain::Channel {
        m.set(e, "channel_width", env.channel_width)?;
        if env.channel_reflect != 1.0 {
            m.set(e, "channel_reflect", env.channel_reflect)?;
        }
    }

    let o = m.object("options")?;
    m.set(o, "logarithmic_integration", project.options.logarithmic_integration)?;
    // the higher-order method when the bodies have patch meshes (every one must: the engine's rule)
    if project.active_bodies().any(|b| b.mesh.as_ref().is_some_and(|m| m.is_patch())) {
        m.set(o, "higher_order", true)?;
    }
    m.set(
        o,
        "log_tolerance",
        if project.options.log_tolerance.is_nan() { 0.0 } else { project.options.log_tolerance },
    )?;
    m.set(o, "frequency_threads", project.options.frequency_threads.clamp(1, 1024))?;
    m.set(o, "inner_threads", project.options.inner_threads.clamp(1, 1024))?;

    let out = m.object("first_order")?;
    m.set(out, "radiation", project.first_order.radiation)?;
    m.set(out, "diffraction", project.first_order.diffraction)?;
    m.set(out, "diffraction_formulation", project.first_order.diffraction_formulation.engine_name())?;
    m.set(out, "motion", project.first_order.motion)?;
    m.set(out, "body_pressure", project.first_order.body_pressure)?;
    m.set(out, "body_velocity", project.first_order.body_velocity)?;
    let second = m.object("second_order")?;
    let s = &project.second_order;
    m.set(second, "drift_momentum_conservation", s.drift_momentum_conservation)?;
    m.set(second, "drift_bidirectional", s.drift_bidirectional)?;
    if let Some(angles) = project.second_order.drift_momentum_conservation_angles {
        m.set(second, "drift_momentum_conservation_angles", angles)?;
    }
    m.set(second, "drift_pressure_integration", project.second_order.drift_pressure_integration)?;
    m.set(second, "drift_control_surface", project.second_order.drift_control_surface)?;
    write_qtf(&mut m, second, &project.second_order, &mut names)?;

    let f = m.object("environment")?;
    let waves = &project.waves.frequencies;
    if !waves.inputs.is_empty() {
        // the limits: omega = 0 is an infinite period, omega = inf a period of 0
        let (zero, inf) = match waves.input_type {
            WaveInputType::Period => (f64::INFINITY, 0.0),
            WaveInputType::Frequency => (0.0, f64::INFINITY),
        };
        let mut values = if waves.input_type == WaveInputType::Period { waves.inputs.clone() } else { waves.omegas() };
        if waves.zero_limit && !options.placeholder_frequency {
            values.push(zero);
        }
        if waves.infinite_limit && !options.placeholder_frequency {
            values.push(inf);
        }
        if waves.input_type == WaveInputType::Period {
            m.set(f, "frequency_input", "periods")?;
            m.set(f, "periods", values)?;
        } else {
            m.set(f, "omegas", values)?;
        }
    } else if options.placeholder_frequency {
        m.set(f, "omegas", vec![1.0])?;
    }
    if project.first_order.diffraction {
        // the engine solves headings only with diffraction
        let mut headings = project.waves.headings.values();
        if headings.is_empty() && options.placeholder_frequency {
            headings = vec![0.0];
        }
        if !headings.is_empty() {
            m.set(f, "headings", headings)?;
        }
    }

    let mut body_names = Vec::new();
    let mut model_bodies = Vec::new();
    for body in &project.bodies {
        if options.meshed_bodies_only && !body.has_geometry() {
            continue;
        }
        let b = m.add(root, "bodies", None, &body.name, None)?;
        let written = write_body(&mut m, b, body, &mut names);
        if body.suppressed {
            // a suppressed body the engine cannot hold stays in the GUI state alone
            if written.and_then(|_| m.set(b, "enabled", false)).is_err() {
                m.remove(b)?;
                continue;
            }
        } else {
            written?;
            body_names.push(body.name.clone());
        }
        model_bodies.push(body.name.clone());
    }
    // the couplings once every body they may name is there
    for body in project.bodies.iter().filter(|b| model_bodies.contains(&b.name)) {
        let b = m.object(&format!("bodies/{}", body.name))?;
        for c in body.couplings.iter().filter(|c| model_bodies.contains(&c.body)) {
            let id = m.add(b, "couplings", None, "", None)?;
            m.set(id, "body", c.body.as_str())?;
            for (item, matrix) in [("stiffness", &c.stiffness), ("damping", &c.damping)] {
                if !matrix.is_zero() {
                    m.set(id, item, Value::reals(&[6, 6], matrix.full()))?;
                }
            }
            disable(&mut m, id, c.enabled)?;
        }
    }
    for lid in project.lids.iter().filter(|l| l.mesh.is_some() || l.placed_with.is_some()) {
        let l = m.add(root, "lids", None, &lid.name, None)?;
        disable(&mut m, l, !lid.suppressed)?;
        match lid.placed_with.as_ref().and_then(|n| project.bodies.iter().find(|b| &b.name == n)) {
            // records of the body's mesh, placed by the body
            Some(body) => {
                if let Some(mesh) = names.name(&mut m, &body.mesh, &body.name)? {
                    let ranges = record_ranges(lid.range[0], lid.range[1], &lid.more_ranges);
                    m.set_surface(l, "mesh", "ranges", &mesh, &ranges)?;
                }
                m.set(l, "placed_with", body.name.as_str())?;
            }
            None => {
                if let Some(mesh) = names.name(&mut m, &lid.mesh, &format!("lid_{}", lid.name))? {
                    m.set(l, "mesh", mesh.as_str())?;
                }
            }
        }
        m.set(l, "damping_factor", lid.damping_factor)?;
    }
    for (k, damper) in project.fixed_dampers.iter().enumerate() {
        if let Some(mesh) = names.name(&mut m, &damper.mesh, &format!("water_damper_{}", k + 1))? {
            let d = m.add(root, "fixed_dampers", None, "", None)?;
            m.set(d, "mesh", mesh.as_str())?;
            m.set(d, "damping_factor", damper.damping_factor)?;
            disable(&mut m, d, damper.enabled)?;
        }
    }

    if options.gui_state {
        let mut state = GuiState {
            version: GUI_TOOL_VERSION,
            finite_depth: Some(env.depth),
            wave_frequencies: Some(WaveGui {
                edit_in_table: waves.edit_in_table,
                start: waves.start,
                end: waves.end,
                count: waves.count,
            }),
            field_grid: Some(project.field_points.clone()),
            headings: Some(project.waves.headings.clone()),
            analytic_meshes: project
                .meshes
                .iter()
                .filter_map(|e| e.mesh.analytic.clone().map(|spec| (e.name.clone(), spec)))
                .collect(),
            patch_reports: project
                .meshes
                .iter()
                .filter_map(|e| e.mesh.report.as_ref().map(|r| (e.name.clone(), (**r).clone())))
                .collect(),
            ..GuiState::default()
        };
        for body in &project.bodies {
            state.bodies.push(saved_body(&mut m, body, &mut names)?);
        }
        for lid in &project.lids {
            let mesh = names.name(&mut m, &lid.mesh, &format!("lid_{}", lid.name))?;
            state.lids.push(SavedLid { lid: lid.clone(), mesh });
        }
        for (k, damper) in project.fixed_dampers.iter().enumerate() {
            let mesh = names.name(&mut m, &damper.mesh, &format!("water_damper_{}", k + 1))?;
            state.fixed_dampers.push(SavedLibrarySurface { surface: damper.clone(), mesh });
        }
        let text = serde_json::to_string(&state).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
        let t = m.add(root, "tool", None, GUI_TOOL, None)?;
        m.set(t, "text", text)?;
    }

    let mut field_points = FieldPointSelection::default();
    if options.field_points && project.field_points.enabled {
        let tanks = project.field_point_tanks();
        field_points = classify_grid(&mut m, project, tanks.as_deref());
        if !field_points.points.is_empty() {
            // the points in the sea (global), those of each tank in its body's axes
            let sea: Vec<f64> = field_points.sea_points().into_iter().flatten().collect();
            if !sea.is_empty() {
                m.set(out, "field_points", Value::reals(&[sea.len() / 3, 3], sea))?;
            }
            for (path, local) in field_points.tank_points(project) {
                let (body, tank) = path.split_once('/').unwrap_or((path.as_str(), ""));
                let t = m.object(&format!("bodies/{body}/tanks/{tank}"))?;
                let xyz: Vec<f64> = local.into_iter().flatten().collect();
                m.set(t, "field_points", Value::reals(&[xyz.len() / 3, 3], xyz))?;
            }
            m.set(out, "field_pressure", project.first_order.field_pressure)?;
            // after the field points: the engine builds no model asking for field velocities
            // without them, and the points are judged on a model it can build
            m.set(out, "field_velocity", project.first_order.field_velocity)?;
        }
    }
    Ok(BuiltModel { model: m, body_names, model_bodies, field_points })
}
