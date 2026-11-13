//! The details of a body: its matrices, dampers, tanks, control surfaces and surfaces.

use eframe::egui::Ui;
use gui_core::format::format_number;
use gui_core::hydrobem_engine::PanelRole;
use gui_core::project::{
    ControlMethod, ControlSurface, DOF_LABELS, InteriorMethod, MassCalculation, Matrix6, MatrixOrigin, PanelSurface,
    PressureShape, Project, SurfaceSource, Tank,
};

use super::{Derived, DetailsAction, Library, MeshTarget, library, mesh_rows, vec3_text};
use crate::output::{Cell, Column, Output, TableData};
use crate::widgets::{
    check, choice, choice_of, integer, message, number, ranges_row, read_only, section, text, warning,
};

pub(super) fn body(ui: &mut Ui, project: &mut Project, i: usize, derived: &Derived, actions: &mut Vec<DetailsAction>) {
    let (density, gravity) = (project.environment.density, project.environment.gravity);
    let library = library(project);
    let old_name = project.bodies[i].name.clone();
    let others: Vec<String> =
        project.bodies.iter().enumerate().filter(|(k, _)| *k != i).map(|(_, b)| b.name.clone()).collect();
    let b = &mut project.bodies[i];
    section(ui, "Body", |ui| {
        if text(ui, ("body", i), "Name", &mut b.name) && (b.name.is_empty() || others.contains(&b.name)) {
            // names are unique and not empty (the engine's rule)
            b.name = old_name.clone();
        }
        check(ui, "Visibility", &mut b.visible, Some("Controls display in the 3D viewer only."));
        let mut enabled = !b.suppressed;
        if check(
            ui,
            "Enabled",
            &mut enabled,
            Some("Off: excluded from the solver input (the engine's enabled = false)."),
        ) {
            b.suppressed = !enabled;
        }
    });
    section(ui, "Mesh", |ui| {
        let mut changed = mesh_rows(ui, &library, &mut b.mesh, MeshTarget::Body(i), actions);
        if changed {
            b.mesh_range = None;
            actions.push(DetailsAction::Refit);
        }
        if let Some(mesh) = b.mesh.clone() {
            let n = mesh.panels.len() as i64;
            let mut use_range = b.mesh_range.is_some();
            if check(
                ui,
                "Panel Range",
                &mut use_range,
                Some("Off: every stored record of the mesh is the body's surface."),
            ) {
                b.mesh_range = use_range.then_some([1, n]);
                changed = true;
            }
            let mut range = b.mesh_range.unwrap_or([1, n]);
            let edited = integer(ui, ("range", i), "Start Panel", &mut range[0], use_range)
                | integer(ui, ("range", i), "End Panel", &mut range[1], use_range);
            if edited {
                b.mesh_range = Some([range[0].clamp(1, n), range[1].clamp(1, n)]);
                changed = true;
            }
            if ranges_row(ui, ("gaps", i), "Unused Records", &mut b.mesh_gaps, true) {
                changed = true;
            }
            let copies = (mesh.displayed_panels() / mesh.panels.len().max(1)) as i64;
            let wetted: i64 = match gui_core::engine_model::wetted_ranges(b) {
                None => n,
                Some(r) => r.iter().map(|[a, z]| z - a + 1).sum(),
            };
            read_only(ui, "Wetted Panels", &format!("{wetted} stored, {} displayed", wetted * copies));
            read_only(ui, "Reference Length [m]", &format_number(mesh.reference_length));
        }
        if changed {
            b.update_mesh_properties(density, gravity);
        }
    });
    section(ui, "Interior Surface", |ui| {
        // the higher-order method takes the interior surface as patches: those of the body's mesh in
        // z = 0 (Detect) or a range of them, a patch mesh of its own (the engine appends it to the
        // body's records), or the waterline's of a trimmed hull
        let patch = b.mesh.as_ref().is_some_and(|m| m.is_patch());
        let methods: &[InteriorMethod] = if patch && b.trim.is_some() {
            &[
                InteriorMethod::None,
                InteriorMethod::Detect,
                InteriorMethod::Waterline,
                InteriorMethod::PanelRange,
                InteriorMethod::Mesh,
            ]
        } else if patch {
            &[InteriorMethod::None, InteriorMethod::Detect, InteriorMethod::PanelRange, InteriorMethod::Mesh]
        } else {
            &InteriorMethod::ALL
        };
        if patch && !methods.contains(&b.interior.method) {
            warning(
                ui,
                &format!(
                    "{}: the higher-order method takes the interior surface only as patches: of the body's mesh (Detect, Panel Range), of a patch mesh of its own (Mesh), or the waterline's of a trimmed hull (Waterline).",
                    b.interior.method.label()
                ),
            );
        }
        if patch && b.interior.method == InteriorMethod::Mesh && b.interior.mesh.as_ref().is_some_and(|m| !m.is_patch())
        {
            warning(ui, "The interior surface of a patch mesh must be a patch mesh too (flat, B-spline or analytic).");
        }
        choice_of(ui, ("irr", i), "Interior Surface", methods, InteriorMethod::label, &mut b.interior.method);
        if let Some(lid) = b.wetted_analytic_lid() {
            warning(
                ui,
                &format!(
                    "Patch {lid} of the mesh is its interior free surface (lid): not taken here, it is solved as hull at z = 0. Use Detect or Panel Range {lid} … {lid}, or turn Interior Free Surface Patch off in the mesh's Details."
                ),
            );
        }
        let range = b.interior.method == InteriorMethod::PanelRange;
        if b.interior.method == InteriorMethod::Mesh {
            mesh_rows(ui, &library, &mut b.interior.mesh, MeshTarget::Interior(i), actions);
        } else {
            integer(ui, ("irr", i), "Start Panel", &mut b.interior.first_panel, range);
            integer(ui, ("irr", i), "End Panel", &mut b.interior.last_panel, range);
            ranges_row(ui, ("irr", i), "Further Ranges", &mut b.interior.more_ranges, range);
        }
        if b.interior.method != InteriorMethod::None && b.mesh.is_some() {
            // What the viewport shows; the solve has the engine build the surface itself.
            let text = match derived.role_count(i, PanelRole::InteriorSurface) {
                Ok(n) => format!("{n} panels (as the engine builds it)"),
                Err(e) => format!("Not shown: {e}"),
            };
            read_only(ui, "Preview", &text);
        }
    });
    section(ui, "Trim", |ui| {
        let mut on = b.trim.is_some();
        if check(
            ui,
            "Trim the Hull",
            &mut on,
            Some("The engine moves the mesh and cuts it at the free surface (a hull with freeboard)."),
        ) {
            b.trim = on.then_some([0.0; 3]);
        }
        let mut t = b.trim.unwrap_or([0.0; 3]);
        let mut changed = number(ui, ("trim", i), "Elevation", "m", &mut t[0], on);
        changed |= number(ui, ("trim", i), "Pitch", "deg", &mut t[1], on);
        changed |= number(ui, ("trim", i), "Roll", "deg", &mut t[2], on);
        if changed && on {
            b.trim = Some(t);
        }
    });
    section(ui, "Position", |ui| {
        for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
            number(ui, ("position", i), axis, "m", &mut b.position[k], true);
        }
        number(ui, ("position", i), "Roll", "deg", &mut b.roll, true);
        number(ui, ("position", i), "Pitch", "deg", &mut b.pitch, true);
        number(ui, ("position", i), "Yaw", "deg", &mut b.yaw, true);
    });
    section(ui, "Rotation Center", |ui| {
        read_only(ui, "Coordinate System", "Body-Local Mesh Coordinates");
        let mut changed = false;
        for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
            changed |= number(ui, ("reference", i), axis, "m", &mut b.reference[k], true);
        }
        if changed {
            b.update_mesh_properties(density, gravity);
        }
    });
    section(ui, "Modes", |ui| {
        for (k, name) in DOF_LABELS.iter().enumerate() {
            check(ui, &format!("{} - {name}", k + 1), &mut b.modes[k], None);
        }
    });
    section(ui, "Excitation", |ui| {
        let options = ["Diffraction".to_string(), "Haskind".to_string()];
        let mut index = usize::from(b.haskind);
        if choice(ui, ("excitation", i), "Excitation Method", &options, &mut index) {
            b.haskind = index == 1;
        }
    });
    section(ui, "Inertia Properties", |ui| {
        let options = ["Displaced".to_string(), "Given".to_string()];
        let mut index = usize::from(b.mass_calculation == MassCalculation::Manual);
        if choice(ui, ("mass", i), "Mass", &options, &mut index) {
            // switching to Manual keeps the last calculated value
            b.mass_calculation = if index == 1 { MassCalculation::Manual } else { MassCalculation::FromMesh };
            b.update_mesh_properties(density, gravity);
        }
        number(ui, ("mass", i), "Mass", "kg", &mut b.mass, b.mass_calculation == MassCalculation::Manual);
        read_only(ui, "Volume [m³]", &format_number(b.displaced_volume));
        read_only(ui, "Center of Buoyancy [m]", &vec3_text(b.center_of_buoyancy));
        read_only(ui, "Waterplane Area [m²]", &format_number(b.waterplane_area));
        if let Some(e) = &b.hydrostatics_error {
            read_only(ui, "Hydrostatics", e);
        }
        for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
            number(ui, ("cog", i), &format!("Center of Gravity {axis}"), "m", &mut b.center_of_gravity[k], true);
        }
        for (k, name) in ["Ixx", "Ixy", "Ixz", "Iyy", "Iyz", "Izz"].iter().enumerate() {
            number(ui, ("inertia", i), name, "kg·m²", &mut b.inertia[k], true);
        }
    });
    let new_name = project.bodies[i].name.clone();
    project.rename_body(&old_name, &new_name);
}

pub(super) fn matrix(ui: &mut Ui, m: &mut Matrix6, origin: &mut MatrixOrigin, body: usize, damping: bool) {
    let (title, unit) = if damping { ("Additional Damping", "N·s/m") } else { ("Additional Stiffness", "N/m") };
    section(ui, title, |ui| {
        let options = ["Rotation Center".to_string(), "Center of Gravity".to_string(), "Point".to_string()];
        let mut index = match origin {
            MatrixOrigin::RotationCenter => 0,
            MatrixOrigin::CenterOfGravity => 1,
            MatrixOrigin::Point(_) => 2,
        };
        if choice(ui, (title, body), if damping { "Damping Origin" } else { "Stiffness Origin" }, &options, &mut index)
        {
            *origin = match index {
                1 => MatrixOrigin::CenterOfGravity,
                2 => MatrixOrigin::Point([0.0; 3]),
                _ => MatrixOrigin::RotationCenter,
            };
        }
        if let MatrixOrigin::Point(p) = origin {
            for (k, axis) in ["Point X", "Point Y", "Point Z"].iter().enumerate() {
                number(ui, (title, body), axis, "m", &mut p[k], true);
            }
        }
        matrix_entries(ui, (title, body), m, unit);
    });
}

/// The 36 entries of a 6 x 6 matrix, edited here or (Edit in Table) in the table.
pub(super) fn matrix_entries(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
    m: &mut Matrix6,
    unit: &str,
) {
    check(ui, "Edit in Table", &mut m.edit_in_table, None);
    let enabled = !m.edit_in_table;
    for r in 0..6 {
        for c in 0..6 {
            let mut v = m.get(r, c);
            if number(ui, id, &format!("({},{})", r + 1, c + 1), unit, &mut v, enabled) {
                m.set(r, c, v);
            }
        }
    }
}

/// The table of a 6 x 6 matrix.
pub(super) fn matrix_table(title: String, m: &Matrix6) -> Output {
    let mut columns = vec![Column::new("Row")];
    columns.extend((1..=6).map(|c| Column::editable(c.to_string(), m.edit_in_table)));
    let rows = (0..6)
        .map(|r| std::iter::once(Cell::Int(r as i64 + 1)).chain((0..6).map(|c| Cell::Num(m.get(r, c)))).collect())
        .collect();
    Output { title, table: Some(TableData { columns, rows }), plot: None, tabs: None }
}

/// The terms of a generalized mode in table order: (piece, component, row c px py pz).
pub(super) fn mode_terms(g: Option<&gui_core::project::GeneralizedMode>) -> Vec<(usize, usize, [f64; 4])> {
    let mut out = Vec::new();
    for (j, piece) in g.into_iter().flat_map(|g| g.pieces.iter().enumerate()) {
        for (c, terms) in piece.terms.iter().enumerate() {
            out.extend(terms.iter().map(|t| (j, c, *t)));
        }
    }
    out
}

/// The polygon (line 0) and partition points (lines 1, 2, ...) of an automatic control surface.
pub(super) fn control_points(c: Option<&ControlSurface>) -> Vec<(usize, usize, [f64; 2])> {
    let Some(c) = c.filter(|c| c.method == ControlMethod::Automatic) else { return Vec::new() };
    let mut out: Vec<(usize, usize, [f64; 2])> =
        c.boundary.iter().flatten().enumerate().map(|(j, p)| (0, j, *p)).collect();
    for (k, line) in c.partitions.iter().enumerate() {
        out.extend(line.iter().enumerate().map(|(j, p)| (k + 1, j, *p)));
    }
    out
}

/// A porous sheet moving with the body: records of the body's mesh (the engine's rule).
pub(super) fn damper(ui: &mut Ui, d: &mut PanelSurface, body: usize, k: usize) {
    section(ui, &format!("Dipole Damper {}", k + 1), |ui| {
        message(ui, "A porous sheet moving with the body (in its forces): a record range of the body mesh.");
        check(
            ui,
            "Enabled",
            &mut d.enabled,
            Some(
                "Off: solved as if absent; its records of the body mesh stay out of the wetted surface (the engine's references do not change).",
            ),
        );
        d.source = SurfaceSource::PanelRange;
        integer(ui, ("damper", body, k), "Start Panel", &mut d.first_panel, true);
        integer(ui, ("damper", body, k), "End Panel", &mut d.last_panel, true);
        ranges_row(ui, ("damper", body, k), "Further Ranges", &mut d.more_ranges, true);
        number(ui, ("damper", body, k), "Damping Factor", "m/s", &mut d.damping_factor, true);
    });
}

pub(super) fn tank(
    ui: &mut Ui,
    library: &Library,
    t: &mut Tank,
    body: usize,
    k: usize,
    actions: &mut Vec<DetailsAction>,
) {
    section(ui, &format!("Tank {}", k + 1), |ui| {
        message(ui, "An internal tank: its walls are a record range of the body mesh, normals into the tank fluid.");
        check(
            ui,
            "Enabled",
            &mut t.enabled,
            Some(
                "Off: solved as if absent; its records of the body mesh stay out of the wetted surface (the engine's references do not change).",
            ),
        );
        integer(ui, ("tank", body, k), "Start Panel", &mut t.first_panel, true);
        integer(ui, ("tank", body, k), "End Panel", &mut t.last_panel, true);
        ranges_row(ui, ("tank", body, k), "Further Ranges", &mut t.more_ranges, true);
        number(ui, ("tank", body, k), "Density Ratio", "", &mut t.density_ratio, true);
        let options = ["Walls Top".to_string(), "Given".to_string()];
        let mut index = usize::from(t.surface_level.is_some());
        if choice(ui, ("tank", body, k), "Surface Level", &options, &mut index) {
            t.surface_level = (index == 1).then_some(0.0);
        }
        let mut z = t.surface_level.unwrap_or(0.0);
        if number(ui, ("tank", body, k), "Surface Level Z", "m", &mut z, t.surface_level.is_some()) {
            t.surface_level = Some(z);
        }
        let mut n = t.sheets.len() as i64;
        if integer(ui, ("tank", body, k), "Sheets Inside", &mut n, true) {
            t.sheets.resize(n.clamp(0, 64) as usize, Default::default());
        }
        let mut n = t.free_surface_plates.len() as i64;
        if integer(ui, ("tank", body, k), "Free Surface Plates", &mut n, true) {
            t.free_surface_plates.resize(n.clamp(0, 64) as usize, Default::default());
        }
    });
    for (j, plate) in t.free_surface_plates.iter_mut().enumerate() {
        surface(ui, library, &[], plate, body, SurfaceKind::TankPlate(k, j), actions);
    }
    for (j, sheet) in t.sheets.iter_mut().enumerate() {
        let title = format!("Tank {} · Sheet {}", k + 1, j + 1);
        section(ui, &title, |ui| {
            let kinds = ["Dipole".to_string(), "Dipole Damper".to_string()];
            let mut index = usize::from(sheet.damper);
            if choice(ui, (title.as_str(), body), "Kind", &kinds, &mut index) {
                sheet.damper = index == 1;
            }
            integer(ui, (title.as_str(), body), "Start Panel", &mut sheet.first_panel, true);
            integer(ui, (title.as_str(), body), "End Panel", &mut sheet.last_panel, true);
            ranges_row(ui, (title.as_str(), body), "Further Ranges", &mut sheet.more_ranges, true);
            number(ui, (title.as_str(), body), "Damping Factor", "m/s", &mut sheet.damping_factor, sheet.damper);
        });
    }
}

pub(super) fn control_surface(
    ui: &mut Ui,
    library: &Library,
    c: &mut ControlSurface,
    body: usize,
    actions: &mut Vec<DetailsAction>,
) {
    section(ui, "Control Surface", |ui| {
        message(ui, "Around the body, for the control-surface drift (Second Order → Drift).");
        choice_of(ui, ("control", body), "Control Surface", &ControlMethod::ALL, ControlMethod::label, &mut c.method);
        match c.method {
            ControlMethod::Cylinder => {
                number(ui, ("control", body), "Radius", "m", &mut c.radius, true);
                number(ui, ("control", body), "Depth", "m", &mut c.depth, true);
                number(ui, ("control", body), "Inner Radius", "m", &mut c.inner_radius, true);
            }
            ControlMethod::Ellipsoid => {
                for (k, axis) in ["a", "b", "c"].iter().enumerate() {
                    number(ui, ("control", body), &format!("Semi-Axis {axis}"), "m", &mut c.semi_axes[k], true);
                }
                number(ui, ("control", body), "Inner Semi-Axis a", "m", &mut c.inner_semi_axes[0], true);
                number(ui, ("control", body), "Inner Semi-Axis b", "m", &mut c.inner_semi_axes[1], true);
            }
            ControlMethod::Automatic => {
                let shapes = ["Circle".to_string(), "Polygon".to_string()];
                let mut index = usize::from(c.boundary.is_some());
                if choice(ui, ("control", body), "Outer", &shapes, &mut index) {
                    c.boundary = (index == 1).then(|| {
                        // a quarter circle of the radius, in the stored sector
                        (0..=4)
                            .map(|j| {
                                let a = std::f64::consts::FRAC_PI_2 * j as f64 / 4.0;
                                [c.radius * a.cos(), c.radius * a.sin()]
                            })
                            .collect()
                    });
                }
                match &mut c.boundary {
                    None => {
                        number(ui, ("control", body), "Radius", "m", &mut c.radius, true);
                    }
                    Some(polygon) => {
                        let mut n = polygon.len() as i64;
                        if integer(ui, ("control", body), "Polygon Points", &mut n, true) {
                            let fill = polygon.last().copied().unwrap_or([c.radius, 0.0]);
                            polygon.resize(n.clamp(3, 10_000) as usize, fill);
                        }
                    }
                }
                let mut lines = c.partitions.len() as i64;
                if integer(ui, ("control", body), "Partitions", &mut lines, true) {
                    c.partitions.resize(lines.clamp(0, 64) as usize, vec![[0.0, 0.0], [c.radius, 0.0]]);
                }
                for (j, line) in c.partitions.iter_mut().enumerate() {
                    let mut n = line.len() as i64;
                    if integer(ui, ("control", body), &format!("Line {} Points", j + 1), &mut n, true) {
                        let fill = line.last().copied().unwrap_or([0.0, 0.0]);
                        line.resize(n.clamp(2, 10_000) as usize, fill);
                    }
                }
                if c.boundary.is_some() || !c.partitions.is_empty() {
                    message(
                        ui,
                        "Edit the (x, y) points in the table: body coordinates, the stored sector, counter-clockwise.",
                    );
                }
                number(ui, ("control", body), "Depth", "m", &mut c.depth, true);
                message(ui, "The surface includes the free surface between the waterline and the outer surface.");
            }
            ControlMethod::Mesh => {
                mesh_rows(ui, library, &mut c.mesh, MeshTarget::Control(body), actions);
            }
        }
        if c.method != ControlMethod::Mesh {
            number(ui, ("control", body), "Panel Size", "m", &mut c.panel_size, true);
        }
        let options = ["Control Surface".to_string(), "Body".to_string()];
        let mut index = usize::from(c.body_waterline_term);
        if choice(ui, ("control", body), "Waterline Term", &options, &mut index) {
            c.body_waterline_term = index == 1;
        }
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceKind {
    Dipole(usize),
    Pressure(usize),
    Plate(usize),
    /// A plate (the second index) of a tank (the first) of the body.
    TankPlate(usize, usize),
}

/// A surface of a body. `others`: the other bodies, for a generated free-surface plate.
pub(super) fn surface(
    ui: &mut Ui,
    library: &Library,
    others: &[String],
    s: &mut PanelSurface,
    body: usize,
    kind: SurfaceKind,
    actions: &mut Vec<DetailsAction>,
) {
    let pressure = matches!(kind, SurfaceKind::Pressure(_));
    let title = match kind {
        SurfaceKind::Dipole(k) => format!("Dipole {}", k + 1),
        SurfaceKind::Pressure(k) => format!("Pressure Surface {}", k + 1),
        SurfaceKind::Plate(k) => format!("Free Surface Plate {}", k + 1),
        SurfaceKind::TankPlate(t, k) => format!("Tank {} · Free Surface Plate {}", t + 1, k + 1),
    };
    let title = title.as_str();
    section(ui, title, |ui| {
        check(
            ui,
            "Enabled",
            &mut s.enabled,
            Some(
                "Off: solved as if absent; its records of the body mesh stay out of the wetted surface (the engine's references do not change).",
            ),
        );
        // a body's plate may also be generated by the engine (not a tank's)
        let generated = matches!(kind, SurfaceKind::Plate(_));
        let mut options = vec!["Panel Range of the Body Mesh".to_string(), "Mesh".to_string()];
        if generated {
            options.push("Generated in a Waterline".to_string());
        }
        let mut index = match s.source {
            SurfaceSource::PanelRange => 0,
            SurfaceSource::ExternalMesh => 1,
            SurfaceSource::Generated => 2,
        };
        if choice(ui, (title, body), "Source", &options, &mut index) {
            s.source = match index {
                1 => SurfaceSource::ExternalMesh,
                2 => SurfaceSource::Generated,
                _ => SurfaceSource::PanelRange,
            };
        }
        match s.source {
            SurfaceSource::PanelRange => {
                integer(ui, (title, body), "Start Panel", &mut s.first_panel, true);
                integer(ui, (title, body), "End Panel", &mut s.last_panel, true);
                ranges_row(ui, (title, body), "Further Ranges", &mut s.more_ranges, true);
            }
            SurfaceSource::ExternalMesh => {
                let target = match kind {
                    SurfaceKind::Dipole(k) => MeshTarget::Dipole(body, k),
                    SurfaceKind::Pressure(k) => MeshTarget::Pressure(body, k),
                    SurfaceKind::Plate(k) => MeshTarget::Plate(body, k),
                    SurfaceKind::TankPlate(t, k) => MeshTarget::TankPlate(body, t, k),
                };
                mesh_rows(ui, library, &mut s.mesh, target, actions);
            }
            SurfaceSource::Generated => {
                number(ui, (title, body), "Inside X", "m", &mut s.inside[0], true);
                number(ui, (title, body), "Inside Y", "m", &mut s.inside[1], true);
                let mut names = vec!["This Body".to_string()];
                names.extend(others.iter().cloned());
                let mut index = s.body.as_ref().and_then(|b| others.iter().position(|o| o == b)).map_or(0, |p| p + 1);
                if choice(ui, (title, body), "Waterline Of", &names, &mut index) {
                    s.body = (index > 0).then(|| others[index - 1].clone());
                }
                message(
                    ui,
                    "The engine makes the plate: the water of z = 0 inside the waterline loop of a trimmed hull that holds the point (x, y), in that hull's body axes after its trim (a moonpool, the column of a tube), less the waterplanes of hulls within it. The hull is this body (it needs a trim) or the body named, a piston over a trimmed tube.",
                );
            }
        }
        match kind {
            SurfaceKind::Plate(_) => message(
                ui,
                if s.source == SurfaceSource::Generated {
                    "A massless rigid plate in z = 0, the water below it only: it moves with the body's rigid modes (a body of plates alone: heave, roll, pitch)."
                } else {
                    "A massless rigid plate in z = 0, the water below it only: it moves with the body's rigid modes (a body of plates alone: heave, roll, pitch); records in z = 0."
                },
            ),
            SurfaceKind::TankPlate(..) => message(
                ui,
                "A massless rigid plate in the tank's free surface (a deck, a full cover), the tank's fluid below it only, moving with the body: records of the body mesh or a mesh of its own at the tank's surface level.",
            ),
            _ => {}
        }
        if pressure {
            check(
                ui,
                "Chamber Coupling",
                &mut s.chamber_coupling,
                Some("The restoring coupling of a constant-volume chamber (an air cushion)."),
            );
            let conventions = ["Variational".to_string(), "Numeric Compatibility".to_string()];
            let mut index = usize::from(s.numeric_convention);
            if choice(ui, (title, body), "Response Convention", &conventions, &mut index) {
                s.numeric_convention = index == 1;
            }
            let mut n = s.pressure_modes.len() as i64;
            if integer(ui, (title, body), "Pressure Modes", &mut n, true) {
                s.pressure_modes.resize(n.clamp(1, 16) as usize, Default::default());
            }
        }
    });
    if pressure {
        for (j, mode) in s.pressure_modes.iter_mut().enumerate() {
            let mtitle = format!("{title} · Mode {}", j + 1);
            section(ui, &mtitle, |ui| {
                choice_of(
                    ui,
                    (mtitle.as_str(), body),
                    "Shape",
                    &PressureShape::ALL,
                    PressureShape::label,
                    &mut mode.shape,
                );
                number(ui, (mtitle.as_str(), body), "Additional Stiffness", "N/m", &mut mode.stiffness, true);
                number(ui, (mtitle.as_str(), body), "Additional Damping", "N·s/m", &mut mode.damping, true);
            });
        }
    }
}
