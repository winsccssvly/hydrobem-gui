//! The project of an engine model (`read_model`) and the helpers of reading: surfaces merged with
//! the GUI state's, inertia moved, axes and point lists compared.

use std::collections::HashMap;

use hydrobem_engine::{Model, ObjectId, Value};

use super::reader::{Reader, engine, read_qtf, unsupported};
use super::{GUI_TOOL, GUI_TOOL_VERSION, GuiState, SavedBody};
use crate::project::{
    Body, DampingLid, DepthType, Domain, Formulation, PanelSurface, Project, SurfaceSource, Tank, TankRef, V3,
    WaveFrequencies, WaveHeadings, WaveInputType,
};

/// The surfaces of the problem over those of the GUI state: the problem's replace the ones it can
/// hold (a range, or a chosen mesh) in order, keeping those without a mesh, when their counts
/// agree; else the problem's alone.
pub(super) fn merge_surfaces(saved: Vec<PanelSurface>, read: Vec<PanelSurface>) -> Vec<PanelSurface> {
    let written: Vec<usize> = saved.iter().enumerate().filter(|(_, s)| !s.missing_mesh()).map(|(i, _)| i).collect();
    if written.len() != read.len() {
        return read;
    }
    let mut out = saved;
    for (k, surface) in written.into_iter().zip(read) {
        out[k] = surface;
    }
    out
}

/// Inertia components about `from` moved to the rotation centre R through G (parallel axes):
/// I_A = I_G + m (|d|^2 E - d d^T), d = A - G.
pub fn move_inertia(body: &mut Body, from: V3) {
    let m = body.mass;
    let shift = |d: V3, sign: f64, i: &mut [f64; 6]| {
        let dd = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        i[0] += sign * m * (dd - d[0] * d[0]);
        i[1] += sign * m * (-d[0] * d[1]);
        i[2] += sign * m * (-d[0] * d[2]);
        i[3] += sign * m * (dd - d[1] * d[1]);
        i[4] += sign * m * (-d[1] * d[2]);
        i[5] += sign * m * (dd - d[2] * d[2]);
    };
    let g = body.center_of_gravity;
    let sub = |a: V3, b: V3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let mut inertia = body.inertia;
    shift(sub(from, g), -1.0, &mut inertia);
    shift(sub(body.reference, g), 1.0, &mut inertia);
    body.inertia = inertia;
}

/// Whether every range lies inside `range`.
pub(super) fn explains_within(range: [i64; 2], ranges: &[[i64; 2]]) -> bool {
    ranges.iter().all(|r| r[0] >= range[0] && r[1] <= range[1])
}

pub fn evenly_spaced(values: &[f64]) -> bool {
    if values.len() < 3 {
        return true;
    }
    let step = (values[values.len() - 1] - values[0]) / (values.len() - 1) as f64;
    values.iter().enumerate().all(|(i, v)| (v - (values[0] + step * i as f64)).abs() <= 1e-9 * step.abs().max(1.0))
}

pub(super) fn same_values(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1e-9 * x.abs().max(1.0))
}

/// The values along one axis of a point list as start, end and count of an even spacing (the
/// range spans them when they are not evenly spaced).
pub fn axis_of(mut values: Vec<f64>) -> (f64, f64, i64) {
    values.sort_by(f64::total_cmp);
    let scale = 1f64.max(values[0].abs()).max(values[values.len() - 1].abs());
    values.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * scale);
    let (start, end) = (values[0], values[values.len() - 1]);
    if values.len() < 2 {
        return (start, end, 1);
    }
    let spacing = values.windows(2).map(|w| w[1] - w[0]).fold(f64::INFINITY, f64::min);
    (start, end, ((end - start) / spacing).round() as i64 + 1)
}

/// Whether two point lists hold the same points with the same tanks, in any order.
fn same_listed(a: &[crate::project::ListedPoint], b: &[crate::project::ListedPoint]) -> bool {
    let close = |p: &V3, q: &V3| (0..3).all(|i| (p[i] - q[i]).abs() <= 1e-9 * p[i].abs().max(1.0));
    let mut used = vec![false; b.len()];
    a.len() == b.len()
        && a.iter().all(|x| {
            let found = (0..b.len()).find(|&k| !used[k] && b[k].tank == x.tank && close(&b[k].point, &x.point));
            found.map(|k| used[k] = true).is_some()
        })
}

fn same_points(a: &[V3], b: &[V3]) -> bool {
    let len = |p: V3| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    a.len() == b.len()
        && a.iter().zip(b).all(|(p, q)| len([p[0] - q[0], p[1] - q[1], p[2] - q[2]]) <= 1e-9 * len(*p).max(1.0))
}

fn restore_surface_meshes(r: &mut Reader, body: &mut Body, saved: &SavedBody) {
    for (s, name) in body.dipole_dampers.iter_mut().zip(&saved.damper_meshes) {
        s.mesh = r.optional_mesh(name);
    }
    if let Some(c) = &mut body.control_surface {
        c.mesh = r.optional_mesh(&saved.control_mesh);
    }
    if body.interior.mesh.is_none() {
        body.interior.mesh = r.optional_mesh(&saved.interior_mesh);
    }
    for (s, name) in body.dipoles.iter_mut().zip(&saved.dipole_meshes) {
        s.mesh = r.optional_mesh(name);
    }
    for (s, name) in body.pressure_surfaces.iter_mut().zip(&saved.pressure_meshes) {
        s.mesh = r.optional_mesh(name);
    }
    for (s, name) in body.free_surface_plates.iter_mut().zip(&saved.plate_meshes) {
        s.mesh = r.optional_mesh(name);
    }
    for (t, names) in body.tanks.iter_mut().zip(&saved.tank_plate_meshes) {
        for (s, name) in t.free_surface_plates.iter_mut().zip(names) {
            if s.mesh.is_none() {
                s.mesh = r.optional_mesh(name);
            }
        }
    }
}

/// The name of the mesh of a tank's free-surface plate `j` (of tank `k`) that has none in the library.
pub(super) fn tank_plate_mesh(body: &Body, tank: &Tank, k: usize, j: usize) -> String {
    format!("{}_{}_plate_{}", body.name, tank.engine_name(k), j + 1)
}

/// The project of an engine model. Returns the warnings: inputs the GUI had to adapt. An error
/// names an input the GUI cannot represent.
pub fn read_model(m: &Model) -> Result<(Project, Vec<String>), String> {
    let mut warnings = Vec::new();
    let mut r =
        Reader { m, meshes: HashMap::new(), analytic: HashMap::new(), reports: HashMap::new(), placed: HashMap::new() };
    let root = m.root();

    // The GUI state; a newer layout or an unreadable one is ignored (the problem still opens).
    let mut state: Option<GuiState> = None;
    if let Ok(tool) = m.object(&format!("tool/{GUI_TOOL}")) {
        match serde_json::from_str::<GuiState>(&r.text(tool, "text")?) {
            Ok(s) if s.version > GUI_TOOL_VERSION => {
                warnings.push("the GUI state was written by a newer HydroBEM GUI and is ignored".into())
            }
            Ok(s) => state = Some(s),
            Err(_) => warnings.push("the GUI state cannot be read and is ignored".into()),
        }
    }
    let gui = state.unwrap_or_default();
    r.analytic = gui.analytic_meshes.iter().cloned().collect();
    r.reports = gui.patch_reports.iter().cloned().collect();

    let mut project = Project::default();
    let e = m.object("environment").map_err(engine)?;
    let env = &mut project.environment;
    if let Some(depth) = gui.finite_depth {
        env.depth = depth;
    }
    let depth = r.real(e, "depth")?;
    env.depth_type = if depth.is_infinite() { DepthType::Infinite } else { DepthType::Finite };
    if depth.is_finite() {
        env.depth = depth;
    }
    env.gravity = r.real(e, "gravity")?;
    env.density = r.real(e, "density")?;
    env.channel_reflect = r.real(e, "channel_reflect")?;
    env.domain = Domain::from_engine(&r.text(e, "domain")?).unwrap_or_default();
    if r.is_set(e, "channel_width")? {
        env.channel_width = r.real(e, "channel_width")?;
    }

    let o = m.object("options").map_err(engine)?;
    project.options.logarithmic_integration = r.boolean(o, "logarithmic_integration")?;
    project.options.log_tolerance = r.real(o, "log_tolerance")?;
    project.options.frequency_threads = r.int(o, "frequency_threads")?;
    project.options.inner_threads = r.int(o, "inner_threads")?;

    let out = m.object("first_order").map_err(engine)?;
    project.first_order.radiation = r.boolean(out, "radiation")?;
    project.first_order.diffraction = r.boolean(out, "diffraction")?;
    project.first_order.diffraction_formulation =
        Formulation::from_engine(&r.text(out, "diffraction_formulation")?).unwrap_or_default();
    project.first_order.motion = r.boolean(out, "motion")?;
    project.first_order.field_velocity = r.boolean(out, "field_velocity")?;
    project.first_order.body_pressure = r.boolean(out, "body_pressure")?;
    project.first_order.body_velocity = r.boolean(out, "body_velocity")?;
    let second = m.object("second_order").map_err(engine)?;
    project.second_order.drift_pressure_integration = r.boolean(second, "drift_pressure_integration")?;
    project.second_order.drift_control_surface = r.boolean(second, "drift_control_surface")?;
    project.second_order.drift_momentum_conservation = r.boolean(second, "drift_momentum_conservation")?;
    project.second_order.drift_bidirectional = r.boolean(second, "drift_bidirectional")?;
    project.second_order.drift_momentum_conservation_angles =
        match r.get(second, "drift_momentum_conservation_angles")? {
            Some(Value::Int(n)) => Some(n),
            _ => None,
        };
    read_qtf(&mut r, second, &mut project.second_order)?;

    let f = m.object("environment").map_err(engine)?;
    let periods = r.text(f, "frequency_input")? == "periods";
    let all = r.reals(f, if periods { "periods" } else { "omegas" })?;
    // the limits apart: omega = 0 (a period of inf) and omega = inf (a period of 0)
    let (zero, inf) = if periods { (f64::INFINITY, 0.0) } else { (0.0, f64::INFINITY) };
    let zero_limit = all.contains(&zero);
    let infinite_limit = all.contains(&inf);
    let values: Vec<f64> = all.iter().copied().filter(|v| *v != zero && *v != inf).collect();
    if values.iter().any(|v| v.is_nan() || *v <= 0.0) {
        return Err(unsupported(if periods { "environment/periods" } else { "environment/omegas" }, "negative values"));
    }
    let waves = &mut project.waves.frequencies;
    *waves = WaveFrequencies {
        input_type: if periods { WaveInputType::Period } else { WaveInputType::Frequency },
        count: values.len() as i64,
        inputs: values.clone(),
        ..WaveFrequencies::default()
    };
    if let (Some(first), Some(last)) = (values.first(), values.last()) {
        (waves.start, waves.end) = (*first, *last);
        waves.edit_in_table = !evenly_spaced(&values); // even rows are edited as start, end and count
    }
    // The generator as last edited, when it still describes these rows.
    if let Some(w) = &gui.wave_frequencies
        && w.count == waves.count
    {
        (waves.start, waves.end, waves.edit_in_table) = (w.start, w.end, w.edit_in_table);
        if !waves.edit_in_table && !evenly_spaced(&values) {
            waves.edit_in_table = true;
        }
    }
    (waves.zero_limit, waves.infinite_limit) = (zero_limit, infinite_limit);
    let headings = r.reals(f, "headings")?;
    // the GUI's headings when the model has the same, or none (written only with diffraction)
    if let Some(h) = &gui.headings
        && (headings.is_empty() || same_values(&h.values(), &headings))
    {
        project.waves.headings = h.clone();
    } else if !headings.is_empty() && !same_values(&project.waves.headings.values(), &headings) {
        project.waves.headings =
            WaveHeadings { start: headings[0], end: headings[headings.len() - 1], count: headings.len() as i64 };
        if !evenly_spaced(&headings) {
            warnings.push(format!(
                "headings are not evenly spaced; the GUI uses {} evenly spaced headings from the first to the last",
                headings.len()
            ));
        }
    }

    // Field points: the grid of the GUI state, else one rebuilt from the points.
    let mut fixed_dampers = Vec::new();
    for d in r.children(root, "fixed_dampers")? {
        let dat = r.path(d);
        let mesh = match r.mesh_ref(d, "mesh", "ranges")? {
            Some((name, ranges)) if ranges.is_empty() => Some(r.mesh(&name).map_err(|e| format!("{dat}: {e}"))?),
            Some(_) => return Err(unsupported(&format!("{dat}/mesh"), "record ranges of a mesh")),
            None => None,
        };
        fixed_dampers.push(PanelSurface {
            enabled: r.enabled(d)?,
            source: SurfaceSource::ExternalMesh,
            mesh,
            damping_factor: r.real(d, "damping_factor")?,
            ..PanelSurface::default()
        });
    }
    // the points in the sea (global), then those of each tank (its body's axes, placed below
    // once the bodies are read)
    let fp = m.object("first_order").map_err(engine)?;
    let mut file_points: Vec<V3> = r.reals(fp, "field_points")?.as_chunks::<3>().0.to_vec();
    let mut file_tanks: Vec<String> = vec![String::new(); file_points.len()];
    let mut tank_points: Vec<(String, String, Vec<V3>)> = Vec::new();
    for b in r.children(root, "bodies")? {
        let body = m.object_info(b).map_err(engine)?.name;
        for t in r.children(b, "tanks")? {
            let local = r.reals(t, "field_points")?;
            if !local.is_empty() {
                let tank = m.object_info(t).map_err(engine)?.name;
                tank_points.push((body.clone(), tank, local.as_chunks::<3>().0.to_vec()));
            }
        }
    }
    let local_tank_points: usize = tank_points.iter().map(|(_, _, p)| p.len()).sum();
    if let Some(grid) = &gui.field_grid {
        project.field_points = grid.clone();
    }
    project.field_points.enabled = false;
    if !file_points.is_empty() || local_tank_points > 0 {
        // written only with field points: without them the GUI's default (on) stays
        project.first_order.field_pressure = r.boolean(out, "field_pressure")?;
        // the depths in their order; every depth the same grid
        let mut levels: Vec<f64> = Vec::new();
        for p in &file_points {
            if !levels.iter().any(|z| (p[2] - z).abs() <= 1e-9 * p[2].abs().max(1.0)) {
                levels.push(p[2]);
            }
        }
        let at = |z: f64| -> Vec<[f64; 2]> {
            file_points.iter().filter(|p| (p[2] - z).abs() <= 1e-9 * z.abs().max(1.0)).map(|p| [p[0], p[1]]).collect()
        };
        let first = levels.first().map(|z| at(*z)).unwrap_or_default();
        let same_grids = levels.iter().all(|z| at(*z) == first);
        let grid = &mut project.field_points;
        grid.enabled = true;
        // a grid of the points in the sea alone (the tanks' points are kept as a list, below)
        if gui.field_grid.is_none() && same_grids && local_tank_points == 0 && !file_points.is_empty() {
            // a Cartesian grid by the spacing of the points along x and y, only when it can be
            // the grid they came from: not far more nodes than points (the engine leaves out only
            // the points inside bodies). Points of rings or of a body-fitted grid would give one
            // of their smallest spacing, of up to 1e18 nodes: they are kept as a list (below).
            let x = axis_of(file_points.iter().map(|p| p[0]).collect());
            let y = axis_of(file_points.iter().map(|p| p[1]).collect());
            if (x.2 as f64) * (y.2 as f64) <= 4.0 * first.len() as f64 + 16.0 {
                grid.z = levels[0];
                grid.extra_levels = levels[1..].to_vec();
                (grid.x_start, grid.x_end, grid.x_count) = x;
                (grid.y_start, grid.y_end, grid.y_count) = y;
            }
        }
    }

    // The mesh library: every mesh object of the model, in its order.
    for id in r.children(root, "meshes")? {
        let name = m.object_info(id).map_err(engine)?.name;
        let mesh = r.mesh(&name)?;
        project.meshes.push(crate::project::MeshEntry { name, mesh });
    }

    // Bodies and lids: the problem's, in the order of the GUI state when there is one, the
    // suppressed bodies and inactive lids from the GUI state alone.
    let (density, gravity) = (project.environment.density, project.environment.gravity);
    // the records placed lids take leave the wetted surface of their body
    for l in r.children(root, "lids")? {
        if let Some(Value::Text(body)) = r.get(l, "placed_with")?
            && let Some((_, ranges)) = r.mesh_ref(l, "mesh", "ranges")?
        {
            r.placed.entry(body).or_default().extend(ranges);
        }
    }
    let engine_bodies = r.children(root, "bodies")?;
    let mut used = vec![false; engine_bodies.len()];
    let name_of = |id: ObjectId| m.object_info(id).map(|i| i.name).unwrap_or_default();
    for saved in &gui.bodies {
        if let Some(i) =
            engine_bodies.iter().enumerate().position(|(i, id)| !used[i] && name_of(*id) == saved.body.name)
        {
            used[i] = true;
            let mut base = saved.body.clone();
            restore_surface_meshes(&mut r, &mut base, saved);
            project.bodies.push(r.body(engine_bodies[i], base, &mut warnings, density, gravity)?);
        } else if saved.body.suppressed {
            // a suppressed body the problem could not hold
            let mut body = saved.body.clone();
            body.mesh = r.optional_mesh(&saved.mesh);
            restore_surface_meshes(&mut r, &mut body, saved);
            body.update_mesh_properties(density, gravity);
            project.bodies.push(body);
        }
    }
    for (i, id) in engine_bodies.iter().enumerate() {
        if !used[i] {
            project.bodies.push(r.body(*id, Body::default(), &mut warnings, density, gravity)?);
        }
    }

    let engine_lids = r.children(root, "lids")?;
    let mut used = vec![false; engine_lids.len()];
    for saved in &gui.lids {
        let position = engine_lids.iter().enumerate().position(|(i, id)| !used[i] && name_of(*id) == saved.lid.name);
        match position {
            Some(i) => {
                used[i] = true;
                project.lids.push(r.lid(engine_lids[i], saved.lid.clone())?);
            }
            _ => {
                let mut lid = saved.lid.clone();
                lid.mesh = r.optional_mesh(&saved.mesh);
                project.lids.push(lid);
            }
        }
    }
    for (i, id) in engine_lids.iter().enumerate() {
        if !used[i] {
            project.lids.push(r.lid(*id, DampingLid::default())?);
        }
    }
    let mut saved_dampers: Vec<PanelSurface> = gui.fixed_dampers.iter().map(|w| w.surface.clone()).collect();
    for (s, w) in saved_dampers.iter_mut().zip(&gui.fixed_dampers) {
        s.mesh = r.optional_mesh(&w.mesh);
    }
    project.fixed_dampers = merge_surfaces(saved_dampers, fixed_dampers);
    project.renumber();

    // The tanks of the field points, "<body>/<tank>", by their body and place.
    let resolve = |name: &str| -> Result<Option<TankRef>, String> {
        if name.is_empty() {
            return Ok(None);
        }
        let found = name.split_once('/').and_then(|(body, tank)| {
            let b = project.bodies.iter().find(|b| b.name == body)?;
            let k = b.tanks.iter().enumerate().position(|(k, t)| t.engine_name(k) == tank)?;
            Some(TankRef { body: body.to_string(), tank: k + 1 })
        });
        found.map(Some).ok_or_else(|| format!("field points: no tank \"{name}\""))
    };
    // the points of the tanks in global coordinates, after those of the sea
    for (body, tank, local) in &tank_points {
        let b = project.bodies.iter().find(|b| &b.name == body);
        for p in local {
            file_points.push(b.map_or(*p, |b| b.to_global(*p)));
            file_tanks.push(format!("{body}/{tank}"));
        }
    }
    // one tank for every point: the grid's
    let tank = file_tanks.first().cloned().filter(|t| file_tanks.iter().all(|x| x == t)).unwrap_or_default();
    if gui.field_grid.as_ref().is_none_or(|g| g.list.is_none()) {
        project.field_points.list = None;
        project.field_points.tank = resolve(&tank)?;
    }
    // Points a grid does not give (different grids per depth, points in different tanks, points
    // the grid would not submit) are kept as a list, one by one.
    if !file_points.is_empty() {
        let mut list = Vec::new();
        for (k, p) in file_points.iter().enumerate() {
            list.push(crate::project::ListedPoint {
                point: *p,
                tank: resolve(file_tanks.get(k).map_or("", String::as_str))?,
            });
        }
        // the GUI's list in its order when it holds the model's points (the model holds those in
        // the sea first, then each tank's)
        let saved = project.field_points.list.as_ref().filter(|saved| same_listed(saved, &list)).is_some();
        let submitted = crate::field_points::select_field_points(&project).points;
        if !saved && (project.field_points.list.is_some() || !same_points(&submitted, &file_points)) {
            project.field_points.list = Some(list);
        }
    }

    Ok((project, warnings))
}
