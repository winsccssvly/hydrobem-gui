//! The details of a lid and of a library mesh (analytic shapes included).

use eframe::egui::Ui;
use gui_core::analytic::{AnalyticKind, AnalyticSpec};
use gui_core::format::format_number;
use gui_core::hydrobem_engine::QualityFlags;
use gui_core::project::Project;

use super::tables::mesh_kind;
use super::{Derived, DetailsAction, MeshTarget, library, mesh_rows, vec3_text};
use crate::widgets::{
    FileAction, check, choice, file, integer, message, number, optional_number, ranges_row, read_only, section, text,
    warning,
};

pub(super) fn lid(ui: &mut Ui, project: &mut Project, i: usize, actions: &mut Vec<DetailsAction>) {
    let library = library(project);
    let bodies: Vec<String> = project.bodies.iter().map(|b| b.name.clone()).collect();
    let l = &mut project.lids[i];
    section(ui, "Lid", |ui| {
        text(ui, ("lid", i), "Name", &mut l.name);
        check(ui, "Visibility", &mut l.visible, Some("Controls display in the 3D viewer only."));
        let mut enabled = !l.suppressed;
        if check(
            ui,
            "Enabled",
            &mut enabled,
            Some("Off: excluded from the solver input (the engine's enabled = false)."),
        ) {
            l.suppressed = !enabled;
        }
        number(ui, ("lid", i), "Damping Factor", "", &mut l.damping_factor, true);
        let mut options = vec!["(global coordinates)".to_string()];
        options.extend(bodies.iter().cloned());
        let mut index = l.placed_with.as_ref().and_then(|n| bodies.iter().position(|b| b == n)).map_or(0, |k| k + 1);
        if choice(ui, ("lid", i), "Placed With", &options, &mut index) {
            l.placed_with = index.checked_sub(1).map(|k| bodies[k].clone());
        }
        if l.placed_with.is_some() {
            message(ui, "Records of the body's mesh, placed by its position and yaw; they leave its wetted surface.");
            integer(ui, ("lid", i), "Start Panel", &mut l.range[0], true);
            integer(ui, ("lid", i), "End Panel", &mut l.range[1], true);
            ranges_row(ui, ("lid", i), "Further Ranges", &mut l.more_ranges, true);
        }
    });
    if project.lids[i].placed_with.is_some() {
        return;
    }
    let l = &mut project.lids[i];
    section(ui, "Mesh", |ui| {
        if mesh_rows(ui, &library, &mut l.mesh, MeshTarget::Lid(i), actions) {
            actions.push(DetailsAction::Refit);
        }
    });
}

pub(super) fn mesh_details(
    ui: &mut Ui,
    project: &mut Project,
    i: usize,
    derived: &Derived,
    actions: &mut Vec<DetailsAction>,
) {
    let users = project.mesh_users(i);
    let others: Vec<String> =
        project.meshes.iter().enumerate().filter(|(k, _)| *k != i).map(|(_, e)| e.name.clone()).collect();
    let entry = &mut project.meshes[i];
    let mesh = entry.mesh.clone();
    section(ui, "Mesh", |ui| {
        let mut name = entry.name.clone();
        if text(ui, ("mesh-name", i), "Name", &mut name) {
            let name = gui_core::project::object_name(&name);
            if !others.contains(&name) {
                entry.name = name;
            }
        }
        if mesh.analytic.is_some() {
            read_only(ui, "Source", "Made in the GUI (analytic shape)");
        } else {
            match file(ui, "Source File", &mesh.source_file) {
                Some(FileAction::Browse) => actions.push(DetailsAction::ReplaceMesh(i, None)),
                Some(FileAction::Dropped(p)) => actions.push(DetailsAction::ReplaceMesh(i, Some(p))),
                None => {}
            }
        }
        read_only(ui, "Title", &mesh.label);
        if mesh.patch.is_some() {
            read_only(ui, "Kind", mesh_kind(&mesh));
            read_only(ui, "Patches", &mesh.panels.len().to_string());
            // where the rows the solve takes came from, and the unknowns they make
            read_only(ui, "Discretization", &mesh.rows_from.label());
            if let Some(n) = mesh.unknowns() {
                read_only(ui, "Unknowns", &format!("{n} coefficients"));
            }
            if let Some(r) = &mesh.report {
                let [ok, warn, fail] = r.counts();
                let file = r.file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                read_only(ui, "cad2bem Report", &file);
                read_only(ui, "Checks", &format!("{}: {ok} ok, {warn} warn, {fail} fail", r.state));
                if fail > 0 {
                    warning(
                        ui,
                        "cad2bem failed a check of some patches (the table says which): convert the CAD again in cad2bem before solving.",
                    );
                }
            }
            message(
                ui,
                "A patch mesh: the bodies that use it are solved by the higher-order method (Options). The potential's discretization per patch is edited in the table.",
            );
            // read from a GDF file (a patch mesh: a higher-order one)
            let gdf =
                std::path::Path::new(&mesh.source_file).extension().is_some_and(|e| e.eq_ignore_ascii_case("gdf"));
            if gdf
                && let Some(size) = optional_number(
                    ui,
                    ("panel-size", mesh.id),
                    "Panel Size",
                    "m",
                    mesh.rows_from.panel_size(),
                    "The discretization of every patch from a panel size, as WAMIT's PANEL_SIZE: n = floor(L / size) + 1 spans along each direction (L the longer of the patch's two edges there), spline order 3, Gauss orders 3 and 4. It replaces the table's rows.",
                )
            {
                actions.push(DetailsAction::PanelSize(i, size));
            }
        }
        read_only(ui, "Stored Panels", &mesh.panels.len().to_string());
        read_only(ui, "Displayed Panels", &mesh.displayed_panels().to_string());
        read_only(ui, "Symmetry", mesh.symmetry.label());
        read_only(ui, "Reference Length [m]", &format_number(mesh.reference_length));
        if let Some(g) = mesh.file_gravity {
            read_only(ui, "File Gravity [m/s²]", &format_number(g));
        }
        read_only(ui, "Used By", &if users.is_empty() { "(unused)".to_string() } else { users.join(", ") });
    });
    if let Some(spec) = &mesh.analytic
        && let Some(changed) = analytic_shape(ui, i, spec)
    {
        actions.push(DetailsAction::RebuildAnalytic(i, changed));
    }
    match derived.meshes.get(&mesh.id) {
        Some(Ok(info)) => {
            let g = &info.geometry;
            section(ui, "Geometry (mesh coordinates, symmetry expanded)", |ui| {
                read_only(ui, "Volume [m³]", &format_number(g.volume));
                read_only(ui, "Volume x, y, z [m³]", &vec3_text(g.volumes));
                read_only(ui, "Centre of Volume [m]", &vec3_text(g.center_of_volume));
                read_only(ui, "Surface Area [m²]", &format_number(g.area));
                read_only(ui, "Waterplane Area [m²]", &format_number(g.waterplane_area));
                read_only(
                    ui,
                    "Waterplane Centroid [m]",
                    &format!(
                        "{}, {}",
                        format_number(g.waterplane_centroid[0]),
                        format_number(g.waterplane_centroid[1])
                    ),
                );
                for (k, axis) in ["X", "Y", "Z"].iter().enumerate() {
                    read_only(
                        ui,
                        &format!("{axis} Range [m]"),
                        &format!("{} … {}", format_number(g.bounds[0][k]), format_number(g.bounds[1][k])),
                    );
                }
            });
            let q = &info.quality;
            section(ui, "Quality", |ui| {
                let (lo, hi) = q.size.iter().fold((f64::INFINITY, 0.0f64), |(a, b), v| (a.min(*v), b.max(*v)));
                read_only(ui, "Panel Size [m]", &format!("{} … {}", format_number(lo), format_number(hi)));
                read_only(ui, "Largest Aspect", &format_number(q.aspect.iter().copied().fold(0.0, f64::max)));
                let count = |f: QualityFlags| q.flags.iter().filter(|x| x.contains(f)).count().to_string();
                read_only(ui, "Inconsistent Orientation", &count(QualityFlags::ORIENTATION));
                read_only(ui, "Free Edges", &count(QualityFlags::FREE_EDGE));
                read_only(ui, "Non-Manifold Edges", &count(QualityFlags::NON_MANIFOLD));
            });
        }
        Some(Err(e)) => section(ui, "Geometry", |ui| read_only(ui, "Status", e)),
        None => {}
    }
}

/// The shape of an analytic mesh made in the GUI: a changed copy to build, when edited.
fn analytic_shape(ui: &mut Ui, i: usize, spec: &AnalyticSpec) -> Option<AnalyticSpec> {
    let mut next = spec.clone();
    section(ui, "Analytic Shape", |ui| {
        message(
            ui,
            "A quarter about x = 0 and y = 0 (symmetry xz_and_yz), its waterline on z = 0. The engine makes the patches from these dimensions.",
        );
        let labels: Vec<String> = AnalyticKind::ALL.iter().map(|k| k.label().to_string()).collect();
        let mut index = AnalyticKind::ALL.iter().position(|k| *k == spec.kind).unwrap_or(0);
        if choice(ui, ("analytic", i), "Shape", &labels, &mut index) {
            next = spec.with_kind(AnalyticKind::ALL[index]);
            return;
        }
        for (k, (name, _)) in spec.kind.dimensions().iter().enumerate() {
            number(ui, ("analytic", i), name, "m", &mut next.dimensions[k], true);
        }
        if spec.kind.has_lid() {
            check(
                ui,
                "Interior Free Surface Patch",
                &mut next.lid,
                Some(
                    "The last patch: the lid against irregular frequencies; a body that takes the mesh uses it as its interior surface.",
                ),
            );
        }
        if spec.kind.has_nonuniform() {
            check(
                ui,
                "Non-Uniform Mapping",
                &mut next.nonuniform,
                Some("Patches mapped finer towards the waterline and the corners (INONUMAP = 1)."),
            );
        }
        if spec.kind == AnalyticKind::CylinderWithDiscs {
            let mut n = next.discs.len() as i64;
            if integer(ui, ("analytic", i), "Dipole Discs", &mut n, true) {
                let last = next.discs.last().copied().unwrap_or([2.0, 1.0, 0.5]);
                next.discs.resize(n.clamp(0, 20) as usize, last);
            }
            for (k, d) in next.discs.iter_mut().enumerate() {
                number(ui, ("analytic", i, k), &format!("Disc {} Outer Radius", k + 1), "m", &mut d[0], true);
                number(ui, ("analytic", i, k), &format!("Disc {} Inner Radius", k + 1), "m", &mut d[1], true);
                number(ui, ("analytic", i, k), &format!("Disc {} Depth", k + 1), "m", &mut d[2], true);
            }
        }
        read_only(ui, "Patches", &spec.patch_count().to_string());
    });
    (next != *spec).then_some(next)
}
