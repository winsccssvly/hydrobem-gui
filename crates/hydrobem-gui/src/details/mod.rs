//! Details of the input nodes and their tables (REQUIREMENTS.md: Body, Analysis and environment,
//! Wave cases, Field points).
//!
//! - `body`: a body, its matrices, dampers, tanks, control surfaces and surfaces.
//! - `mesh`: a lid and a library mesh (analytic shapes included).
//! - `analysis`: environment, options, first and second order, waves and field points.
//! - `tables`: the tables of the input nodes (`output`) and their edits (`apply_edit`).

mod analysis;
mod body;
mod mesh;
mod tables;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use eframe::egui::Ui;
use gui_core::analytic::AnalyticSpec;
use gui_core::engine_model::MeshInfo;
use gui_core::field_points::FieldPointSelection;
use gui_core::format::format_number;
use gui_core::hydrobem_engine::{MemoryEstimate, PanelRole};
use gui_core::mesh::Mesh;
use gui_core::project::{DOF_LABELS, Project, V3};

use crate::tree::{BodyPart, Node};
use crate::widgets::{check, choice, integer, message, number, ranges_row, read_only, section, text};
use analysis::{environment, field_points, first_order, frequencies, options, second_order};
use body::{body, control_surface, damper, matrix, matrix_entries, surface, tank};
use mesh::{lid, mesh_details};

pub use body::SurfaceKind;
pub use tables::{apply_edit, output};

/// What the engine says about the project as it stands, recomputed when an input changes.
pub struct Derived {
    pub field_points: FieldPointSelection,
    pub memory: Option<Result<MemoryEstimate, String>>,
    /// The panels of the problem as the engine builds them, by role (what the viewport draws), or
    /// why the engine cannot build them.
    pub roles: Result<gui_core::engine_model::RolePanels, String>,
    /// Per library mesh (by its id): what the engine says about it alone.
    pub meshes: HashMap<u64, Result<MeshInfo, String>>,
}

impl Default for Derived {
    fn default() -> Self {
        Derived {
            field_points: FieldPointSelection::default(),
            memory: None,
            roles: Err(String::new()),
            meshes: HashMap::new(),
        }
    }
}

impl Derived {
    /// The engine's panels of one role of body `i`.
    pub fn role_count(&self, i: usize, role: PanelRole) -> Result<usize, String> {
        match &self.roles {
            Ok(r) => Ok(r.bodies.get(&i).map_or(0, |p| p.iter().filter(|(x, _)| *x == role).count())),
            Err(e) => Err(e.clone()),
        }
    }

    pub fn compute(project: &Project) -> Derived {
        let field_points = gui_core::field_points::select_field_points(project);
        let memory = Some(
            gui_core::solve::make_solve_request(project)
                .and_then(|mut r| gui_core::solve::estimate_memory(&mut r))
                .map_err(|e| e.message),
        );
        let roles = gui_core::engine_model::role_panels(project);
        let meshes = project.meshes.iter().map(|e| (e.mesh.id, gui_core::engine_model::mesh_info(&e.mesh))).collect();
        Derived { field_points, memory, roles, meshes }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshTarget {
    Body(usize),
    Dipole(usize, usize),
    Pressure(usize, usize),
    Lid(usize),
    DipoleDamper(usize),
    Control(usize),
    Interior(usize),
    /// A free-surface plate of a body.
    Plate(usize, usize),
    /// A free-surface plate (the third index) of a tank (the second) of a body.
    TankPlate(usize, usize, usize),
    /// The panelled zone of the potential QTF's free surface (`second_order/free_surface_mesh`).
    FreeSurface,
}

/// Something Details asks the application to do.
pub enum DetailsAction {
    /// Read a GDF file into the mesh library (a dropped file, or a dialog when `None`) and give it
    /// to `target`, if any.
    ImportMesh(Option<MeshTarget>, Option<PathBuf>),
    /// Read the library mesh `index` again from a file (a dropped file, or a dialog).
    ReplaceMesh(usize, Option<PathBuf>),
    /// Make the analytic library mesh `index` again with this shape (its users take it).
    RebuildAnalytic(usize, AnalyticSpec),
    /// The discretization of the patch mesh `index` made from a panel size (m) by the engine.
    PanelSize(usize, f64),
    /// A structural change: the viewport fits the scene again.
    Refit,
}

/// Gives the library mesh `index` (or none) to `target`.
pub fn assign_mesh(project: &mut Project, target: MeshTarget, mesh: Option<Arc<Mesh>>) {
    let (density, gravity) = (project.environment.density, project.environment.gravity);
    match target {
        MeshTarget::Body(i) => {
            let b = &mut project.bodies[i];
            if let Some(new) = &mesh {
                let previous = b.mesh.clone();
                b.follow_analytic_lid(previous.as_deref(), new);
            }
            b.mesh = mesh;
            b.mesh_range = None;
            b.update_mesh_properties(density, gravity);
        }
        MeshTarget::Dipole(i, k) => project.bodies[i].dipoles[k].mesh = mesh,
        MeshTarget::Pressure(i, k) => project.bodies[i].pressure_surfaces[k].mesh = mesh,
        MeshTarget::Lid(i) => project.lids[i].mesh = mesh,
        MeshTarget::DipoleDamper(k) => project.fixed_dampers[k].mesh = mesh,
        MeshTarget::Control(i) => {
            if let Some(c) = &mut project.bodies[i].control_surface {
                c.mesh = mesh;
            }
        }
        MeshTarget::Interior(i) => project.bodies[i].interior.mesh = mesh,
        MeshTarget::Plate(i, k) => project.bodies[i].free_surface_plates[k].mesh = mesh,
        MeshTarget::TankPlate(i, t, k) => project.bodies[i].tanks[t].free_surface_plates[k].mesh = mesh,
        MeshTarget::FreeSurface => project.second_order.free_surface_mesh = mesh,
    }
}

/// Reads a GDF file into the library (a higher-order one without SPL file with `choice`) and gives
/// it to `target`; returns its library index.
pub fn import_mesh(
    project: &mut Project,
    target: Option<MeshTarget>,
    path: &Path,
    choice: Option<&gui_core::mesh::GdfDiscretization>,
) -> Result<usize, String> {
    let index = project.import_mesh_with(path, choice)?;
    if let Some(target) = target {
        let mesh = project.meshes[index].mesh.clone();
        assign_mesh(project, target, Some(mesh));
    }
    Ok(index)
}

/// The library as (name, mesh) pairs, for the mesh choices.
pub type Library = Vec<(String, Arc<Mesh>)>;

enum MeshPick {
    Keep,
    Set(Option<Arc<Mesh>>),
    Import,
}

/// A choice among the library's meshes, "(none)" and "Import GDF…".
fn mesh_choice(
    ui: &mut Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    library: &Library,
    current: &Option<Arc<Mesh>>,
) -> MeshPick {
    let mut options = vec!["(none)".to_string()];
    options.extend(library.iter().map(|(n, _)| n.clone()));
    options.push("Import GDF…".into());
    let mut index = current.as_ref().and_then(|m| library.iter().position(|(_, x)| x.id == m.id)).map_or(0, |i| i + 1);
    let before = index;
    if !choice(ui, key, "Mesh", &options, &mut index) || index == before {
        return MeshPick::Keep;
    }
    match index {
        0 => MeshPick::Set(None),
        i if i == options.len() - 1 => MeshPick::Import,
        i => MeshPick::Set(Some(library[i - 1].1.clone())),
    }
}

/// The mesh rows of a body, surface or lid: the choice, and what the mesh holds.
pub(super) fn mesh_rows(
    ui: &mut Ui,
    library: &Library,
    mesh: &mut Option<Arc<Mesh>>,
    target: MeshTarget,
    actions: &mut Vec<DetailsAction>,
) -> bool {
    let mut changed = false;
    match mesh_choice(ui, ("mesh", target_key(target)), library, mesh) {
        MeshPick::Keep => {}
        MeshPick::Set(m) => {
            *mesh = m;
            changed = true;
        }
        MeshPick::Import => actions.push(DetailsAction::ImportMesh(Some(target), None)),
    }
    match mesh {
        Some(m) => {
            read_only(ui, "Stored Panels", &m.panels.len().to_string());
            read_only(ui, "Symmetry", m.symmetry.label());
        }
        None => read_only(ui, "Status", "No mesh: choose one, or import a GDF or HBG file"),
    }
    changed
}

fn target_key(t: MeshTarget) -> (u8, usize, usize) {
    match t {
        MeshTarget::Body(i) => (0, i, 0),
        MeshTarget::Dipole(i, k) => (1, i, k),
        MeshTarget::Pressure(i, k) => (2, i, k),
        MeshTarget::Lid(i) => (3, i, 0),
        MeshTarget::DipoleDamper(k) => (4, k, 0),
        MeshTarget::Control(i) => (5, i, 0),
        MeshTarget::Interior(i) => (6, i, 0),
        MeshTarget::Plate(i, k) => (8, i, k),
        MeshTarget::TankPlate(i, t, k) => (9, i, t * 1024 + k),
        MeshTarget::FreeSurface => (7, 0, 0),
    }
}

/// Copies of the library's names and meshes (cheap: the meshes are shared).
pub fn library(project: &Project) -> Library {
    project.meshes.iter().map(|e| (e.name.clone(), e.mesh.clone())).collect()
}

pub(super) fn vec3_text(v: V3) -> String {
    format!("{}, {}, {}", format_number(v[0]), format_number(v[1]), format_number(v[2]))
}

pub fn show(ui: &mut Ui, project: &mut Project, node: Node, derived: &Derived, actions: &mut Vec<DetailsAction>) {
    match node {
        Node::Meshes => section(ui, "Meshes", |ui| {
            message(
                ui,
                "Right-click Meshes and choose Import Mesh… (a GDF file) or New Analytic Mesh (a cylinder, sphere, barge … of the engine's analytic shapes), or drop GDF or .hbg files onto Meshes (a CAD file is converted to .hbg by cad2bem, a program of its own). Select a mesh to view it alone; bodies, surfaces and lids choose their mesh from this library.",
            )
        }),
        Node::Mesh(i) => mesh_details(ui, project, i, derived, actions),
        Node::Bodies => section(ui, "Bodies", |ui| {
            message(
                ui,
                "Right-click Bodies and choose New Body. Right-click a body to add its additional matrices, dipoles and pressure surfaces.",
            )
        }),
        Node::Lids => section(ui, "Lids", |ui| {
            message(
                ui,
                "Damping lids on the free surface: right-click Lids → New Lid. A lid is a mesh of its own (global coordinates) or records of a body's mesh placed with it.",
            )
        }),
        Node::FixedDampers => section(ui, "Fixed Dampers", |ui| {
            message(ui, "Porous sheets at rest in global coordinates: right-click Fixed Dampers → New Fixed Damper.")
        }),
        Node::Environment => environment(ui, project),
        Node::Options => options(ui, project, derived),
        Node::FirstOrder => first_order(ui, project),
        Node::SecondOrder => second_order(ui, project, actions),
        Node::Body(i) => body(ui, project, i, derived, actions),
        Node::BodyPart(i, BodyPart::Damping) => {
            let b = &mut project.bodies[i];
            if let Some(m) = &mut b.additional_damping {
                matrix(ui, m, &mut b.damping_origin, i, true);
            }
        }
        Node::BodyPart(i, BodyPart::Stiffness) => {
            let b = &mut project.bodies[i];
            if let Some(m) = &mut b.additional_stiffness {
                matrix(ui, m, &mut b.stiffness_origin, i, false);
            }
        }
        Node::BodyPart(i, BodyPart::Dipole(k)) => {
            let library = library(project);
            surface(ui, &library, &[], &mut project.bodies[i].dipoles[k], i, SurfaceKind::Dipole(k), actions)
        }
        Node::BodyPart(i, BodyPart::Plate(k)) => {
            let library = library(project);
            // the other bodies, whose trimmed waterline a generated plate may cover
            let others: Vec<String> =
                project.bodies.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, b)| b.name.clone()).collect();
            let plate = &mut project.bodies[i].free_surface_plates[k];
            surface(ui, &library, &others, plate, i, SurfaceKind::Plate(k), actions)
        }
        Node::BodyPart(i, BodyPart::Damper(k)) => damper(ui, &mut project.bodies[i].dipole_dampers[k], i, k),
        Node::BodyPart(i, BodyPart::Tank(k)) => {
            let library = library(project);
            tank(ui, &library, &mut project.bodies[i].tanks[k], i, k, actions)
        }
        Node::BodyPart(i, BodyPart::ControlSurface) => {
            let library = library(project);
            if let Some(c) = &mut project.bodies[i].control_surface {
                control_surface(ui, &library, c, i, actions);
            }
        }
        Node::BodyPart(i, BodyPart::BodyPoints) => {
            if let Some(points) = &mut project.bodies[i].body_points {
                section(ui, "Body Points", |ui| {
                    message(
                        ui,
                        "Body-local points where the body outputs (First Order → Body Pressure, Body Velocity) are given instead of at the panels: for every body or none. Edit them in the table.",
                    );
                    let mut n = points.len() as i64;
                    if integer(ui, ("points", i), "Count", &mut n, true) {
                        let fill = points.last().copied().unwrap_or([0.0; 3]);
                        points.resize(n.clamp(0, 100_000) as usize, fill);
                    }
                });
            }
            let b = &mut project.bodies[i];
            if b.body_points.is_some() {
                section(ui, "Interpolation", |ui| {
                    if integer(ui, ("points", i), "Neighbouring Panels", &mut b.body_point_neighbors, true) {
                        b.body_point_neighbors = b.body_point_neighbors.max(1);
                    }
                });
            }
        }
        Node::BodyPart(i, BodyPart::Generalized(k)) => {
            let n = project.bodies[i].mesh.as_ref().map_or(0, |m| m.panels.len() as i64);
            let g = &mut project.bodies[i].generalized_modes[k];
            section(ui, &format!("Generalized Mode {}", k + 1), |ui| {
                message(
                    ui,
                    "A displacement mode after the six rigid ones, always free: u, v, w as polynomial terms c x^px y^py z^pz on records of the body mesh. Edit the terms in the table.",
                );
                text(ui, ("gen", i, k), "Name", &mut g.name);
                let parities: Vec<String> = DOF_LABELS.iter().map(|s| s.to_string()).collect();
                choice(ui, ("gen", i, k), "Parity", &parities, &mut g.parity);
                let mut count = g.pieces.len() as i64;
                if integer(ui, ("gen", i, k), "Pieces", &mut count, true) {
                    g.pieces.resize(count.clamp(1, 64) as usize, Default::default());
                }
            });
            section(ui, "Display", |ui| {
                message(
                    ui,
                    "The viewport shows the wetted hull displaced by Scale units of this coordinate (the engine's displacements; 0: off). Display only.",
                );
                number(ui, ("gen-view", i, k), "Scale", "", &mut g.view_scale, true);
            });
            for (j, piece) in g.pieces.iter_mut().enumerate() {
                let title = format!("Piece {}", j + 1);
                section(ui, &title, |ui| {
                    let mut ranged = piece.range.is_some();
                    if check(ui, "Panel Range", &mut ranged, Some("Off: every record of the body mesh.")) {
                        piece.range = ranged.then_some([1, n.max(1)]);
                    }
                    let mut r = piece.range.unwrap_or([1, n.max(1)]);
                    let mut changed = integer(ui, (title.as_str(), i, k), "Start Panel", &mut r[0], ranged);
                    changed |= integer(ui, (title.as_str(), i, k), "End Panel", &mut r[1], ranged);
                    ranges_row(ui, (title.as_str(), i, k), "Further Ranges", &mut piece.more_ranges, ranged);
                    if changed && ranged {
                        piece.range = Some(r);
                    }
                    for (c, name) in ["u Terms", "v Terms", "w Terms"].iter().enumerate() {
                        let mut t = piece.terms[c].len() as i64;
                        if integer(ui, (title.as_str(), i, k), name, &mut t, true) {
                            piece.terms[c].resize(t.clamp(0, 64) as usize, [0.0; 4]);
                        }
                    }
                });
            }
        }
        Node::BodyPart(i, BodyPart::Coupling(k)) => {
            let others: Vec<String> =
                project.bodies.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, b)| b.name.clone()).collect();
            let c = &mut project.bodies[i].couplings[k];
            section(ui, &format!("Coupling {}", k + 1), |ui| {
                message(
                    ui,
                    "The external stiffness and damping of this body's forces against the motions of another body (6 x 6 each, each about the origin of that body's additional matrices). Give the other direction on the other body.",
                );
                check(ui, "Enabled", &mut c.enabled, None);
                let mut index = others.iter().position(|n| *n == c.body).unwrap_or(usize::MAX);
                if choice(ui, ("coupling", i, k), "Other Body", &others, &mut index)
                    && let Some(name) = others.get(index)
                {
                    c.body = name.clone();
                }
                if c.body.is_empty() || !others.contains(&c.body) {
                    message(ui, "Choose the other body.");
                }
                let options = ["Stiffness".to_string(), "Damping".to_string()];
                choice(ui, ("coupling", i, k), "Matrix", &options, &mut c.shown);
            });
            let (m, title, unit) = if c.shown == 1 {
                (&mut c.damping, "Coupling Damping", "N·s/m")
            } else {
                (&mut c.stiffness, "Coupling Stiffness", "N/m")
            };
            section(ui, title, |ui| matrix_entries(ui, (title, i, k), m, unit));
        }
        Node::BodyPart(i, BodyPart::ModeMatrices) => {
            let n = 6 + project.bodies[i].generalized_modes.len();
            if let Some(m) = &mut project.bodies[i].mode_matrices {
                if m.mass.len() != n * n {
                    m.resize(n);
                }
                section(ui, "Mode Matrices", |ui| {
                    message(
                        ui,
                        "The (6 + G) x (6 + G) matrices of the rigid (about R) and generalized coordinates; the rigid block of the mass is zero (the body's mass and inertia). Edit them in the table.",
                    );
                    read_only(ui, "Size", &format!("{n} x {n}"));
                    let options = ["Mass".to_string(), "Damping".to_string(), "Stiffness".to_string()];
                    choice(ui, ("modes", i), "Table Shows", &options, &mut m.shown);
                });
            }
        }
        Node::FixedDamper(k) => {
            let library = library(project);
            let d = &mut project.fixed_dampers[k];
            section(ui, &format!("Fixed Damper {}", k + 1), |ui| {
                message(ui, "A porous sheet of the water, at rest, in global coordinates.");
                check(ui, "Enabled", &mut d.enabled, None);
                mesh_rows(ui, &library, &mut d.mesh, MeshTarget::DipoleDamper(k), actions);
                number(ui, ("water-damper", k), "Damping Factor", "m/s", &mut d.damping_factor, true);
            });
        }
        Node::BodyPart(i, BodyPart::Pressure(k)) => {
            let library = library(project);
            surface(
                ui,
                &library,
                &[],
                &mut project.bodies[i].pressure_surfaces[k],
                i,
                SurfaceKind::Pressure(k),
                actions,
            )
        }
        Node::Lid(i) => lid(ui, project, i, actions),
        Node::Headings => section(ui, "Headings", |ui| {
            let h = &mut project.waves.headings;
            number(ui, "headings", "Start", "deg", &mut h.start, true);
            number(ui, "headings", "End", "deg", &mut h.end, true);
            integer(ui, "headings", "Count", &mut h.count, true);
        }),
        Node::Frequencies => frequencies(ui, project),
        Node::FieldPoints => field_points(ui, project, derived),
        Node::Result(_) => {}
    }
}
