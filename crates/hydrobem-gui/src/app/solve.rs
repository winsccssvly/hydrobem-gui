//! Run/Stop, the solve and its progress.

use std::sync::Arc;

use eframe::egui::{self, Align, Color32, Frame, Id, Layout, RichText, Sense, Stroke, Ui, vec2};
use gui_core::format::{format_bytes, format_elapsed};
use gui_core::hydrobem_engine::{RunState, SolveProgress, WorkerState};
use gui_core::results::location_text;
use gui_core::solve::{SolveEvent, SolveHandle, SolveRequest, estimate_memory, make_solve_request};
use gui_core::workers;

use super::{App, Modal, Running};
use crate::results_view::Selection;
use crate::style;
use crate::tree::{Node, ResultNode};

impl App {
    // ---- Solving -------------------------------------------------------------------------------

    pub fn run(&mut self) {
        if self.is_solving() {
            if let Some(r) = &self.running {
                r.handle.cancel();
            }
            self.status = "Cancelling…".into();
        } else {
            // the next frame: an editor that has the focus commits its text first
            self.pending_run = 2;
        }
    }

    fn select_failing_body(&mut self, body: Option<usize>, names: &[String]) {
        if let Some(name) = body.and_then(|b| names.get(b))
            && let Some(i) = self.project.bodies.iter().position(|b| &b.name == name)
        {
            self.selected = Node::Body(i);
        }
    }

    pub(super) fn start_run(&mut self, ctx: &egui::Context) {
        let mut request = match make_solve_request(&self.project) {
            Ok(r) => r,
            Err(e) => {
                // the locations count every body of the model, the suppressed ones too
                let names: Vec<String> = self.project.bodies.iter().map(|b| b.name.clone()).collect();
                let at = e.location.as_ref().map(|l| location_text(l, &names)).unwrap_or_default();
                self.select_failing_body(e.location.as_ref().and_then(|l| l.body), &names);
                self.message(
                    "Cannot solve",
                    if at.is_empty() { e.message } else { format!("{}\n\nAt: {at}", e.message) },
                );
                return;
            }
        };
        if let Ok(estimate) = estimate_memory(&mut request) {
            let free = crate::system::available_memory();
            if free > 0 && estimate.peak as u64 > free {
                self.pending_request = Some(request);
                self.modal = Some(Modal::Memory {
                    text: format!(
                        "The engine estimates {} for this solve, but only {} of memory is free. Solve anyway?",
                        format_bytes(estimate.peak as f64),
                        format_bytes(free as f64)
                    ),
                });
                return;
            }
        }
        self.start_solve(request, ctx);
    }

    pub(super) fn start_solve(&mut self, request: SolveRequest, ctx: &egui::Context) {
        let body_names = request.model_bodies.clone();
        let ctx = ctx.clone();
        let handle = SolveHandle::start(request, move || ctx.request_repaint());
        let omegas: Vec<f64> =
            self.project.waves.frequencies.omegas().into_iter().filter(|w| w.is_finite() && *w > 0.0).collect();
        let threads = (self.project.options.frequency_threads.max(1) as usize).min(omegas.len().max(1));
        self.running = Some(Running {
            handle,
            progress: SolveProgress::default(),
            threads,
            omegas,
            body_names,
            solved_for: self.fingerprint.clone(),
        });
        self.finished = None;
        self.status = "Solving…".into();
        self.status_ok = true;
    }

    /// Stops the running solve and exits once it has stopped.
    pub(super) fn stop_and_exit(&mut self) {
        if let Some(running) = &self.running {
            running.handle.cancel();
        }
        self.exit_when_stopped = true;
        self.status = "Stopping the solve to exit…".into();
    }

    /// Whether the window will close (an exit was asked and allowed).
    pub fn will_close(&self) -> bool {
        self.allow_close
    }

    pub(super) fn poll_solve(&mut self) {
        let Some(running) = &mut self.running else { return };
        let mut finished = None;
        for event in running.handle.poll() {
            if let SolveEvent::Finished(outcome) = event {
                finished = Some(*outcome);
            }
        }
        // the engine's snapshot, read without waiting for the solve
        if let Some(p) = running.handle.progress() {
            running.progress = p;
        }
        let Some(outcome) = finished else { return };
        let running = self.running.take().expect("a running solve");
        self.show_frequencies = false;
        match outcome {
            Ok(result) => {
                let warnings = result.diagnostics.len();
                let frequencies = result.omegas().len();
                let route = result.route.clone();
                self.finished = Some(format!("Completed · {frequencies} frequencies"));
                self.status = if warnings > 0 {
                    format!("Solved with {warnings} engine warning(s) · {route}")
                } else {
                    format!("Solved successfully · {route}")
                };
                self.status_ok = true;
                self.result = Some(Arc::new(result));
                self.result_for = running.solved_for.clone();
                self.selection = Selection::new();
                self.selected = Node::Result(ResultNode::Summary);
            }
            Err(e) if e.cancelled => {
                self.finished = Some("Cancelled".into());
                self.status = "The solve was cancelled.".into();
            }
            Err(e) => {
                self.finished = Some("Failed".into());
                self.status = "The solve failed.".into();
                self.status_ok = false;
                let at = e.location.as_ref().map(|l| location_text(l, &running.body_names)).unwrap_or_default();
                self.select_failing_body(e.location.as_ref().and_then(|l| l.body), &running.body_names);
                self.message(
                    "The solve failed",
                    if at.is_empty() { e.message } else { format!("{}\n\nAt: {at}", e.message) },
                );
            }
        }
    }

    /// Forgets the last solve: the results views show "No results yet" until the next Run.
    pub fn clear_results(&mut self) {
        if self.result.take().is_none() {
            return;
        }
        self.selection = Selection::new();
        self.finished = None;
        self.viewport.highlight = None;
        self.status = "Results cleared.".into();
        self.status_ok = true;
        if matches!(self.selected, Node::Result(_)) {
            self.selected = Node::Result(ResultNode::Summary);
        }
    }
}

/// The Run button of the top bar: a green rounded button with a play icon; red with a stop icon
/// while solving.
pub(super) fn run_button(ui: &mut Ui, solving: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(78.0, 24.0), Sense::click());
    let fill = match (solving, resp.hovered()) {
        (true, _) => style::STOP,
        (false, true) => style::RUN_HOVER,
        (false, false) => style::RUN,
    };
    let p = ui.painter();
    p.rect_filled(rect, 4.0, fill);
    let c = egui::pos2(rect.left() + 18.0, rect.center().y);
    if solving {
        p.rect_filled(egui::Rect::from_center_size(c, vec2(9.0, 9.0)), 1.0, Color32::WHITE);
    } else {
        p.add(egui::Shape::convex_polygon(
            vec![c + vec2(-4.0, -5.5), c + vec2(5.5, 0.0), c + vec2(-4.0, 5.5)],
            Color32::WHITE,
            Stroke::NONE,
        ));
    }
    p.text(
        egui::pos2(rect.left() + 30.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        if solving { "Stop" } else { "Run" },
        egui::FontId::proportional(13.5),
        Color32::WHITE,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, if solving { "Stop" } else { "Run" }));
    resp.on_hover_text(if solving { "Stop the solve (F5)" } else { "Run the solve (F5)" })
}

/// The button left of Run while solving: opens the list of each frequency's progress.
pub(super) fn frequencies_button(ui: &mut Ui, open: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 24.0), Sense::click());
    let p = ui.painter();
    let fill = if open || resp.hovered() { style::HOVER } else { style::TOP_BAR };
    p.rect_filled(rect, 4.0, fill);
    p.rect_stroke(rect, 4.0, Stroke::new(1.0, style::GRID_LINE), egui::StrokeKind::Inside);
    for k in 0..3 {
        let y = rect.center().y + (k as f32 - 1.0) * 4.5;
        p.line_segment(
            [egui::pos2(rect.center().x - 6.0, y), egui::pos2(rect.center().x + 6.0, y)],
            Stroke::new(1.5, style::ARROW),
        );
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Frequency Progress"));
    resp.on_hover_text("The progress of each worker")
}

/// The fill of a running, a finished and a failed frequency's bar.
pub(super) const RUNNING_FILL: Color32 = Color32::from_rgb(0x9C, 0xC8, 0xEE);
const DONE_FILL: Color32 = Color32::from_rgb(0xA8, 0xDC, 0xB9);
const FAILED_FILL: Color32 = Color32::from_rgb(0xF2, 0xB8, 0xB5);

/// A flat progress bar: `fraction` of it filled, `text` in its middle.
pub(super) fn progress_bar(ui: &mut Ui, width: f32, fraction: f32, fill: Color32, text: &str) {
    let (bar, resp) = ui.allocate_exact_size(vec2(width, 16.0), Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ProgressIndicator, true, text));
    let p = ui.painter();
    p.rect_filled(bar, 3.0, style::SECTION);
    if fraction > 0.0 {
        let filled = egui::Rect::from_min_size(bar.min, vec2(bar.width() * fraction.min(1.0), bar.height()));
        p.rect_filled(filled, 3.0, fill);
    }
    p.text(bar.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(12.0), style::TEXT);
}

/// The progress of the solve below the Run button (its top right at `anchor`), by the engine's
/// frequency worker: what each is on (or Idle) and the share of its item done; the steps of the
/// second order once they begin, and the frequencies completed of all below.
pub(super) fn frequency_list(ctx: &egui::Context, anchor: egui::Pos2, running: &Running) {
    let f = &running.progress;
    const WORKER: f32 = 52.0;
    const STATUS: f32 = 330.0;
    const BAR: f32 = 120.0;
    egui::Area::new(Id::unique("frequency-progress"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(anchor)
        .show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(WORKER + STATUS + BAR + 16.0);
                let cell = |ui: &mut Ui, width: f32, add: &mut dyn FnMut(&mut Ui)| {
                    ui.allocate_ui_with_layout(vec2(width, 18.0), Layout::left_to_right(Align::Center), |ui| {
                        ui.set_min_width(width);
                        add(ui);
                    });
                };
                ui.horizontal(|ui| {
                    cell(ui, WORKER, &mut |ui| {
                        ui.label(RichText::new("Worker").strong());
                    });
                    cell(ui, STATUS, &mut |ui| {
                        ui.label(RichText::new("Status").strong());
                    });
                    cell(ui, BAR, &mut |ui| {
                        ui.label(RichText::new("Progress").strong());
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (k, w) in workers::worker_rows(f, running.threads).iter().enumerate() {
                        ui.horizontal(|ui| {
                            cell(ui, WORKER, &mut |ui| {
                                ui.label(format!("{}", k + 1));
                            });
                            let status = workers::worker_status(f, w, &running.omegas);
                            cell(ui, STATUS, &mut |ui| {
                                let text = RichText::new(status.as_str());
                                let text =
                                    if w.state == WorkerState::Running { text } else { text.color(style::READ_ONLY) };
                                ui.add(egui::Label::new(text).truncate());
                            });
                            let fraction = workers::worker_fraction(w);
                            let (text, color) = match w.state {
                                WorkerState::Running => (format!("{:.0}%", 100.0 * fraction), RUNNING_FILL),
                                WorkerState::Done => ("100%".to_string(), DONE_FILL),
                                WorkerState::Idle => (String::new(), style::SECTION),
                            };
                            progress_bar(ui, BAR, fraction as f32, color, &text);
                        });
                    }
                    if !f.second_order.is_empty() {
                        ui.separator();
                        ui.label(RichText::new("Second order").strong());
                        for s in &f.second_order {
                            ui.horizontal(|ui| {
                                cell(ui, WORKER, &mut |_| {});
                                cell(ui, STATUS, &mut |ui| {
                                    ui.add(egui::Label::new(workers::step_label(s.step)).truncate());
                                });
                                let (text, color) = match s.state {
                                    RunState::Done => ("100%".to_string(), DONE_FILL),
                                    RunState::Running => (format!("{:.0}%", 100.0 * s.fraction), RUNNING_FILL),
                                    RunState::Failed => ("failed".to_string(), FAILED_FILL),
                                    RunState::Cancelled => ("cancelled".to_string(), style::SECTION),
                                    RunState::Waiting => (String::new(), style::SECTION),
                                };
                                progress_bar(ui, BAR, s.fraction as f32, color, &text);
                            });
                        }
                    }
                });
                ui.separator();
                ui.label(format!("Completed {} of {} frequencies ({})", f.done, f.total, format_elapsed(f.seconds)));
                let note = "Shares are the engine's: each item's stages weighted by their cost. A worker is one \
                            of the engine's frequency threads (Frequency Threads), each with its inner threads.";
                ui.add(egui::Label::new(RichText::new(note).small().color(style::READ_ONLY)).wrap());
            });
        });
}

/// Clear Results: an outlined button beside Run, greyed without results.
pub(super) fn clear_button(ui: &mut Ui, enabled: bool) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(104.0, 24.0), sense);
    let p = ui.painter();
    if enabled && resp.hovered() {
        p.rect_filled(rect, 4.0, style::HOVER);
    }
    p.rect_stroke(rect, 4.0, Stroke::new(1.0, style::GRID_LINE), egui::StrokeKind::Inside);
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "Clear Results",
        egui::FontId::proportional(13.0),
        if enabled { style::TEXT } else { style::READ_ONLY },
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, "Clear Results"));
    resp.on_hover_text("Forget the last solve's results (Run again to recompute them)")
}
