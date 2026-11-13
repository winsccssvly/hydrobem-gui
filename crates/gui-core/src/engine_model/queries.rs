//! The engine's answers on the geometry without a solve: a body's hydrostatics, the panels by
//! role, a mode shape, a mesh alone, the interior free surface.

use std::collections::HashMap;
use std::sync::Arc;

use hydrobem_engine::{Hydrostatics, Model, PanelRole};

use super::BuildOptions;
use super::write::{build_model, vec3};
use crate::mesh::Mesh;
use crate::project::{Body, Project, V3};

/// The hydrostatics of a body of `mesh` with its rotation centre at `reference`, as the solve
/// computes them (body axes about R).
/// `ranges` are the records of the wetted surface (none: every record).
pub fn body_hydrostatics(
    mesh: &Arc<Mesh>,
    ranges: &[[i64; 2]],
    reference: V3,
    density: f64,
    gravity: f64,
) -> Result<Hydrostatics, String> {
    let run = || -> hydrobem_engine::Result<Hydrostatics> {
        let mut m = Model::new();
        let root = m.root();
        let e = m.object("environment")?;
        m.set(e, "density", density)?;
        m.set(e, "gravity", gravity)?;
        // the model rules need something to solve; nothing is solved
        m.set(e, "omegas", vec![1.0])?;
        m.set(e, "headings", vec![0.0])?;
        let mid = m.add(root, "meshes", None, "mesh", None)?;
        mesh.write_object(&mut m, mid)?;
        if mesh.is_patch() {
            let o = m.object("options")?;
            m.set(o, "higher_order", true)?;
        }
        let b = m.add(root, "bodies", None, "body", None)?;
        m.set_surface(b, "mesh", "ranges", "mesh", ranges)?;
        m.set(b, "rotation_center", vec3(reference))?;
        m.body_hydrostatics(b)
    };
    run().map_err(|e| e.message().to_string())
}

/// The elements of the problem as the engine builds them, by role (wetted, interior surface,
/// dipole, damper, pressure surface, tank, plate, control surface; lids and dampers of the
/// water), global coordinates, symmetry expanded. The bodies by their index in the project.
#[derive(Clone, Debug, Default)]
pub struct RolePanels {
    /// The elements of each body as the engine solves them (`Model::body_panels`): their counts by
    /// role.
    pub bodies: HashMap<usize, Vec<(PanelRole, [V3; 4])>>,
    /// What the viewport draws of each body: the engine's tessellation of the same elements
    /// (`Model::body_tessellation`), a low-order panel one quadrilateral, a patch several on its
    /// exact surface, each with its role, its corner normals and the sides that bound its element.
    pub smooth: HashMap<usize, Vec<(PanelRole, crate::mesh::SmoothQuad)>>,
}

pub fn role_panels(project: &Project) -> Result<RolePanels, String> {
    let options =
        BuildOptions { field_points: false, gui_state: false, placeholder_frequency: true, meshed_bodies_only: true };
    let run = || -> hydrobem_engine::Result<RolePanels> {
        let mut built = build_model(project, &options)?;
        let m = &mut built.model;
        // a control surface is built only for its drift
        if project.active_bodies().any(|b| b.control_surface.is_some()) {
            let second = m.object("second_order")?;
            m.set(second, "drift_control_surface", true)?;
        }
        let mut panels = RolePanels::default();
        for (i, body) in project.bodies.iter().enumerate() {
            if body.suppressed || !body.has_geometry() {
                continue;
            }
            let id = m.object(&format!("bodies/{}", body.name))?;
            // the default roles, then the control surface on its own: each its own divisions, so
            // that the few patches of a control surface are not drawn as coarsely as a body of
            // many elements
            let mut groups = vec![None];
            if body.control_surface.is_some() {
                groups.push(Some(PanelRole::ControlSurface.name()));
            }
            let (mut list, mut smooth) = (Vec::new(), Vec::new());
            for role in groups {
                let p = m.body_panels(id, role, true)?;
                let n = crate::mesh::tessellation_steps(p.len());
                let t = m.body_tessellation(id, role, true, n, n)?;
                list.extend((0..p.len()).filter_map(|k| Some((p.role[k]?, p.vertices[k]))));
                smooth.extend(drawn_quads(&t, p.len() <= crate::scene::OUTLINE_LIMIT));
            }
            panels.bodies.insert(i, list);
            panels.smooth.insert(i, smooth);
        }
        Ok(panels)
    };
    run().map_err(|e| e.message().to_string())
}

/// The quadrilaterals of a tessellation with their roles, outlined where they bound their element
/// (its seven keys); a low-order panel (one quadrilateral) only when `outline_panels`.
fn drawn_quads(t: &hydrobem_engine::Tessellation, outline_panels: bool) -> Vec<(PanelRole, crate::mesh::SmoothQuad)> {
    let keys: Vec<hydrobem_engine::PanelKey> = (0..t.len()).map(|k| t.key(k)).collect();
    let edges = crate::mesh::boundary_edges(&t.vertices, &keys);
    let mut per_element: HashMap<hydrobem_engine::PanelKey, usize> = HashMap::new();
    for key in &keys {
        *per_element.entry(*key).or_default() += 1;
    }
    (0..t.len())
        .filter_map(|k| {
            let role = t.role[k]?;
            let alone = per_element[&keys[k]] == 1;
            let edges = if alone && !outline_panels { [false; 4] } else { edges[k] };
            Some((role, crate::mesh::SmoothQuad { corners: t.vertices[k], normals: t.normals[k], edges }))
        })
        .collect()
}

/// A panel's corners and the displacement of each corner.
pub type DisplacedPanel = ([V3; 4], [V3; 4]);

/// The wetted panels of body `i` (global, every symmetry copy) with the displacement of each
/// corner by one unit of its generalized coordinate `k` (the engine's `mode_displacements`).
pub fn mode_shape(project: &Project, i: usize, k: usize) -> Result<Vec<DisplacedPanel>, String> {
    let body = project.bodies.get(i).ok_or("no such body")?;
    // the body alone: its coordinates are the six rigid ones, then its generalized modes
    let mut alone = Project { environment: project.environment.clone(), ..Project::default() };
    alone.bodies.push(Body { suppressed: false, couplings: Vec::new(), ..body.clone() });
    let options =
        BuildOptions { field_points: false, gui_state: false, placeholder_frequency: true, meshed_bodies_only: false };
    let run = || -> hydrobem_engine::Result<Vec<DisplacedPanel>> {
        let mut built = build_model(&alone, &options)?;
        let m = &mut built.model;
        let id = m.object(&format!("bodies/{}", body.name))?;
        let p = m.body_panels(id, Some(PanelRole::Wetted.name()), true)?;
        let rows: Vec<usize> = (0..p.len()).filter(|&r| p.element[r] > 0).collect();
        let points: Vec<[f64; 3]> = rows.iter().flat_map(|&r| p.vertices[r]).collect();
        let elements: Vec<i32> = rows.iter().flat_map(|&r| [p.element[r]; 4]).collect();
        let bodies = vec![0; points.len()];
        let (d, coordinates) = m.mode_displacements(&bodies, &elements, &points)?;
        let column = 6 + k;
        if column >= coordinates {
            return Err(hydrobem_engine::Error::value(format!(
                "generalized mode {} of {}: the model has {coordinates} coordinates",
                k + 1,
                body.name
            )));
        }
        Ok(rows
            .iter()
            .enumerate()
            .map(|(n, &r)| {
                let at = |v: usize| {
                    let base = ((4 * n + v) * coordinates + column) * 3;
                    [d[base], d[base + 1], d[base + 2]]
                };
                (p.vertices[r], [at(0), at(1), at(2), at(3)])
            })
            .collect())
    };
    run().map_err(|e| e.message().to_string())
}

/// What the engine says about a mesh alone.
pub struct MeshInfo {
    /// Volumes, centre of volume, area, bounds, waterplane.
    pub geometry: hydrobem_engine::MeshGeometry,
    /// Per stored record its size, aspect and flags (orientation, free and non-manifold edges).
    pub quality: hydrobem_engine::MeshQuality,
    /// Per stored element its centroid, unit normal into the fluid and area.
    pub elements: hydrobem_engine::MeshElements,
}

pub fn mesh_info(mesh: &Arc<Mesh>) -> Result<MeshInfo, String> {
    let run = || -> hydrobem_engine::Result<_> {
        let mut m = Model::new();
        let root = m.root();
        let id = m.add(root, "meshes", None, "mesh", None)?;
        mesh.write_object(&mut m, id)?;
        Ok(MeshInfo {
            geometry: m.mesh_geometry(id)?,
            quality: m.mesh_geometry_quality(id)?,
            elements: m.mesh_elements(id)?,
        })
    };
    run().map_err(|e| e.message().to_string())
}

/// The interior free surface the engine builds for `body` (generated, or a selected range):
/// global coordinates, symmetry expanded. For the viewer only; a solve passes the method and the
/// engine builds the surface again.
pub fn interior_preview(body: &Body, environment: &crate::project::Environment) -> Result<Vec<[V3; 4]>, String> {
    if body.has_roll_or_pitch() {
        return Err(format!("{}: the engine accepts body yaw only; set Roll and Pitch to zero.", body.name));
    }
    let mut project = Project { environment: environment.clone(), ..Project::default() };
    project.bodies.push(Body { suppressed: false, ..body.clone() });
    let options =
        BuildOptions { field_points: false, gui_state: false, placeholder_frequency: true, meshed_bodies_only: false };
    let run = || -> hydrobem_engine::Result<Vec<[V3; 4]>> {
        let mut built = build_model(&project, &options)?;
        let b = built.model.object(&format!("bodies/{}", body.name))?;
        // by the engine's tessellation: a patch of a curved waterline drawn on its exact surface
        let role = Some(PanelRole::InteriorSurface.name());
        let n = crate::mesh::tessellation_steps(built.model.body_panels(b, role, true)?.len());
        Ok(built.model.body_tessellation(b, role, true, n, n)?.vertices)
    };
    run().map_err(|e| e.message().to_string())
}
