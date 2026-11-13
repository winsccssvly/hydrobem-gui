//! The shortcuts, dropped files, the menu bar, the status bar and the window's title.

use std::path::PathBuf;

use eframe::egui::{self, Align, Color32, Id, Key, KeyboardShortcut, Layout, Modifiers, RichText, Stroke, Ui, vec2};
use gui_core::format::format_elapsed;
use gui_core::workers;

use super::solve::{RUNNING_FILL, clear_button, frequencies_button, frequency_list, progress_bar, run_button};
use super::{App, CAD_EXTENSIONS, Export, FileAction, Modal, OUTDATED, OUTDATED_TEXT};
use crate::output::OutputLayout;
use crate::style;

impl App {
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        let shortcut = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        if shortcut(Modifiers::COMMAND | Modifiers::SHIFT, Key::S) {
            self.save(true);
        } else if shortcut(Modifiers::COMMAND, Key::S) {
            self.save(false);
        }
        if shortcut(Modifiers::COMMAND, Key::N) {
            self.request(FileAction::New);
        }
        if shortcut(Modifiers::COMMAND, Key::O) {
            self.request(FileAction::Open(None));
        }
        if shortcut(Modifiers::NONE, Key::F5) {
            self.run();
        }
    }

    /// A model (.hbm) or results (.hbr) file dropped onto the window opens as from the File menu,
    /// with the unsaved prompt. A GDF or .hbg opens only on Meshes in the tree (imported) or on a
    /// Source File field (the field takes it); a CAD file (STEP, IGES) is not read: the status bar
    /// says that cad2bem, a program of its own, converts it.
    pub(super) fn dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        let extension = |p: &PathBuf| p.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
        // while such a file is dragged over the window: a frame and what a drop does
        let hovered: Vec<PathBuf> = ctx.input(|i| i.raw.hovered_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(path) = hovered.iter().find(|p| matches!(extension(p).as_deref(), Some("hbm" | "hbr"))) {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::unique("file-drop")));
            let rect = ctx.content_rect().shrink(6.0);
            painter.rect_filled(rect, 6.0, Color32::from_rgba_unmultiplied(0, 120, 215, 28));
            painter.rect_stroke(rect, 6.0, Stroke::new(2.0, style::SELECT), egui::StrokeKind::Inside);
            let what = if extension(path).as_deref() == Some("hbr") { "results" } else { "project" };
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("Drop to open the {what} {name}"),
                egui::FontId::proportional(18.0),
                style::SELECT,
            );
        }
        // a CAD file: cad2bem, a program of its own, converts it; its geometry file is imported
        if dropped.iter().any(|p| crate::widgets::has_extension(p, &CAD_EXTENSIONS)) {
            self.status = "A CAD file (STEP, IGES) is converted by cad2bem, a program of its own: import the \
                           geometry file (.hbg) it writes onto Meshes."
                .into();
            self.status_ok = false;
            return;
        }
        let Some(path) = dropped.iter().find(|p| matches!(extension(p).as_deref(), Some("hbm" | "hbr"))) else {
            if let Some(other) =
                dropped.iter().find(|p| !crate::widgets::has_extension(p, &gui_core::mesh::MESH_EXTENSIONS))
            {
                self.status = format!("{} is not a HydroBEM model (.hbm) or results (.hbr) file.", other.display());
            }
            return;
        };
        let path = path.clone();
        self.request(if extension(&path).as_deref() == Some("hbr") {
            FileAction::OpenResults(Some(path))
        } else {
            FileAction::Open(Some(path))
        });
    }

    pub(super) fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.menu_button("File", |ui| {
                let solving = self.is_solving();
                if ui.add_enabled(!solving, egui::Button::new("New Project").shortcut_text("Ctrl+N")).clicked() {
                    self.request(FileAction::New);
                }
                if ui.add_enabled(!solving, egui::Button::new("Open Project…").shortcut_text("Ctrl+O")).clicked() {
                    self.request(FileAction::Open(None));
                }
                ui.add_enabled_ui(!solving, |ui| {
                    ui.menu_button("Open Recent", |ui| {
                        if self.settings.recent.is_empty() {
                            ui.label(RichText::new("No recent files").color(style::READ_ONLY));
                        }
                        for path in self.settings.recent.clone() {
                            let exists = path.is_file();
                            if ui.add_enabled(exists, egui::Button::new(path.display().to_string())).clicked() {
                                let results = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("hbr"));
                                self.request(if results {
                                    FileAction::OpenResults(Some(path))
                                } else {
                                    FileAction::Open(Some(path))
                                });
                            }
                        }
                        ui.separator();
                        if ui.button("Clear Recent Projects").clicked() {
                            self.settings.recent.clear();
                        }
                    });
                });
                ui.separator();
                if ui.add(egui::Button::new("Save").shortcut_text("Ctrl+S")).clicked() {
                    self.save(false);
                }
                if ui.add(egui::Button::new("Save As…").shortcut_text("Ctrl+Shift+S")).clicked() {
                    self.save(true);
                }
                ui.separator();
                if ui.add_enabled(!solving, egui::Button::new("Open Results…")).clicked() {
                    self.request(FileAction::OpenResults(None));
                }
                ui.separator();
                // the results as files: once there are results
                ui.add_enabled_ui(self.result.is_some(), |ui| {
                    ui.menu_button("Export", |ui| {
                        if ui.button("WAMIT (s)…").clicked() {
                            self.export(Export::WamitPeriod);
                        }
                        if ui.button("WAMIT (rad/s)…").clicked() {
                            self.export(Export::WamitOmega);
                        }
                        if ui.button("NumPy (.npz)…").clicked() {
                            self.export(Export::Npz);
                        }
                    });
                });
                ui.separator();
                if ui.button("Exit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Fit").clicked() {
                    self.viewport.camera.fit(self.viewport.scene.bounds);
                }
                if ui.button("Reset Camera").clicked() {
                    self.viewport.camera.set_view(crate::viewport::camera::View::Iso, self.viewport.scene.bounds);
                }
                ui.checkbox(&mut self.settings.show_field_points, "Show Field Points");
                ui.checkbox(&mut self.settings.show_free_surface, "Show Free Surface");
                ui.checkbox(&mut self.settings.show_normals, "Show Normals")
                    .on_hover_text("A mesh shown alone (select it under Meshes): an arrow along each panel's or patch's normal into the fluid, as the engine takes it (a panel: its vertex order; a patch: the engine's surface normal at its middle)");
                ui.checkbox(&mut self.settings.show_spans, "Show Spans")
                    .on_hover_text("A patch mesh shown alone: the curves between the nu x nv spans of each patch's discretization");
                ui.menu_button("Output Layout", |ui| {
                    ui.radio_value(&mut self.settings.output_layout, OutputLayout::Split, "Table + Graph");
                    ui.radio_value(&mut self.settings.output_layout, OutputLayout::Tabs, "Tabs");
                });
            });
            ui.menu_button("Solve", |ui| {
                let solving = self.is_solving();
                if ui.add(egui::Button::new(if solving { "Stop" } else { "Run" }).shortcut_text("F5")).clicked() {
                    self.run();
                }
                if ui.add_enabled(self.result.is_some() && !solving, egui::Button::new("Clear Results")).clicked() {
                    self.clear_results();
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About HydroBEM").clicked() {
                    self.modal = Some(Modal::About);
                }
            });
            // at the right end: the Run button (Stop while solving), then Clear Results
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(6.0);
                let solving = self.is_solving();
                if clear_button(ui, self.result.is_some() && !solving).clicked() {
                    self.clear_results();
                }
                ui.add_space(4.0);
                let run = run_button(ui, solving);
                if run.clicked() {
                    self.run();
                }
                if let Some(running) = &self.running {
                    ui.add_space(4.0);
                    if frequencies_button(ui, self.show_frequencies).clicked() {
                        self.show_frequencies = !self.show_frequencies;
                    }
                    if self.show_frequencies {
                        frequency_list(ui.ctx(), run.rect.right_bottom() + vec2(0.0, 4.0), running);
                    }
                }
            });
        });
    }

    pub(super) fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let b = gui_core::hydrobem_engine::build_info();
            ui.label(
                RichText::new(format!("HydroBEM {} · API {}", gui_core::engine_info::engine_label(), b.api_version))
                    .color(if b.avx2 { style::BUILD_OK } else { style::ERROR }),
            )
            .on_hover_text(format!(
                "{}\n{}\n{} {}",
                b.compiler,
                b.linear_algebra,
                b.build_type,
                if b.avx2 { "AVX2" } else { "" }
            ));
            ui.label(RichText::new(&self.status).color(if self.status_ok { style::TEXT } else { style::ERROR }));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(running) = &self.running {
                    ui.label(format_elapsed(running.handle.started.elapsed().as_secs_f64()));
                    // the phase of the solve and its share; each worker's is in the list beside Run
                    let (fraction, text) = workers::phase_status(&running.progress);
                    ui.label(text);
                    progress_bar(ui, 180.0, fraction as f32, RUNNING_FILL, "");
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                } else if self.results_outdated() {
                    ui.label(RichText::new(OUTDATED).color(style::CAUTION)).on_hover_text(OUTDATED_TEXT);
                } else if let Some(done) = &self.finished {
                    ui.label(done);
                }
            });
        });
    }

    pub(super) fn window_title(&mut self, ctx: &egui::Context) {
        let name = self.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
        let title = match name {
            Some(n) => format!("{n} — HydroBEM"),
            None => "HydroBEM".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

/// Where files dragged over the window from another program are, for the drop targets: the
/// system's cursor (the window gets no mouse moves during such a drag), else egui's pointer.
pub(super) fn file_drag(ctx: &egui::Context) {
    let (dragging, hovering) = ctx.input(|i| (!i.raw.dropped_files.is_empty(), !i.raw.hovered_files.is_empty()));
    let pos = (dragging || hovering).then(|| {
        let ppp = ctx.pixels_per_point();
        let window = ctx.input(|i| i.viewport().inner_rect);
        window
            .zip(crate::system::cursor_position())
            .map(|(r, (x, y))| egui::pos2(x as f32 / ppp, y as f32 / ppp) - r.min.to_vec2())
            .or_else(|| ctx.input(|i| i.pointer.hover_pos()))
    });
    crate::widgets::set_file_drag_pos(ctx, pos.flatten());
    if hovering {
        // no events arrive while the files are dragged: follow the cursor
        ctx.request_repaint();
    }
}
