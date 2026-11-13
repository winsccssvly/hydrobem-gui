//! The dialogs.

use eframe::egui::{self, Id, Key, RichText};

use super::{App, FileAction, Modal};
use crate::style;

impl App {
    /// The discretization of higher-order GDF files without an SPL file: a panel size (WAMIT's
    /// PANEL_SIZE) or spans per patch, and the spline and Gauss orders (KSPLIN, IQUADO, IQUADI).
    fn discretization_modal(&mut self, ctx: &egui::Context) {
        let Some(Modal::Discretization { files, mut choice }) = self.modal.take() else { return };
        let (mut read, mut cancel) = (false, false);
        egui::Modal::new(Id::unique("discretization")).show(ctx, |ui| {
            ui.set_max_width(480.0);
            ui.heading("Higher-Order GDF: Discretization");
            ui.add_space(6.0);
            let names: Vec<String> =
                files.iter().map(|(p, _)| p.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect();
            ui.label(format!(
                "{} {} patches but no SPL file beside {}. A GDF file holds only the geometry: choose the \
                 discretization of the potential on every patch (WAMIT takes it from its cfg file: \
                 PANEL_SIZE, KSPLIN, IQUADO, IQUADI). It can be changed afterwards in the mesh's table.",
                names.join(", "),
                if names.len() > 1 { "have" } else { "has" },
                if names.len() > 1 { "them" } else { "it" }
            ));
            ui.add_space(8.0);
            egui::Grid::new("discretization-grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.radio_value(&mut choice.by_panel_size, true, "Panel Size [m]");
                ui.add_enabled(
                    choice.by_panel_size,
                    egui::DragValue::new(&mut choice.panel_size).speed(0.01).range(1e-3..=1e6),
                );
                ui.end_row();
                ui.radio_value(&mut choice.by_panel_size, false, "Spans per Patch (nu x nv)");
                ui.add_enabled_ui(!choice.by_panel_size, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut choice.nu).range(1..=1000));
                        ui.label("x");
                        ui.add(egui::DragValue::new(&mut choice.nv).range(1..=1000));
                    });
                });
                ui.end_row();
                ui.label("Spline Order (KSPLIN)");
                ui.add(egui::DragValue::new(&mut choice.order).range(2..=6));
                ui.end_row();
                ui.label("Outer Gauss Order (IQUADO)");
                ui.add(egui::DragValue::new(&mut choice.outer).range(1..=16));
                ui.end_row();
                ui.label("Inner Gauss Order (IQUADI)");
                ui.add(egui::DragValue::new(&mut choice.inner).range(1..=16));
                ui.end_row();
            });
            ui.add_space(4.0);
            let note = if choice.by_panel_size {
                "Each patch gets n = floor(L / size) + 1 spans along each direction, L the longer of its two edges there."
            } else {
                "Every patch gets these spans."
            };
            ui.label(RichText::new(note).small().color(style::READ_ONLY));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                read = ui.button("Import").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if read {
            for (path, then) in files {
                self.read_mesh(&path, then, Some(&choice));
            }
        } else if !cancel {
            self.modal = Some(Modal::Discretization { files, choice });
        }
    }

    pub(super) fn modals(&mut self, ctx: &egui::Context) {
        if let Some(Modal::Discretization { .. }) = &self.modal {
            self.discretization_modal(ctx);
            return;
        }
        let Some(modal) = self.modal.take() else { return };
        let mut keep = true;
        let mut then: Option<FileAction> = None;
        let mut run_anyway = false;
        egui::Modal::new(Id::unique("modal")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            match &modal {
                Modal::Message { title, text } => {
                    ui.heading(title);
                    ui.add_space(6.0);
                    ui.label(text);
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        keep = false;
                    }
                }
                Modal::Unsaved(action) => {
                    ui.heading("Unsaved changes");
                    ui.add_space(6.0);
                    ui.label("The project has unsaved changes. Save them first?");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
                            keep = false;
                            if self.save(false) {
                                then = Some(action.clone());
                            }
                        }
                        if ui.button("Discard").clicked() {
                            keep = false;
                            then = Some(action.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                }
                Modal::Memory { text } => {
                    ui.heading("Memory");
                    ui.add_space(6.0);
                    ui.label(text);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Solve").clicked() {
                            keep = false;
                            run_anyway = true;
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                            // the model judged too large is not kept
                            self.pending_request = None;
                        }
                    });
                }
                Modal::StopAndExit => {
                    ui.heading("A solve is running");
                    ui.add_space(6.0);
                    ui.label("Stop the solve and exit? Its results are lost.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Stop and Exit").clicked() {
                            keep = false;
                            self.stop_and_exit();
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                }
                Modal::Discretization { .. } => unreachable!("shown apart"),
                Modal::About => {
                    let b = gui_core::hydrobem_engine::build_info();
                    let i = gui_core::engine_info::import();
                    ui.heading("HydroBEM");
                    ui.add_space(6.0);
                    ui.label(format!("GUI {} (egui + wgpu)", env!("CARGO_PKG_VERSION")));
                    ui.label(format!(
                        "Engine hydrobem.dll {} ({}) · C ABI {}",
                        b.version,
                        b.commit,
                        gui_core::hydrobem_engine::ABI_VERSION
                    ));
                    ui.label(format!("Imported from HydroBEM {}", i.source_describe));
                    ui.label(format!("{} · {}", b.linear_algebra, b.compiler));
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        keep = false;
                    }
                }
            }
        });
        if keep && self.modal.is_none() {
            self.modal = Some(modal);
        }
        if let Some(action) = then {
            self.perform(action);
            if self.allow_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if run_anyway && let Some(mut request) = self.pending_request.take() {
            // the user chose to solve: the engine must not refuse it for its memory estimate
            match gui_core::solve::without_memory_check(&mut request) {
                Ok(()) => self.start_solve(request, ctx),
                Err(e) => self.message("Cannot solve", e.message),
            }
        }
    }
}
