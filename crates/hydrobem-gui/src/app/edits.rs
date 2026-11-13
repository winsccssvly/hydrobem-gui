//! The tree's and Details' actions, the engine's view of the inputs, the viewport's scene.

use std::path::{Path, PathBuf};

use gui_core::scene::{Focus, SceneInputs, build_scene};

use super::files::mesh_dialog;
use super::{App, MeshImport};
use crate::details::{Derived, DetailsAction};
use crate::tree::{BodyPart, NewPart, Node, TreeAction};

impl App {
    /// The hull of the selected generalized mode displaced by its view scale, when it has one.
    fn mode_shape(&self) -> Option<gui_core::scene::ModeShape> {
        let Node::BodyPart(i, BodyPart::Generalized(k)) = self.selected else { return None };
        let scale = self.project.bodies.get(i)?.generalized_modes.get(k)?.view_scale;
        if scale == 0.0 {
            return None;
        }
        let shape = gui_core::engine_model::mode_shape(&self.project, i, k).ok()?;
        let quads = shape
            .into_iter()
            .map(|(q, d)| std::array::from_fn(|v| std::array::from_fn(|a| q[v][a] + scale * d[v][a])))
            .collect();
        Some(gui_core::scene::ModeShape { quads })
    }

    /// The engine's view of the inputs, recomputed when they change.
    pub(super) fn refresh_derived(&mut self) {
        if self.derived_for != self.fingerprint {
            self.derived = Derived::compute(&self.project);
            self.derived_for = self.fingerprint.clone();
        }
    }

    /// What the viewport shows for the selected node: a mesh alone, a body or lid alone, nothing
    /// for the mesh library (its meshes overlap), everything otherwise.
    fn focus(&self) -> Focus {
        match self.selected {
            Node::Meshes => Focus::Nothing,
            Node::Mesh(i) => Focus::Mesh(i),
            Node::Body(i) | Node::BodyPart(i, _) => Focus::Body(i),
            Node::Lid(i) => Focus::Lid(i),
            _ => Focus::All,
        }
    }

    pub(super) fn refresh_scene(
        &mut self,
        overlay: Option<&gui_core::scene::FieldOverlay>,
        body_overlay: Option<&gui_core::scene::BodyOverlay>,
        animated: Option<&gui_core::animation::Response>,
    ) {
        let amplitude = self.selection.display_scale;
        let animation =
            animated.map(|response| gui_core::animation::Frame { response, phase: self.animation_phase, amplitude });
        let focus = self.focus();
        if focus != self.focus {
            self.focus = focus;
            self.viewport.refit = true;
        }
        let hash = |label: &str, values: &[f64]| {
            format!("{label:?}{}", values.iter().fold(0u64, |a, v| a.wrapping_mul(31).wrapping_add(v.to_bits())))
        };
        let mode_view = match self.selected {
            Node::BodyPart(i, BodyPart::Generalized(k)) => {
                self.project.bodies.get(i).and_then(|b| b.generalized_modes.get(k)).map_or(0.0, |g| g.view_scale)
            }
            _ => 0.0,
        };
        let animation_key = animation.map(|f| {
            let values = f.response.field.as_ref().map(|x| x.values.as_slice()).unwrap_or(&[]);
            let mut field: f64 = values.iter().map(|v| v.re * 1.3 + v.im).sum();
            if let Some(b) = &f.response.body_values {
                field += b.values.iter().flatten().map(|v| v.re * 1.7 + v.im).sum::<f64>();
                field += b.component.map_or(0.5, |c| c as f64);
            }
            format!("{}|{}|{}|{:?}|{field}", f.phase, f.amplitude, f.response.omega, f.response.bodies)
        });
        let key = format!(
            "{:?}|{animation_key:?}|{mode_view}|{}|{}|{}|{}|{}|{}|{}|{}",
            focus,
            self.fingerprint,
            self.settings.smoothing,
            self.settings.show_field_points,
            self.settings.show_free_surface,
            self.settings.show_normals,
            self.settings.show_spans,
            overlay.map(|o| hash(&o.label, &o.values)).unwrap_or_default(),
            body_overlay.map(|o| hash(&o.label, &o.values)).unwrap_or_default(),
        );
        if key == self.scene_for {
            return;
        }
        let markers =
            if self.settings.show_field_points { Some(self.derived.field_points.points.as_slice()) } else { None };
        let mode_shape = self.mode_shape();
        let scene = build_scene(
            &self.project,
            &SceneInputs {
                focus,
                engine: self.derived.roles.as_ref().ok(),
                overlay,
                smoothing: self.settings.smoothing,
                markers,
                body_overlay,
                free_surface: self.settings.show_free_surface,
                normals: self.settings.show_normals,
                spans: self.settings.show_spans,
                mode_shape: mode_shape.as_ref(),
                animation,
            },
        );
        if overlay.is_none() {
            self.viewport.highlight = None;
        }
        self.viewport.set_scene(scene);
        self.scene_for = key;
    }

    pub(super) fn apply_tree(&mut self, actions: Vec<TreeAction>) {
        for action in actions {
            match action {
                TreeAction::Select(node) => self.selected = node,
                TreeAction::ImportMesh => {
                    self.apply_details(vec![DetailsAction::ImportMesh(None, None)]);
                }
                // files dropped onto Meshes behind an open dialog wait for it
                TreeAction::ImportMeshFiles(_) if self.modal.is_some() => {}
                TreeAction::ImportMeshFiles(paths) => {
                    let before = self.project.meshes.len();
                    self.apply_details(paths.into_iter().map(|p| DetailsAction::ImportMesh(None, Some(p))).collect());
                    // the files read now (a higher-order GDF without SPL file waits for its question)
                    let count = self.project.meshes.len() - before;
                    if count > 1 {
                        self.status = format!("Imported {count} meshes; the last is selected.");
                    }
                }
                TreeAction::NewAnalyticMesh => self.new_analytic_mesh(),
                TreeAction::ExportMesh(i) => self.export_mesh(i, None),
                TreeAction::DeleteMesh(i) => {
                    if self.project.mesh_users(i).is_empty() {
                        self.project.meshes.remove(i);
                        self.selected = Node::Meshes;
                    }
                }
                TreeAction::AddPart(i, part) => {
                    let names: Vec<String> = self.project.bodies.iter().map(|b| b.name.clone()).collect();
                    let b = &mut self.project.bodies[i];
                    let node = match part {
                        NewPart::Damping => {
                            b.additional_damping.get_or_insert_with(Default::default);
                            BodyPart::Damping
                        }
                        NewPart::Stiffness => {
                            b.additional_stiffness.get_or_insert_with(Default::default);
                            BodyPart::Stiffness
                        }
                        NewPart::Dipole => {
                            b.dipoles.push(Default::default());
                            BodyPart::Dipole(b.dipoles.len() - 1)
                        }
                        NewPart::Pressure => {
                            b.pressure_surfaces.push(Default::default());
                            BodyPart::Pressure(b.pressure_surfaces.len() - 1)
                        }
                        NewPart::Plate => {
                            b.free_surface_plates.push(Default::default());
                            BodyPart::Plate(b.free_surface_plates.len() - 1)
                        }
                        NewPart::Damper => {
                            b.dipole_dampers.push(Default::default());
                            BodyPart::Damper(b.dipole_dampers.len() - 1)
                        }
                        NewPart::Tank => {
                            b.tanks.push(Default::default());
                            BodyPart::Tank(b.tanks.len() - 1)
                        }
                        NewPart::ControlSurface => {
                            b.control_surface.get_or_insert_with(Default::default);
                            BodyPart::ControlSurface
                        }
                        NewPart::BodyPoints => {
                            b.body_points.get_or_insert_with(|| vec![[0.0, 0.0, -0.5]]);
                            BodyPart::BodyPoints
                        }
                        NewPart::Generalized => {
                            let k = b.generalized_modes.len();
                            b.generalized_modes.push(gui_core::project::GeneralizedMode {
                                name: format!("Generalized_{}", k + 1),
                                ..Default::default()
                            });
                            if let Some(m) = &mut b.mode_matrices {
                                m.resize(6 + b.generalized_modes.len());
                            }
                            BodyPart::Generalized(k)
                        }
                        NewPart::ModeMatrices => {
                            let mut m = gui_core::project::ModeMatrices::default();
                            m.resize(6 + b.generalized_modes.len());
                            b.mode_matrices.get_or_insert(m);
                            BodyPart::ModeMatrices
                        }
                        NewPart::Coupling => {
                            // the first other body without a coupling yet
                            let other = names
                                .iter()
                                .find(|n| **n != b.name && b.couplings.iter().all(|c| &c.body != *n))
                                .cloned()
                                .unwrap_or_default();
                            b.couplings.push(gui_core::project::Coupling { body: other, ..Default::default() });
                            BodyPart::Coupling(b.couplings.len() - 1)
                        }
                    };
                    if let Some(open) = self.expanded.bodies.get_mut(i) {
                        *open = true;
                    }
                    self.selected = Node::BodyPart(i, node);
                }
                TreeAction::NewFixedDamper => {
                    self.project.fixed_dampers.push(gui_core::project::PanelSurface {
                        source: gui_core::project::SurfaceSource::ExternalMesh,
                        ..Default::default()
                    });
                    self.selected = Node::FixedDamper(self.project.fixed_dampers.len() - 1);
                }
                TreeAction::DeleteFixedDamper(k) => {
                    self.project.fixed_dampers.remove(k);
                    self.selected = Node::FixedDampers;
                }
                TreeAction::DeletePart(i, part) => {
                    let b = &mut self.project.bodies[i];
                    match part {
                        BodyPart::Damping => b.additional_damping = None,
                        BodyPart::Stiffness => b.additional_stiffness = None,
                        BodyPart::Dipole(k) => {
                            b.dipoles.remove(k);
                        }
                        BodyPart::Pressure(k) => {
                            b.pressure_surfaces.remove(k);
                        }
                        BodyPart::Plate(k) => {
                            b.free_surface_plates.remove(k);
                        }
                        BodyPart::Damper(k) => {
                            b.dipole_dampers.remove(k);
                        }
                        BodyPart::Tank(k) => {
                            b.tanks.remove(k);
                            let name = b.name.clone();
                            self.project.forget_tank(&name, k);
                        }
                        BodyPart::ControlSurface => b.control_surface = None,
                        BodyPart::BodyPoints => b.body_points = None,
                        BodyPart::Generalized(k) => {
                            b.generalized_modes.remove(k);
                            if let Some(m) = &mut b.mode_matrices {
                                m.resize(6 + b.generalized_modes.len());
                            }
                        }
                        BodyPart::ModeMatrices => b.mode_matrices = None,
                        BodyPart::Coupling(k) => {
                            b.couplings.remove(k);
                        }
                    }
                    self.project.update_mesh_properties();
                    self.selected = Node::Body(i);
                }
                TreeAction::NewBody => {
                    let i = self.project.add_body();
                    self.selected = Node::Body(i);
                }
                TreeAction::NewLid => {
                    let i = self.project.add_lid();
                    self.selected = Node::Lid(i);
                }
                TreeAction::DeleteBody(i) => {
                    let removed = self.project.bodies.remove(i);
                    self.project.forget_body(&removed.name);
                    if i < self.expanded.bodies.len() {
                        self.expanded.bodies.remove(i);
                    }
                    self.selected = Node::Bodies;
                    self.viewport.refit = true;
                }
                TreeAction::DeleteLid(i) => {
                    self.project.lids.remove(i);
                    self.selected = Node::Lids;
                    self.viewport.refit = true;
                }
            }
        }
    }

    /// A circular cylinder made in the GUI, added to the library and selected for its Details.
    pub fn new_analytic_mesh(&mut self) {
        match gui_core::analytic::AnalyticSpec::default().build(None) {
            Ok(mesh) => {
                let index = self.project.add_mesh("cylinder", mesh);
                self.selected = Node::Mesh(index);
                self.viewport.refit = true;
                self.status = format!("New analytic mesh {}", self.project.meshes[index].name);
            }
            Err(e) => self.message("Cannot make the mesh", e),
        }
    }

    /// Imports a mesh file into the library, as Import Mesh… does, and selects it.
    pub fn apply_import(&mut self, path: &Path) {
        self.apply_details(vec![DetailsAction::ImportMesh(None, Some(path.to_path_buf()))]);
    }

    /// Writes mesh `index` of the library as a geometry file (`.hbg`) at `path`, or where the
    /// user chooses.
    pub fn export_mesh(&mut self, index: usize, path: Option<PathBuf>) {
        let Some(entry) = self.project.meshes.get(index) else { return };
        let Some(path) = path else {
            let name = format!("{}.hbg", entry.name);
            return self.pick_then(
                move || {
                    rfd::FileDialog::new()
                        .add_filter("HydroBEM geometry", &["hbg"])
                        .set_title("Export Mesh")
                        .set_file_name(name)
                        .save_file()
                },
                move |app, p| app.export_mesh(index, Some(p)),
            );
        };
        match entry.mesh.save_hbg(&path) {
            Ok(()) => self.status = format!("Exported {} to {}", entry.name, path.display()),
            Err(e) => self.message("Cannot write the mesh", e),
        }
    }

    pub(super) fn apply_details(&mut self, actions: Vec<DetailsAction>) {
        for action in actions {
            match action {
                DetailsAction::ImportMesh(target, path) => {
                    if path.is_none() {
                        self.pick_then(
                            || mesh_dialog().set_title("Import Mesh").pick_file(),
                            move |app, p| app.apply_details(vec![DetailsAction::ImportMesh(target, Some(p))]),
                        );
                    }
                    if let Some(path) = path {
                        if gui_core::mesh::needs_discretization(&path) {
                            self.ask_discretization(path, MeshImport::Into(target));
                        } else {
                            self.read_mesh(&path, MeshImport::Into(target), None);
                        }
                    }
                }
                DetailsAction::ReplaceMesh(index, path) => {
                    if path.is_none() {
                        self.pick_then(
                            || mesh_dialog().set_title("Read Mesh Again").pick_file(),
                            move |app, p| app.apply_details(vec![DetailsAction::ReplaceMesh(index, Some(p))]),
                        );
                    }
                    if let Some(path) = path {
                        if gui_core::mesh::needs_discretization(&path) {
                            self.ask_discretization(path, MeshImport::Replace(index));
                        } else {
                            self.read_mesh(&path, MeshImport::Replace(index), None);
                        }
                    }
                }
                DetailsAction::RebuildAnalytic(index, spec) => {
                    let Some(entry) = self.project.meshes.get(index) else { continue };
                    match spec.rebuild(&entry.mesh) {
                        Ok(mesh) => {
                            self.project.replace_mesh(index, mesh);
                            self.viewport.refit = true;
                        }
                        Err(e) => self.status = format!("The shape is not accepted: {e}"),
                    }
                }
                DetailsAction::PanelSize(index, size) => {
                    let Some(entry) = self.project.meshes.get(index) else { continue };
                    let dir = self.file.as_deref().and_then(std::path::Path::parent);
                    match entry.mesh.with_panel_size(size, dir) {
                        Ok(mesh) => {
                            let spans: i64 = mesh.discretization().iter().map(|r| r[0] * r[1]).sum();
                            self.project.replace_mesh(index, mesh);
                            self.project.update_mesh_properties();
                            self.status = format!(
                                "The discretization from a panel size of {} m: {spans} spans over the stored patches.",
                                gui_core::format::format_number(size)
                            );
                        }
                        Err(e) => self.message("Panel Size", e),
                    }
                }
                DetailsAction::Refit => self.viewport.refit = true,
            }
        }
    }
}
