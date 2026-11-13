//! The 3D viewport (REQUIREMENTS.md, Field points and viewport): the scene drawn by wgpu, the
//! camera, the colour legend at the top left, the views on the right-click menu, the animation
//! player at the bottom left, the colour bar of a heatmap at the top right, and
//! field-point picking.

pub mod camera;
pub mod renderer;

use std::sync::Arc;

use eframe::egui::{
    self, Align2, Color32, FontId, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, UiBuilder, pos2, vec2,
};
use eframe::egui_wgpu;
use gui_core::format::format_number;
use gui_core::scene::{self, LineVertex, Scene};

use crate::style;
use camera::{Camera, View};

/// The animation's player at the bottom of the viewport: one period, repeated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    /// rad/s
    pub omega: f64,
    /// ωt in 0..2π
    pub phase: f64,
    pub paused: bool,
}

pub struct Viewport {
    /// Shown while an animation is on; the bar's buttons and drags change it in place.
    pub player: Option<Player>,
    pub camera: Camera,
    pub scene: Arc<Scene>,
    pub scene_id: u64,
    /// The highlighted field point (an index into the scene's field points).
    pub highlight: Option<usize>,
    /// The highlighted stored records of a mesh shown alone (zero-based, sorted).
    pub records: Vec<usize>,
    /// The record clicked in this frame's `show`.
    pub picked_record: Option<usize>,
    /// A click in this frame's `show` that hit neither a field point nor a record: the highlights
    /// are cleared, and the table's selection with them.
    pub clicked_away: bool,
    /// Fit the camera to the next scene (after a structural change).
    pub refit: bool,
    /// Why the engine cannot build the problem; the surfaces are then the GUI's drawing.
    pub problem: Option<String>,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            player: None,
            camera: Camera::default(),
            scene: Arc::new(Scene::default()),
            scene_id: 0,
            highlight: None,
            records: Vec::new(),
            picked_record: None,
            clicked_away: false,
            refit: true,
            problem: None,
        }
    }
}

impl Viewport {
    /// A new scene; the camera is kept unless a refit was asked for.
    pub fn set_scene(&mut self, scene: Scene) {
        self.scene_id += 1;
        if self.refit && scene.has_content {
            self.camera.set_view(View::Iso, scene.bounds);
            self.refit = false;
        }
        self.scene = Arc::new(scene);
    }

    /// Draws the viewport; returns the field point clicked, if any.
    pub fn show(&mut self, ui: &mut Ui) -> Option<usize> {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let delta = resp.drag_delta();
        // left drag rotates; right drag pans (a right click without a drag opens the menu), so do
        // middle drag and left drag with Shift
        let shift = ui.input(|i| i.modifiers.shift);
        if resp.dragged_by(egui::PointerButton::Secondary)
            || resp.dragged_by(egui::PointerButton::Middle)
            || (shift && resp.dragged_by(egui::PointerButton::Primary))
        {
            self.camera.pan(delta.x, delta.y);
        } else if resp.dragged_by(egui::PointerButton::Primary) {
            self.camera.rotate(delta.x, delta.y);
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.camera.zoom(scroll);
            }
        }
        let bounds = self.scene.bounds;
        resp.context_menu(|ui| {
            if ui.button("Fit").clicked() {
                self.camera.fit(bounds);
                ui.close();
            }
            ui.separator();
            for (label, view) in
                [("Iso", View::Iso), ("Front", View::Front), ("Top", View::Top), ("Right", View::Right)]
            {
                if ui.button(label).clicked() {
                    self.camera.set_view(view, bounds);
                    ui.close();
                }
            }
        });
        let aspect = f64::from(rect.width().max(1.0) / rect.height().max(1.0));
        let m = self.camera.view_projection(aspect);
        let mut picked = None;
        self.picked_record = None;
        self.clicked_away = false;
        if resp.clicked_by(egui::PointerButton::Primary)
            && let Some(p) = resp.interact_pointer_pos()
        {
            let local = p - rect.min;
            picked = camera::pick(&m, &self.scene.field_points, local.x, local.y, rect.width(), rect.height());
            if picked.is_some() {
                self.highlight = picked;
            } else if let Some(r) =
                camera::pick_record(&m, &self.scene.records, local.x, local.y, rect.width(), rect.height())
            {
                self.records = vec![r];
                self.picked_record = Some(r);
            } else {
                // empty space: nothing stays picked
                self.highlight = None;
                self.records.clear();
                self.clicked_away = true;
            }
        }

        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
        let look = self.camera.look();
        let light = {
            let l = [-look[0], -look[1], -look[2] + 0.35];
            let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
            [(l[0] / n) as f32, (l[1] / n) as f32, (l[2] / n) as f32]
        };
        let eye = self.camera.eye().map(|v| v as f32);
        let bg = style::VIEWPORT;
        let linear = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        let callback = renderer::ViewportCallback {
            scene: self.scene.clone(),
            scene_id: self.scene_id,
            view_projection: m,
            eye,
            light,
            background: [linear(bg.r()), linear(bg.g()), linear(bg.b())],
            highlight: self.highlight_lines(),
            highlight_faces: self.record_faces(),
            size,
        };
        ui.painter().add(egui_wgpu::Callback::new_paint_callback(rect, callback));

        self.overlays(ui, rect);
        picked
    }

    /// The quadrilaterals of the highlighted records, every symmetry copy.
    fn record_quads(&self) -> impl Iterator<Item = &[[f64; 3]; 4]> {
        self.scene.records.iter().filter(|(r, _)| self.records.binary_search(r).is_ok()).map(|(_, q)| q)
    }

    /// The highlighted records filled over the mesh.
    fn record_faces(&self) -> Vec<scene::Vertex> {
        let color = [0.96, 0.50, 0.08, 0.62];
        self.record_quads()
            .flat_map(|q| {
                let d1 = [q[2][0] - q[0][0], q[2][1] - q[0][1], q[2][2] - q[0][2]];
                let d2 = [q[3][0] - q[1][0], q[3][1] - q[1][1], q[3][2] - q[1][2]];
                let n = [d1[1] * d2[2] - d1[2] * d2[1], d1[2] * d2[0] - d1[0] * d2[2], d1[0] * d2[1] - d1[1] * d2[0]];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-30);
                let normal = n.map(|v| (v / len) as f32);
                [0, 1, 2, 0, 2, 3].map(|k| scene::Vertex { position: q[k].map(|v| v as f32), normal, color })
            })
            .collect()
    }

    /// A red square around the highlighted field point; the outlines of the highlighted panels.
    fn highlight_lines(&self) -> Vec<LineVertex> {
        let mut lines = Vec::new();
        if self.scene.record_outlines {
            let red = [0.86, 0.12, 0.12, 1.0];
            for q in self.record_quads() {
                for k in 0..4 {
                    for p in [q[k], q[(k + 1) % 4]] {
                        lines.push(LineVertex { position: p.map(|v| v as f32), color: red });
                    }
                }
            }
        }
        let Some(p) = self.highlight.and_then(|i| self.scene.field_points.get(i)) else { return lines };
        let h = (self.scene.marker_size * 2.2) as f32;
        let (x, y, z) = (p[0] as f32, p[1] as f32, p[2] as f32 + h * 0.05);
        let red = [0.86, 0.12, 0.12, 1.0];
        let c = [[x - h, y - h], [x + h, y - h], [x + h, y + h], [x - h, y + h]];
        lines.extend((0..4).flat_map(|i| {
            let (a, b) = (c[i], c[(i + 1) % 4]);
            [LineVertex { position: [a[0], a[1], z], color: red }, LineVertex { position: [b[0], b[1], z], color: red }]
        }));
        lines
    }

    fn overlays(&mut self, ui: &mut Ui, rect: Rect) {
        let card = Frame::new()
            .fill(Color32::from_rgba_unmultiplied(0xF8, 0xFA, 0xFC, 235))
            .stroke(Stroke::new(1.0, style::GRID_LINE))
            .inner_margin(Margin::symmetric(8, 6));

        // the colour legend at the top left (the views are on the right-click menu)
        if !self.scene.legend.is_empty() {
            let legend = Rect::from_min_size(rect.min + vec2(10.0, 10.0), vec2(rect.width() - 160.0, 40.0));
            ui.scope_builder(UiBuilder::new().max_rect(legend), |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                card.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let swatch = |c: [u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
                        for (k, &(name, color)) in self.scene.legend.iter().enumerate() {
                            if k > 0 {
                                ui.add_space(6.0);
                            }
                            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                            ui.painter().rect_filled(r, 0.0, swatch(color));
                            ui.label(RichText::new(name).color(swatch(color)));
                        }
                    });
                });
            });
        }

        if let Some(player) = self.player.as_mut() {
            let bar = Rect::from_min_size(
                pos2(rect.left() + 10.0, rect.bottom() - 48.0),
                vec2((rect.width() - 20.0).min(760.0), 38.0),
            );
            ui.scope_builder(UiBuilder::new().max_rect(bar), |ui| {
                card.show(ui, |ui| player_bar(ui, player));
            });
        }

        if let Some(problem) = self.problem.as_ref().filter(|_| self.scene.has_content) {
            let bottom = if self.player.is_some() { rect.bottom() - 56.0 } else { rect.bottom() - 10.0 };
            let width = (rect.width() - 20.0).min(760.0);
            let note =
                Rect::from_min_max(pos2(rect.left() + 10.0, bottom - 80.0), pos2(rect.left() + 10.0 + width, bottom));
            ui.scope_builder(UiBuilder::new().max_rect(note).layout(egui::Layout::bottom_up(egui::Align::Min)), |ui| {
                card.show(ui, |ui| {
                    ui.set_max_width(width - 16.0);
                    let text = format!(
                        "The engine cannot build the problem: {problem}. The surfaces are drawn from the inputs, not by the engine."
                    );
                    ui.add(egui::Label::new(RichText::new(text).color(style::ERROR)).wrap());
                });
            });
        }
        if !self.scene.has_content {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Drop a GDF or .hbg file onto Meshes in the tree, or right-click Meshes to import; then right-click Bodies → New Body.",
                FontId::proportional(14.0),
                style::READ_ONLY,
            );
        }
        if let Some(scale) = &self.scene.color_scale {
            color_bar(ui, rect, scale);
        }
    }
}

/// A square icon button of the player.
fn icon_button(ui: &mut Ui, tooltip: &str, paint: impl FnOnce(&egui::Painter, Rect, Color32)) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 3.0, style::HOVER);
    }
    paint(ui.painter(), r.shrink(7.0), style::TEXT);
    resp.on_hover_text(tooltip).clicked()
}

/// Play/pause and stop, then one period as a bar: a click or a drag moves to that phase; the time
/// within the period at the right. At the end of the period it starts again.
fn player_bar(ui: &mut Ui, player: &mut Player) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let paused = player.paused;
        let toggled = icon_button(ui, if paused { "Play" } else { "Pause" }, |p, r, c| {
            if paused {
                p.add(egui::Shape::convex_polygon(
                    vec![r.left_top(), r.right_center(), r.left_bottom()],
                    c,
                    Stroke::NONE,
                ));
            } else {
                let w = r.width() / 3.0;
                p.rect_filled(Rect::from_min_size(r.left_top(), vec2(w, r.height())), 0.0, c);
                p.rect_filled(Rect::from_min_size(r.right_top() - vec2(w, 0.0), vec2(w, r.height())), 0.0, c);
            }
        });
        if toggled {
            player.paused = !player.paused;
        }
        if icon_button(ui, "Stop (back to the start)", |p, r, c| {
            p.rect_filled(r, 0.0, c);
        }) {
            player.paused = true;
            player.phase = 0.0;
        }
        ui.add_space(6.0);
        let tau = std::f64::consts::TAU;
        let period = if player.omega > 0.0 { tau / player.omega } else { 0.0 };
        let text = format!(
            "t = {:.2} s / T = {:.2} s · {:.0}°",
            player.phase / tau * period,
            period,
            player.phase.to_degrees()
        );
        let text_width = 190.0;
        let width = (ui.available_width() - text_width - 8.0).max(60.0);
        let (track, resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click_and_drag());
        if let Some(pos) = resp.interact_pointer_pos()
            && (resp.dragged() || resp.clicked() || resp.drag_started())
        {
            let t = f64::from(((pos.x - track.left()) / track.width()).clamp(0.0, 1.0));
            player.phase = (t * tau).min(tau - 1e-9);
        }
        let p = ui.painter();
        let y = track.center().y;
        let line = Rect::from_min_max(pos2(track.left(), y - 2.0), pos2(track.right(), y + 2.0));
        p.rect_filled(line, 2.0, style::GRID_LINE);
        let x = track.left() + track.width() * (player.phase / tau) as f32;
        p.rect_filled(Rect::from_min_max(line.min, pos2(x, line.max.y)), 2.0, style::SELECT);
        let knob = if resp.hovered() || resp.dragged() { 7.0 } else { 6.0 };
        p.circle(pos2(x, y), knob, Color32::WHITE, Stroke::new(2.0, style::SELECT));
        ui.add_sized(vec2(text_width, 24.0), egui::Label::new(RichText::new(text).color(style::TEXT)).truncate());
    });
}

/// The colour bar of the heatmap: the quantity, its unit, the levels used and the displayed range.
fn color_bar(ui: &Ui, rect: Rect, scale: &scene::ColorScale) {
    // as wide as its label and its note need (a note such as "interpolated between points")
    let p = ui.painter();
    let width =
        |text: &str, size: f32| p.layout_no_wrap(text.to_string(), FontId::proportional(size), style::TEXT).size().x;
    let w = (width(&scale.label, 12.0).max(width(scale.note, 11.0)) + 16.0).max(118.0);
    let card = Rect::from_min_size(pos2(rect.right() - 10.0 - w, rect.top() + 10.0), vec2(w, 210.0));
    p.rect_filled(card, 2.0, Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, 235));
    p.rect_stroke(card, 2.0, Stroke::new(1.0, style::GRID_LINE), egui::StrokeKind::Inside);
    let font = FontId::proportional(12.0);
    p.text(card.left_top() + vec2(8.0, 6.0), Align2::LEFT_TOP, &scale.label, font.clone(), style::TEXT);
    let bar = Rect::from_min_size(card.left_top() + vec2(8.0, 26.0), vec2(14.0, 150.0));
    let levels = scale.levels.max(2);
    for i in 0..levels {
        let t = i as f64 / (levels - 1) as f64;
        let c = scene::heat_color(t);
        let h = bar.height() / levels as f32;
        let y = bar.bottom() - (i + 1) as f32 * h;
        p.rect_filled(
            Rect::from_min_size(pos2(bar.left(), y), vec2(bar.width(), h + 0.5)),
            0.0,
            Color32::from_rgb(c[0], c[1], c[2]),
        );
    }
    p.rect_stroke(bar, 0.0, Stroke::new(1.0, style::ARROW), egui::StrokeKind::Outside);
    let tick = |y: f32, v: f64| {
        p.line_segment([pos2(bar.right(), y), pos2(bar.right() + 4.0, y)], Stroke::new(1.0, style::ARROW));
        p.text(pos2(bar.right() + 7.0, y), Align2::LEFT_CENTER, format_number(v), font.clone(), style::TEXT);
    };
    tick(bar.top(), scale.max);
    tick(bar.center().y, (scale.min + scale.max) / 2.0);
    tick(bar.bottom(), scale.min);
    // the range is the solver values' either way; the note says what the colours between are
    let note = scale.note;
    p.text(
        pos2(card.left() + 8.0, card.bottom() - 8.0),
        Align2::LEFT_BOTTOM,
        note,
        FontId::proportional(11.0),
        style::READ_ONLY,
    );
}
