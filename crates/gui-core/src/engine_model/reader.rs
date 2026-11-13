//! The reader of an engine model: its items and objects, a body with its surfaces, a lid, and
//! the QTFs.

use std::collections::HashMap;
use std::sync::Arc;

use hydrobem_engine::{Error, Model, ObjectId, Value};

use super::read::{explains_within, merge_surfaces, move_inertia};
use super::write::{remaining, surface_ranges};
use crate::mesh::Mesh;
use crate::project::{
    Body, ControlMethod, ControlSurface, Coupling, DampingLid, GeneralizedMode, InteriorMethod, MODE_NAMES,
    MassCalculation, Matrix6, MatrixOrigin, ModeMatrices, ModePiece, PanelSurface, PressureMode, PressureShape,
    SurfaceSource, Tank, TankSheet, V3,
};

pub(super) fn unsupported(at: &str, what: &str) -> String {
    format!("{at}: {what} (not supported by the GUI)")
}

pub(super) fn engine(e: Error) -> String {
    e.message().to_string()
}

/// A mesh reference: the mesh name and its record ranges (none: every record).
type MeshRef = (String, Vec<[i64; 2]>);

pub(super) struct Reader<'a> {
    pub(super) m: &'a Model,
    pub(super) meshes: HashMap<String, Arc<Mesh>>,
    /// The analytic shapes of the GUI state, by mesh name.
    pub(super) analytic: HashMap<String, crate::analytic::AnalyticSpec>,
    /// cad2bem's reports of the GUI state, by mesh name.
    pub(super) reports: HashMap<String, crate::patch_report::PatchReport>,
    /// Per body name, the record ranges of its mesh that lids placed with it take.
    pub(super) placed: HashMap<String, Vec<[i64; 2]>>,
}

impl Reader<'_> {
    pub(super) fn get(&self, id: ObjectId, item: &str) -> Result<Option<Value>, String> {
        self.m.get(id, item).map_err(engine)
    }

    pub(super) fn is_set(&self, id: ObjectId, item: &str) -> Result<bool, String> {
        Ok(self.m.item_info(id, item).map_err(engine)?.is_set)
    }

    fn integer(&self, id: ObjectId, item: &str) -> Result<i64, String> {
        Ok(match self.get(id, item)? {
            Some(Value::Int(n)) => n,
            Some(v) => v.as_real().unwrap_or(0.0) as i64,
            None => 0,
        })
    }

    pub(super) fn real(&self, id: ObjectId, item: &str) -> Result<f64, String> {
        Ok(self.get(id, item)?.and_then(|v| v.as_real()).unwrap_or(0.0))
    }

    /// The item `enabled` (true when unset).
    pub(super) fn enabled(&self, id: ObjectId) -> Result<bool, String> {
        Ok(self.get(id, "enabled")?.and_then(|v| v.as_bool()).unwrap_or(true))
    }

    pub(super) fn boolean(&self, id: ObjectId, item: &str) -> Result<bool, String> {
        Ok(self.get(id, item)?.and_then(|v| v.as_bool()).unwrap_or(false))
    }

    pub(super) fn int(&self, id: ObjectId, item: &str) -> Result<i64, String> {
        Ok(self.get(id, item)?.and_then(|v| v.as_int()).unwrap_or(0))
    }

    pub(super) fn text(&self, id: ObjectId, item: &str) -> Result<String, String> {
        Ok(self.get(id, item)?.and_then(|v| v.as_text().map(str::to_string)).unwrap_or_default())
    }

    pub(super) fn reals(&self, id: ObjectId, item: &str) -> Result<Vec<f64>, String> {
        Ok(self.get(id, item)?.and_then(|v| v.as_reals().map(<[f64]>::to_vec)).unwrap_or_default())
    }

    fn vec3(&self, id: ObjectId, item: &str) -> Result<V3, String> {
        let v = self.reals(id, item)?;
        Ok(if v.len() == 3 { [v[0], v[1], v[2]] } else { [0.0; 3] })
    }

    pub(super) fn path(&self, id: ObjectId) -> String {
        self.m.path(id).unwrap_or_default()
    }

    pub(super) fn children(&self, id: ObjectId, list: &str) -> Result<Vec<ObjectId>, String> {
        Ok(self.m.children(id, list).map_err(engine)?.to_vec())
    }

    /// A surface: the mesh its mesh item names and the record ranges of its ranges item (none:
    /// every record).
    pub(super) fn mesh_ref(&self, id: ObjectId, mesh_item: &str, ranges_item: &str) -> Result<Option<MeshRef>, String> {
        let name = match self.get(id, mesh_item)? {
            Some(Value::Text(name)) if !name.is_empty() => name,
            _ => return Ok(None),
        };
        let ranges = if self.is_set(id, ranges_item)? {
            self.get(id, ranges_item)?.and_then(|v| v.as_ranges()).unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(Some((name, ranges)))
    }

    pub(super) fn mesh(&mut self, name: &str) -> Result<Arc<Mesh>, String> {
        if let Some(mesh) = self.meshes.get(name) {
            return Ok(mesh.clone());
        }
        let id = self.m.object(&format!("meshes/{name}")).map_err(|_| format!("unknown mesh \"{name}\""))?;
        let mut mesh = Mesh::from_object(self.m, id).map_err(|e| format!("meshes/{name}: {e}"))?;
        if mesh.is_patch() {
            mesh.rows_from = crate::mesh::RowsFrom::Project;
        }
        // cad2bem's report as saved while it still names the mesh's patches; else (a model without
        // the GUI state) the report beside the geometry file the mesh was read from
        match self.reports.get(name).filter(|r| r.matches(&mesh.patch_names())) {
            Some(report) => mesh.report = Some(Arc::new(report.clone())),
            None => mesh = mesh.with_report(),
        }
        // an analytic mesh of the GUI stays editable while its object is still that shape's
        if let Some(spec) = self.analytic.get(name)
            && mesh.patch.as_ref().is_some_and(|p| p.kind == "analytic_patches")
            && mesh.panels.len() == spec.patch_count()
        {
            mesh.analytic = Some(spec.clone());
        }
        let mesh = Arc::new(mesh);
        self.meshes.insert(name.to_string(), mesh.clone());
        Ok(mesh)
    }

    pub(super) fn optional_mesh(&mut self, name: &Option<String>) -> Option<Arc<Mesh>> {
        name.as_ref().and_then(|n| self.mesh(n).ok())
    }

    /// A body from its object, over `base` (its GUI state, or the defaults).
    pub(super) fn body(
        &mut self,
        b: ObjectId,
        base: Body,
        warnings: &mut Vec<String>,
        density: f64,
        gravity: f64,
    ) -> Result<Body, String> {
        let at = self.path(b);

        let mut body = base;
        body.name = self.m.object_info(b).map_err(engine)?.name;
        body.placed_lids = self.placed.get(&body.name).cloned().unwrap_or_default();
        body.suppressed = !self.enabled(b)?;
        body.haskind = self.text(b, "excitation_method")? == "haskind";
        body.trim = if self.is_set(b, "trim")? { Some(self.vec3(b, "trim")?) } else { None };
        body.body_point_neighbors = self.integer(b, "body_point_neighbors")?;
        body.body_points = if self.is_set(b, "body_points")? {
            Some(self.reals(b, "body_points")?.as_chunks::<3>().0.to_vec())
        } else {
            None
        };
        body.position = self.vec3(b, "position")?;
        body.yaw = self.real(b, "yaw")?;
        body.reference = self.vec3(b, "rotation_center")?;
        let modes: Vec<String> = self
            .get(b, "modes")?
            .and_then(|v| if let Value::Texts(t) = v { Some(t) } else { None })
            .unwrap_or_default();
        body.modes = std::array::from_fn(|i| modes.iter().any(|m| m == MODE_NAMES[i]));

        let (mesh_name, body_ranges) = match self.mesh_ref(b, "mesh", "ranges")? {
            Some((name, ranges)) => (Some(name), ranges),
            None => (None, Vec::new()),
        };
        body.mesh = match &mesh_name {
            Some(name) => Some(self.mesh(name).map_err(|e| format!("{at}: {e}"))?),
            None => None,
        };

        // The mass: Displaced, or a value (Given); none opens as Displaced.
        match self.get(b, "mass")? {
            Some(Value::Real(mass)) => {
                body.mass_calculation = MassCalculation::Manual;
                body.mass = mass;
            }
            Some(Value::Text(t)) if t == "displaced" => body.mass_calculation = MassCalculation::FromMesh,
            _ => {
                body.mass_calculation = MassCalculation::FromMesh;
                warnings.push(format!("{at}: no mass is given; the body opens with the displaced mass (Displaced)"));
            }
        }
        if self.is_set(b, "center_of_gravity")? {
            body.center_of_gravity = self.vec3(b, "center_of_gravity")?;
        }
        // the side of the inertia the switch inertia_input chooses (the other is unused)
        let components = self.text(b, "inertia_input")? == "components";
        let mut has_inertia = false;
        for (i, item) in ["Ixx", "Ixy", "Ixz", "Iyy", "Iyz", "Izz"].iter().enumerate() {
            body.inertia[i] = if components { self.real(b, item)? } else { 0.0 };
            has_inertia |= body.inertia[i] != 0.0;
        }
        for (item, origin, matrix, about) in [
            ("additional_stiffness", "stiffness_origin", &mut body.additional_stiffness, &mut body.stiffness_origin),
            ("additional_damping", "damping_origin", &mut body.additional_damping, &mut body.damping_origin),
        ] {
            *about = match self.get(b, origin)? {
                Some(Value::Text(t)) if t == "center_of_gravity" => MatrixOrigin::CenterOfGravity,
                Some(Value::Reals { data, .. }) if data.len() == 3 => MatrixOrigin::Point([data[0], data[1], data[2]]),
                _ => MatrixOrigin::RotationCenter,
            };
            if self.is_set(b, item)? {
                let info = self.m.item_info(b, item).map_err(engine)?;
                if info.shape != [6, 6] {
                    return Err(unsupported(&format!("{at}/{item}"), "matrices other than 6 x 6"));
                }
                let edit_in_table = matrix.as_ref().is_some_and(|m| m.edit_in_table);
                *matrix = Some(Matrix6 { values: self.reals(b, item)?, edit_in_table });
            } else if matrix.as_ref().is_some_and(|m| !m.is_zero()) {
                // the GUI state names a matrix the problem does not have: the problem decides
                *matrix = None;
            }
        }

        // The interior surface: generated, detected, a range of the body's mesh or a mesh.
        let interior = self.text(b, "interior_surface")?;
        let iat = format!("{at}/interior_surface_mesh");
        body.interior.method = match interior.as_str() {
            "none" => InteriorMethod::None,
            "mesh" => match self.mesh_ref(b, "interior_surface_mesh", "interior_surface_ranges")? {
                Some((name, ranges)) if Some(&name) != mesh_name.as_ref() => {
                    if !ranges.is_empty() {
                        return Err(unsupported(&iat, "record ranges of another mesh"));
                    }
                    body.interior.mesh = Some(self.mesh(&name).map_err(|e| format!("{iat}: {e}"))?);
                    InteriorMethod::Mesh
                }
                Some((_, r)) if !r.is_empty() => {
                    (body.interior.first_panel, body.interior.last_panel) = (r[0][0], r[0][1]);
                    body.interior.more_ranges = r[1..].to_vec();
                    InteriorMethod::PanelRange
                }
                Some(_) => return Err(unsupported(&iat, "every record of the body mesh")),
                None => return Err(format!("{at}: interior_surface = 'mesh' needs interior_surface_mesh")),
            },
            method => InteriorMethod::from_engine_method(method).ok_or_else(|| {
                unsupported(&format!("{at}/interior_surface"), &format!("interior method \"{method}\""))
            })?,
        };

        // Dipoles and pressure surfaces: ranges of the body's mesh or meshes of their own.
        let mut dipoles = Vec::new();
        for d in self.children(b, "dipoles")? {
            let dat = self.path(d);
            let mut surface = PanelSurface::default();
            self.surface(d, &mesh_name, &dat, &mut surface)?;
            dipoles.push(surface);
        }
        body.dipoles = merge_surfaces(std::mem::take(&mut body.dipoles), dipoles);
        let mut pressures = Vec::new();
        for s in self.children(b, "pressure_surfaces")? {
            let sat = self.path(s);
            let mut surface = PanelSurface { pressure_modes: Vec::new(), ..PanelSurface::default() };
            for mode in self.children(s, "pressure_modes")? {
                let shape = self.text(mode, "shape")?;
                surface.pressure_modes.push(PressureMode {
                    shape: PressureShape::ALL.into_iter().find(|x| x.engine_name() == shape).ok_or_else(|| {
                        unsupported(&format!("{}/shape", self.path(mode)), &format!("shape \"{shape}\""))
                    })?,
                    stiffness: self.real(mode, "additional_stiffness")?,
                    damping: self.real(mode, "additional_damping")?,
                });
            }
            surface.chamber_coupling = self.boolean(s, "chamber_coupling")?;
            surface.numeric_convention = self.text(s, "response_convention")? == "numeric_compatibility";
            self.surface(s, &mesh_name, &sat, &mut surface)?;
            pressures.push(surface);
        }
        body.pressure_surfaces = merge_surfaces(std::mem::take(&mut body.pressure_surfaces), pressures);

        // Free-surface plates: ranges of the body's mesh or meshes of their own.
        let mut plates = Vec::new();
        for p in self.children(b, "free_surface_plates")? {
            let pat = self.path(p);
            let mut surface = PanelSurface::default();
            if self.is_set(p, "inside")? {
                // a plate the engine generates inside a trimmed hull's waterline loop
                let inside = self.reals(p, "inside")?;
                if inside.len() != 2 {
                    return Err(format!("{pat}/inside: not a point (x, y)"));
                }
                surface.source = SurfaceSource::Generated;
                surface.inside = [inside[0], inside[1]];
                surface.body = if self.is_set(p, "body")? { Some(self.text(p, "body")?) } else { None };
                surface.enabled = self.enabled(p)?;
            } else {
                self.surface(p, &mesh_name, &pat, &mut surface)?;
            }
            plates.push(surface);
        }
        body.free_surface_plates = merge_surfaces(std::mem::take(&mut body.free_surface_plates), plates);

        // Generalized modes: polynomial pieces on records of the body's mesh; the mode matrices.
        body.generalized_modes.clear();
        for g in self.children(b, "generalized_modes")? {
            let gat = self.path(g);
            let parity = self.text(g, "parity")?;
            let mut mode = GeneralizedMode {
                name: self.m.object_info(g).map_err(engine)?.name,
                parity: MODE_NAMES.iter().position(|n| *n == parity).unwrap_or(0),
                pieces: Vec::new(),
                view_scale: 0.0,
            };
            for p in self.children(g, "pieces")? {
                let pat = self.path(p);
                let mut piece = ModePiece::default();
                for (k, item) in ["u", "v", "w"].iter().enumerate() {
                    if self.is_set(p, item)? {
                        piece.terms[k] = self.reals(p, item)?.as_chunks::<4>().0.to_vec();
                    }
                }
                // the records of the body's mesh it covers (none: every wetted record)
                let ranges = if self.is_set(p, "ranges")? {
                    self.get(p, "ranges")?.and_then(|v| v.as_ranges()).unwrap_or_default()
                } else {
                    Vec::new()
                };
                if !ranges.is_empty() && mesh_name.is_none() {
                    return Err(unsupported(&format!("{pat}/ranges"), "records of a body without a mesh"));
                }
                piece.range = ranges.first().copied();
                piece.more_ranges = ranges.get(1..).map(<[[i64; 2]]>::to_vec).unwrap_or_default();
                mode.pieces.push(piece);
            }
            if mode.pieces.is_empty() {
                return Err(format!("{gat}: a generalized mode needs a piece"));
            }
            body.generalized_modes.push(mode);
        }
        let n = 6 + body.generalized_modes.len();
        let mut matrices = ModeMatrices::default();
        matrices.resize(n);
        let mut any = false;
        for (item, target) in [
            ("mode_mass", &mut matrices.mass),
            ("mode_damping", &mut matrices.damping),
            ("mode_stiffness", &mut matrices.stiffness),
        ] {
            if self.is_set(b, item)? {
                let values = self.reals(b, item)?;
                if values.len() != n * n {
                    return Err(format!("{at}/{item}: {n} x {n} values (6 + the generalized modes)"));
                }
                *target = values;
                any = true;
            }
        }
        let shown = body.mode_matrices.as_ref().map_or(0, |m| m.shown);
        body.mode_matrices = any.then_some(ModeMatrices { shown, ..matrices });

        // Dipole dampers: as dipoles, with their damping factor.
        let mut dampers = Vec::new();
        for d in self.children(b, "dipole_dampers")? {
            let dat = self.path(d);
            let mut surface = PanelSurface::default();
            self.surface(d, &mesh_name, &dat, &mut surface)?;
            surface.damping_factor = self.real(d, "damping_factor")?;
            dampers.push(surface);
        }
        body.dipole_dampers = merge_surfaces(std::mem::take(&mut body.dipole_dampers), dampers);

        // Tanks: a record range of the body's mesh each; their own sheets are not represented.
        let mut tanks = Vec::new();
        for (k, t) in self.children(b, "tanks")?.into_iter().enumerate() {
            let tat = self.path(t);
            let mut free_surface_plates = Vec::new();
            for p in self.children(t, "free_surface_plates")? {
                let pat = self.path(p);
                let mut surface = PanelSurface::default();
                self.surface(p, &mesh_name, &pat, &mut surface)?;
                free_surface_plates.push(surface);
            }
            let mut sheets = Vec::new();
            for (list, damper) in [("dipoles", false), ("dipole_dampers", true)] {
                for s in self.children(t, list)? {
                    let sat = self.path(s);
                    let r = self.body_range(s, &mesh_name, &sat)?;
                    let damping_factor = if damper { self.real(s, "damping_factor")? } else { 0.0 };
                    sheets.push(TankSheet {
                        first_panel: r[0][0],
                        last_panel: r[0][1],
                        more_ranges: r[1..].to_vec(),
                        damper,
                        damping_factor,
                    });
                }
            }
            let r = self.body_range(t, &mesh_name, &tat)?;
            let surface_level = match self.get(t, "surface_level")? {
                Some(Value::Real(z)) => Some(z),
                _ => None,
            };
            let name = self.m.object_info(t).map_err(engine)?.name;
            tanks.push(Tank {
                enabled: self.enabled(t)?,
                // the name the GUI gives an unnamed tank stays unnamed
                name: if name == format!("tank_{}", k + 1) { String::new() } else { name },
                first_panel: r[0][0],
                last_panel: r[0][1],
                more_ranges: r[1..].to_vec(),
                density_ratio: self.real(t, "density_ratio")?,
                surface_level,
                sheets,
                free_surface_plates,
            });
        }
        body.tanks = tanks;

        // Couplings: the 6 x 6 stiffness and damping against another body's motions.
        let mut couplings = Vec::new();
        for c in self.children(b, "couplings")? {
            let mut coupling =
                Coupling { enabled: self.enabled(c)?, body: self.text(c, "body")?, ..Coupling::default() };
            for (item, matrix) in [("stiffness", &mut coupling.stiffness), ("damping", &mut coupling.damping)] {
                if self.is_set(c, item)? {
                    matrix.values = self.reals(c, item)?;
                }
            }
            if let Some(saved) = body.couplings.iter().find(|s| s.body == coupling.body) {
                coupling.shown = saved.shown;
                coupling.stiffness.edit_in_table = saved.stiffness.edit_in_table;
                coupling.damping.edit_in_table = saved.damping.edit_in_table;
            }
            couplings.push(coupling);
        }
        // a coupling to a body the model does not hold (one it left out) is kept from the GUI state
        let root = self.m.root();
        let in_model: Vec<String> = self
            .children(root, "bodies")?
            .into_iter()
            .map(|o| self.m.object_info(o).map(|i| i.name))
            .collect::<hydrobem_engine::Result<_>>()
            .map_err(engine)?;
        couplings.extend(body.couplings.iter().filter(|c| !in_model.contains(&c.body)).cloned());
        body.couplings = couplings;

        // The control surface: generated by a method, or a mesh of the library.
        body.control_surface = None;
        let method = self.text(b, "control_surface")?;
        if method != "none" {
            let (c, cat) = (b, format!("{at}/control_surface"));
            let mut cs = ControlSurface::default();
            if self.text(c, "control_surface_outer")? == "polygon" && self.is_set(c, "control_surface_boundary")? {
                cs.boundary = Some(self.reals(c, "control_surface_boundary")?.as_chunks::<2>().0.to_vec());
            }
            if self.is_set(c, "control_surface_partitions")? {
                let mut line = Vec::new();
                for p in self.reals(c, "control_surface_partitions")?.as_chunks::<2>().0 {
                    if p[0].is_nan() || p[1].is_nan() {
                        cs.partitions.push(std::mem::take(&mut line));
                    } else {
                        line.push(*p);
                    }
                }
                if !line.is_empty() {
                    cs.partitions.push(line);
                }
                cs.partitions.retain(|l| !l.is_empty());
            }
            if method != "mesh" {
                cs.method = ControlMethod::ALL
                    .into_iter()
                    .find(|m| m.engine_name() == Some(method.as_str()))
                    .ok_or_else(|| unsupported(&cat, &format!("method \"{method}\"")))?;
            } else {
                cs.method = ControlMethod::Mesh;
                cs.mesh = match self.mesh_ref(c, "control_surface_mesh", "control_surface_ranges")? {
                    Some((name, ranges)) if ranges.is_empty() => {
                        Some(self.mesh(&name).map_err(|e| format!("{cat}: {e}"))?)
                    }
                    Some(_) => return Err(unsupported(&format!("{cat}/mesh"), "record ranges")),
                    None => None,
                };
            }
            for (item, target) in [
                ("control_surface_radius", &mut cs.radius),
                ("control_surface_depth", &mut cs.depth),
                ("control_surface_inner_radius", &mut cs.inner_radius),
                ("control_surface_panel_size", &mut cs.panel_size),
            ] {
                if self.is_set(c, item)? {
                    *target = self.real(c, item)?;
                }
            }
            if self.is_set(c, "control_surface_semi_axes")? {
                cs.semi_axes = self.vec3(c, "control_surface_semi_axes")?;
            }
            if self.is_set(c, "control_surface_inner_semi_axes")? {
                let v = self.reals(c, "control_surface_inner_semi_axes")?;
                if v.len() == 2 {
                    cs.inner_semi_axes = [v[0], v[1]];
                }
            }
            cs.body_waterline_term = self.text(c, "control_surface_waterline_term")? == "body";
            body.control_surface = Some(cs);
        }

        // The wetted surface is the body's range of its mesh less the records its surfaces take:
        // the range of the GUI state when it explains the records, else the one spanning them.
        let given = body.mesh_range.take();
        body.mesh_gaps.clear();
        if let Some(mesh) = &body.mesh
            && !body_ranges.is_empty()
        {
            let n = mesh.panels.len() as i64;
            let taken = surface_ranges(&body);
            let explains = |range: [i64; 2]| remaining(range, n, &taken) == body_ranges;
            let spanning =
                body_ranges.iter().chain(&taken).fold([i64::MAX, i64::MIN], |r, x| [r[0].min(x[0]), r[1].max(x[1])]);
            let range = match [given.unwrap_or([1, n]), [1, n], spanning].into_iter().find(|r| explains(*r)) {
                Some(range) => range,
                None => {
                    // records inside the range that neither the body nor a surface takes: gaps
                    let range = given.filter(|g| explains_within(*g, &body_ranges)).unwrap_or([1, n]);
                    let mut used = taken.clone();
                    used.extend(body_ranges.iter().copied());
                    body.mesh_gaps = remaining(range, n, &used);
                    let mut all = taken.clone();
                    all.extend(body.mesh_gaps.iter().copied());
                    if remaining(range, n, &all) != body_ranges {
                        return Err(unsupported(
                            &format!("{at}/mesh"),
                            "a wetted surface whose records a surface also takes",
                        ));
                    }
                    range
                }
            };
            body.mesh_range = (range != [1, n]).then_some(range);
        }

        // The displaced volume is derived; a mass from the mesh is recomputed the same way.
        if body.mesh.is_some() {
            body.update_mesh_properties(density, gravity);
        }

        // Radii of gyration: I = m r |r| with the mass in use, the diagonal or a symmetric 3 x 3.
        let radii = if components { Vec::new() } else { self.reals(b, "radii_of_gyration")? };
        if radii.iter().any(|r| *r != 0.0) {
            let r = radii;
            let m = body.mass;
            if m.is_nan() || m <= 0.0 {
                return Err(format!("{at}/radii_of_gyration: the inertia from radii of gyration needs the mass"));
            }
            let i = |x: f64| m * x * x.abs();
            body.inertia = match r.len() {
                3 => [i(r[0]), 0.0, 0.0, i(r[1]), 0.0, i(r[2])],
                9 => [i(r[0]), i(r[1]), i(r[2]), i(r[4]), i(r[5]), i(r[8])],
                _ => return Err(format!("{at}/radii_of_gyration: 3 or 3 x 3 values")),
            };
            has_inertia = true;
            warnings.push(format!("{at}: the radii of gyration open as Ixx ... Izz about the rotation centre"));
        }

        // The GUI keeps the inertia about R; other origins (the engine's default is G) move there.
        if has_inertia {
            let from = match self.get(b, "inertia_origin")? {
                Some(Value::Text(t)) if t == "rotation_center" => None,
                Some(Value::Reals { data, .. }) if data.len() == 3 => Some([data[0], data[1], data[2]]),
                _ => Some(body.center_of_gravity),
            };
            if let Some(from) = from {
                if body.mass.is_nan() || body.mass < 0.0 {
                    return Err(format!("{at}/inertia_origin: moving the inertia to rotation_center needs the mass"));
                }
                move_inertia(&mut body, from);
            }
        }
        Ok(body)
    }

    /// Record ranges of the body's mesh (one or more).
    fn body_range(&self, id: ObjectId, body_mesh: &Option<String>, at: &str) -> Result<Vec<[i64; 2]>, String> {
        match self.mesh_ref(id, "mesh", "ranges")? {
            Some((name, ranges)) if Some(&name) == body_mesh.as_ref() && !ranges.is_empty() => Ok(ranges),
            Some((name, ranges)) if Some(&name) == body_mesh.as_ref() && ranges.is_empty() => {
                Err(unsupported(&format!("{at}/mesh"), "every record of the body mesh"))
            }
            Some(_) => Err(unsupported(&format!("{at}/mesh"), "a surface of another mesh")),
            None => Err(format!("{at}: needs a mesh")),
        }
    }

    fn surface(
        &mut self,
        id: ObjectId,
        body_mesh: &Option<String>,
        at: &str,
        surface: &mut crate::project::PanelSurface,
    ) -> Result<(), String> {
        let Some((name, ranges)) = self.mesh_ref(id, "mesh", "ranges")? else {
            return Err(format!("{at}: needs a mesh"));
        };
        surface.enabled = self.enabled(id)?;
        if Some(&name) == body_mesh.as_ref() {
            if ranges.is_empty() {
                return Err(unsupported(&format!("{at}/mesh"), "every record of the body mesh"));
            }
            surface.source = SurfaceSource::PanelRange;
            (surface.first_panel, surface.last_panel) = (ranges[0][0], ranges[0][1]);
            surface.more_ranges = ranges[1..].to_vec();
        } else {
            if !ranges.is_empty() {
                return Err(unsupported(&format!("{at}/mesh"), "record ranges of an external mesh"));
            }
            surface.source = SurfaceSource::ExternalMesh;
            surface.mesh = Some(self.mesh(&name).map_err(|e| format!("{at}: {e}"))?);
        }
        Ok(())
    }

    pub(super) fn lid(&mut self, l: ObjectId, base: DampingLid) -> Result<DampingLid, String> {
        let at = self.path(l);
        let mut lid = base;
        lid.placed_with = match self.get(l, "placed_with")? {
            Some(Value::Text(t)) if !t.is_empty() => Some(t),
            _ => None,
        };
        lid.suppressed = !self.enabled(l)?;
        if lid.placed_with.is_some() {
            lid.damping_factor = self.real(l, "damping_factor")?;
            lid.name = self.m.object_info(l).map_err(engine)?.name;
            lid.mesh = None;
            (lid.range, lid.more_ranges) = match self.mesh_ref(l, "mesh", "ranges")? {
                Some((_, ranges)) if !ranges.is_empty() => (ranges[0], ranges[1..].to_vec()),
                _ => return Err(unsupported(&format!("{at}/mesh"), "a placed lid of every record")),
            };
            return Ok(lid);
        }
        lid.name = self.m.object_info(l).map_err(engine)?.name;
        lid.damping_factor = self.real(l, "damping_factor")?;
        lid.mesh = match self.mesh_ref(l, "mesh", "ranges")? {
            Some((name, ranges)) if ranges.is_empty() => Some(self.mesh(&name).map_err(|e| format!("{at}: {e}"))?),
            Some(_) => return Err(unsupported(&format!("{at}/mesh"), "a lid of record ranges")),
            None => None,
        };
        Ok(lid)
    }
}

/// The number of an item, or None for its `"auto"` (or no value).
fn auto_real(r: &Reader, id: ObjectId, item: &str) -> Result<Option<f64>, String> {
    Ok(match r.get(id, item)? {
        Some(Value::Real(x)) => Some(x),
        Some(Value::Int(n)) => Some(n as f64),
        _ => None,
    })
}

fn auto_int(r: &Reader, id: ObjectId, item: &str) -> Result<Option<i64>, String> {
    Ok(match r.get(id, item)? {
        Some(Value::Int(n)) => Some(n),
        Some(Value::Real(x)) => Some(x as i64),
        _ => None,
    })
}

/// The QTFs and the free surface of the potential QTF (`second_order`).
pub(super) fn read_qtf(r: &mut Reader, second: ObjectId, s: &mut crate::project::SecondOrder) -> Result<(), String> {
    use crate::project::{FreeSurfaceMethod, NewmanAverage};
    s.qtf_quadratic_pressure_integration = r.boolean(second, "qtf_quadratic_pressure_integration")?;
    s.qtf_quadratic_control_surface = r.boolean(second, "qtf_quadratic_control_surface")?;
    s.qtf_potential_direct = r.boolean(second, "qtf_potential_direct")?;
    s.qtf_potential_indirect = r.boolean(second, "qtf_potential_indirect")?;
    s.qtf_newman = r.boolean(second, "qtf_newman")?;
    s.qtf_bidirectional = r.boolean(second, "qtf_bidirectional")?;
    s.qtf_difference = r.boolean(second, "qtf_difference")?;
    s.qtf_sum = r.boolean(second, "qtf_sum")?;
    let average = r.text(second, "qtf_newman_average")?;
    s.qtf_newman_average = NewmanAverage::ALL.into_iter().find(|a| a.engine_name() == average).unwrap_or_default();
    let method = r.text(second, "free_surface_method")?;
    s.free_surface_method = FreeSurfaceMethod::ALL.into_iter().find(|m| m.engine_name() == method).unwrap_or_default();
    s.free_surface_panel_size = auto_real(r, second, "free_surface_panel_size")?;
    s.free_surface_inner_radius = auto_real(r, second, "free_surface_inner_radius")?;
    s.quadrature_radius_step = auto_real(r, second, "quadrature_radius_step")?;
    s.quadrature_azimuthal_nodes = auto_int(r, second, "quadrature_azimuthal_nodes")?;
    s.outer_circle_segments = auto_int(r, second, "outer_circle_segments")?;
    s.expansion_order = auto_int(r, second, "expansion_order")?;
    s.free_surface_quadrature_points = r.int(second, "free_surface_quadrature_points")?;
    s.quadrature_annuli = r.int(second, "quadrature_annuli")?;
    s.quadrature_radial_nodes = r.int(second, "quadrature_radial_nodes")?;
    s.free_surface_mesh = match r.mesh_ref(second, "free_surface_mesh", "free_surface_ranges")? {
        Some((name, ranges)) if ranges.is_empty() => {
            Some(r.mesh(&name).map_err(|e| format!("second_order/free_surface_mesh: {e}"))?)
        }
        Some(_) => return Err(unsupported("second_order/free_surface_ranges", "records of a free-surface mesh")),
        None => None,
    };
    Ok(())
}
