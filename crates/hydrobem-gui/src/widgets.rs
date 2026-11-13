//! The Details grid (sections of label | value rows), the tree row and the pane headers, drawn as
//! REQUIREMENTS.md describes them.

use eframe::egui::{
    self, Align, Align2, Color32, FontId, Frame, Id, Layout, Response, RichText, Sense, Stroke, TextEdit, Ui,
    UiBuilder, pos2, vec2,
};
use gui_core::format::{format_number, parse_number};

use crate::style;

pub const LABEL_WIDTH: f32 = 185.0;
pub const ROW_HEIGHT: f32 = 27.0;
const ERROR: Color32 = Color32::from_rgb(0xC0, 0x30, 0x30);

const FILE_DRAG_POS: &str = "file-drag-pos";

/// Where the files dragged over the window are this frame, for the drop targets (the app sets it:
/// the window gets no mouse moves during a drag from another program).
pub fn file_drag_pos(ctx: &egui::Context) -> Option<egui::Pos2> {
    ctx.data(|d| d.get_temp(Id::unique(FILE_DRAG_POS)))
}

pub fn set_file_drag_pos(ctx: &egui::Context, pos: Option<egui::Pos2>) {
    ctx.data_mut(|d| match pos {
        Some(p) => {
            d.insert_temp(Id::unique(FILE_DRAG_POS), p);
        }
        None => d.remove::<egui::Pos2>(Id::unique(FILE_DRAG_POS)),
    });
}

/// The files dropped this frame with this extension (lower case), when the drop is over `rect`.
pub fn dropped_on(ui: &Ui, rect: egui::Rect, extensions: &[&str]) -> Vec<std::path::PathBuf> {
    if !file_drag_pos(ui.ctx()).is_some_and(|p| rect.contains(p)) {
        return Vec::new();
    }
    ui.input(|i| {
        i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| has_extension(p, extensions)).collect()
    })
}

/// Whether files with one of these extensions are dragged over `rect` now.
pub fn dragged_over(ui: &Ui, rect: egui::Rect, extensions: &[&str]) -> bool {
    file_drag_pos(ui.ctx()).is_some_and(|p| rect.contains(p))
        && ui.input(|i| {
            i.raw.hovered_files.iter().any(|f| f.path.as_ref().is_some_and(|p| has_extension(p, extensions)))
        })
}

/// Whether a path has one of these extensions (any case).
pub fn has_extension(path: &std::path::Path, extensions: &[&str]) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// A pane header ("Details of Body 1", the output title).
pub fn strip(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, style::HEADER);
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, style::HEADER_LINE));
    let galley = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(13.5), style::TEXT);
    let clip = rect.shrink2(vec2(10.0, 0.0));
    ui.painter().with_clip_rect(clip).galley(
        pos2(clip.left(), rect.center().y - galley.size().y / 2.0),
        galley,
        style::TEXT,
    );
}

fn arrow(ui: &Ui, center: egui::Pos2, open: bool, color: Color32) {
    let pts = if open {
        vec![center + vec2(-4.0, -2.0), center + vec2(4.0, -2.0), center + vec2(0.0, 3.0)]
    } else {
        vec![center + vec2(-2.0, -4.0), center + vec2(3.0, 0.0), center + vec2(-2.0, 4.0)]
    };
    ui.painter().add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// One full-width tree row with an optional expander, as QTreeWidget draws it. The expander
/// toggles on a click on the arrow or a double click.
pub fn tree_row(ui: &mut Ui, depth: usize, label: &str, open: Option<&mut bool>, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::TreeItem, true, selected, label));
    let x0 = rect.left() + 8.0 + depth as f32 * 20.0;
    let text_x = x0 + 16.0;
    let band = rect.shrink2(vec2(4.0, 0.5));
    if selected {
        ui.painter().rect_filled(band, 0.0, style::SELECT);
    } else if resp.hovered() {
        ui.painter().rect_filled(band, 0.0, style::HOVER);
    }
    let (fg, arrow_color) = if selected { (Color32::WHITE, Color32::WHITE) } else { (style::TEXT, style::ARROW) };
    if let Some(open) = open {
        let on_arrow = resp.interact_pointer_pos().is_some_and(|p| p.x < text_x - 2.0);
        if (resp.clicked() && on_arrow) || resp.double_clicked() {
            *open = !*open;
        }
        arrow(ui, pos2(x0 + 5.0, rect.center().y), *open, arrow_color);
    }
    ui.painter().text(pos2(text_x, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.5), fg);
    resp
}

/// A collapsible group of Details rows; its open state lives in egui memory.
pub fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    let id = Id::unique(("details-section", title));
    let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(true);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    let p = ui.painter();
    p.rect_filled(rect, 0.0, style::SECTION);
    p.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, style::GRID_LINE));
    arrow(ui, pos2(rect.left() + 15.0, rect.center().y), open, style::ARROW);
    ui.painter().text(
        pos2(rect.left() + 26.0, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.5),
        style::TEXT,
    );
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    if open {
        add(ui);
    }
}

/// "Label [unit]"
pub fn labelled(label: &str, unit: &str) -> String {
    if unit.is_empty() { label.to_string() } else { format!("{label} [{unit}]") }
}

/// A two-column row: the label cell, then the value widget in a child ui.
pub fn row<R>(ui: &mut Ui, label: &str, tooltip: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> R {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    // the label column: a share of the panel (the engine's names are long), at least LABEL_WIDTH
    let label_width =
        (rect.width() * 0.55).max(LABEL_WIDTH).min(rect.width() - 80.0).max(LABEL_WIDTH.min(rect.width()));
    let line = Stroke::new(1.0, style::GRID_LINE);
    let p = ui.painter();
    p.hline(rect.x_range(), rect.bottom() - 0.5, line);
    p.vline(rect.left() + label_width, rect.y_range(), line);
    let label_rect = rect.with_max_x(rect.left() + label_width - 4.0).shrink2(vec2(10.0, 0.0));
    let galley = p.layout_no_wrap(label.to_string(), FontId::proportional(13.5), style::TEXT);
    let clipped = galley.size().x > label_rect.width();
    p.with_clip_rect(label_rect.intersect(ui.clip_rect())).galley(
        pos2(label_rect.left(), rect.center().y - galley.size().y / 2.0),
        galley,
        style::TEXT,
    );
    // a label cut short shows whole on hover, before its tooltip
    match (tooltip, clipped) {
        (Some(tip), true) => {
            resp.on_hover_text(format!(
                "{label}

{tip}"
            ));
        }
        (Some(tip), false) => {
            resp.on_hover_text(tip);
        }
        (None, true) => {
            resp.on_hover_text(label);
        }
        (None, false) => {}
    }
    let value = rect.with_min_x(rect.left() + label_width + 1.0).shrink2(vec2(5.0, 2.0));
    ui.scope_builder(UiBuilder::new().max_rect(value).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.set_clip_rect(value.intersect(ui.clip_rect()));
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        add(ui)
    })
    .inner
}

pub fn read_only(ui: &mut Ui, label: &str, text: &str) {
    row(ui, label, None, |ui| {
        ui.add(egui::Label::new(RichText::new(text).color(style::READ_ONLY)).truncate()).on_hover_text(text);
    });
}

/// A message spanning the whole row.
pub fn message(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.add(egui::Label::new(RichText::new(text).color(style::READ_ONLY)).wrap());
    });
    ui.add_space(6.0);
}

/// A message spanning the whole row, in red: an input the engine would refuse or misread.
pub fn warning(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.add(egui::Label::new(RichText::new(text).color(ERROR)).wrap());
    });
    ui.add_space(6.0);
}

/// A message spanning the whole row, in amber: a caution, not an error.
pub fn caution(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.add(egui::Label::new(RichText::new(text).color(crate::style::CAUTION)).wrap());
    });
    ui.add_space(6.0);
}

/// A text field that edits a buffer and commits on Enter or when it loses focus; Escape reverts.
/// Returns the committed text when it differs from `current`.
fn committed_text(ui: &mut Ui, id: Id, current: &str, enabled: bool, valid: impl Fn(&str) -> bool) -> Option<String> {
    let mut buffer: Option<String> = ui.data(|d| d.get_temp(id));
    // a valid buffer lives only while the field has focus: one left over (the field was not drawn
    // when it lost focus, e.g. another project was opened) is dropped; an invalid one stays, red
    if buffer.as_deref().is_some_and(&valid) && !ui.memory(|m| m.has_focus(id)) {
        ui.data_mut(|d| d.remove::<String>(id));
        buffer = None;
    }
    let mut text = buffer.clone().unwrap_or_else(|| current.to_string());
    let invalid = buffer.is_some() && !valid(&text);
    let color = if !enabled {
        style::READ_ONLY
    } else if invalid {
        ERROR
    } else {
        style::TEXT
    };
    let edit = TextEdit::singleline(&mut text).id(id).frame(Frame::NONE).desired_width(f32::INFINITY).text_color(color);
    let resp = ui.add_enabled(enabled, edit);
    if resp.changed() || (resp.has_focus() && buffer.is_none()) {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    }
    // egui drops the focus on Escape before the field sees it: the key in the frame it was lost
    if (resp.has_focus() || resp.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.data_mut(|d| d.remove::<String>(id));
        resp.surrender_focus();
        return None;
    }
    if resp.lost_focus() && valid(&text) {
        ui.data_mut(|d| d.remove::<String>(id));
        return (text != current).then_some(text);
    }
    if invalid {
        resp.on_hover_text("Not a valid value; it is not applied.");
    }
    None
}

/// A number row; the value changes only when a valid number is committed.
pub fn number(
    ui: &mut Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    unit: &str,
    value: &mut f64,
    enabled: bool,
) -> bool {
    let id = Id::unique(("number", key, label));
    let current = format_number(*value);
    let committed = row(ui, &labelled(label, unit), None, |ui| {
        committed_text(ui, id, &current, enabled, |t| parse_number(t).is_some())
    });
    match committed.and_then(|t| parse_number(&t)) {
        Some(v) if v != *value => {
            *value = v;
            true
        }
        _ => false,
    }
}

/// A row of an optional positive number: empty until one is committed (`value` None).
pub fn optional_number(
    ui: &mut Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    unit: &str,
    value: Option<f64>,
    tooltip: &str,
) -> Option<f64> {
    let id = Id::unique(("optional-number", key, label));
    let current = value.map(format_number).unwrap_or_default();
    // an empty field is valid and applies nothing: a click in and out leaves no red text behind
    let committed = row(ui, &labelled(label, unit), Some(tooltip), |ui| {
        committed_text(ui, id, &current, true, |t| t.trim().is_empty() || parse_number(t).is_some_and(|v| v > 0.0))
    });
    committed.and_then(|t| parse_number(&t)).filter(|v| Some(*v) != value)
}

/// An integer row.
pub fn integer(
    ui: &mut Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    value: &mut i64,
    enabled: bool,
) -> bool {
    let id = Id::unique(("integer", key, label));
    let current = value.to_string();
    let committed =
        row(ui, label, None, |ui| committed_text(ui, id, &current, enabled, |t| t.trim().parse::<i64>().is_ok()));
    match committed.and_then(|t| t.trim().parse::<i64>().ok()) {
        Some(v) if v != *value => {
            *value = v;
            true
        }
        _ => false,
    }
}

/// A text row (names); empty text is refused.
pub fn text(ui: &mut Ui, key: impl std::hash::Hash + std::fmt::Debug, label: &str, value: &mut String) -> bool {
    let id = Id::unique(("text", key, label));
    let current = value.clone();
    match row(ui, label, None, |ui| committed_text(ui, id, &current, true, |t| !t.trim().is_empty())) {
        Some(t) => {
            *value = t.trim().to_string();
            true
        }
        None => false,
    }
}

/// Record ranges as text, "129-140, 150" (empty: none); text that is not ranges is refused.
pub fn ranges_row(
    ui: &mut Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    value: &mut Vec<[i64; 2]>,
    enabled: bool,
) -> bool {
    use gui_core::format::{parse_ranges, ranges_text};
    let id = Id::unique(("ranges", key, label));
    let current = ranges_text(value);
    let tip = "Further record ranges of the mesh, e.g. 129-140, 150";
    match row(ui, label, Some(tip), |ui| committed_text(ui, id, &current, enabled, |t| parse_ranges(t).is_some())) {
        Some(t) => match parse_ranges(&t) {
            Some(r) if r != *value => {
                *value = r;
                true
            }
            _ => false,
        },
        None => false,
    }
}

pub fn check(ui: &mut Ui, label: &str, value: &mut bool, tooltip: Option<&str>) -> bool {
    row(ui, label, tooltip, |ui| ui.checkbox(value, "").changed())
}

/// A check box row that is greyed out (and keeps its value) unless `enabled`.
pub fn check_enabled(ui: &mut Ui, label: &str, value: &mut bool, enabled: bool, tooltip: &str) -> bool {
    row(ui, label, Some(tooltip), |ui| ui.add_enabled(enabled, egui::Checkbox::new(value, "")).changed())
}

/// A drop-down row; `index` is the chosen option.
pub fn choice(
    ui: &mut Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    options: &[String],
    index: &mut usize,
) -> bool {
    let id = Id::unique(("choice", key, label));
    row(ui, label, None, |ui| {
        // the whole value cell is the drop-down: its text, an arrow at the right, a hover shade
        let cell = ui.max_rect().expand2(vec2(5.0, 2.0)).shrink2(vec2(0.0, 0.5));
        let resp = ui.allocate_rect(cell, Sense::click());
        let selected = options.get(*index).cloned().unwrap_or_default();
        resp.widget_info(|| {
            let mut info = egui::WidgetInfo::new(egui::Role::ComboBox);
            info.label = Some(label.to_string());
            info.current_text_value = Some(selected.clone());
            info
        });
        let open = egui::Popup::is_id_open(ui.ctx(), id.with("popup"));
        let painter = ui.painter().with_clip_rect(cell);
        if open {
            painter.rect_filled(cell, 0.0, style::TABLE_SELECT);
        } else if resp.hovered() {
            painter.rect_filled(cell, 0.0, style::HOVER);
        }
        painter.text(
            pos2(cell.left() + 5.0, cell.center().y),
            Align2::LEFT_CENTER,
            &selected,
            FontId::proportional(13.5),
            style::TEXT,
        );
        let c = pos2(cell.right() - 12.0, cell.center().y);
        painter.add(egui::Shape::convex_polygon(
            vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)],
            style::ARROW,
            Stroke::NONE,
        ));
        let mut changed = false;
        egui::Popup::menu(&resp).id(id.with("popup")).width(cell.width()).show(|ui| {
            ui.set_min_width(cell.width() - 12.0);
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for (i, option) in options.iter().enumerate() {
                    if ui.selectable_label(i == *index, option).clicked() && i != *index {
                        *index = i;
                        changed = true;
                    }
                }
            });
        });
        changed
    })
}

/// A choice among the values of an enum.
pub fn choice_of<T: Copy + PartialEq>(
    ui: &mut Ui,
    key: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    values: &[T],
    name: impl Fn(T) -> &'static str,
    value: &mut T,
) -> bool {
    let options: Vec<String> = values.iter().map(|v| name(*v).to_string()).collect();
    let mut index = values.iter().position(|v| v == value).unwrap_or(0);
    if choice(ui, key, label, &options, &mut index) {
        *value = values[index];
        true
    } else {
        false
    }
}

pub enum FileAction {
    Browse,
    Dropped(std::path::PathBuf),
}

/// A file row: "…" opens a dialog, a file dropped on the row is taken; the path shows in full as
/// a tooltip.
pub fn file(ui: &mut Ui, label: &str, path: &str) -> Option<FileAction> {
    row(ui, label, None, |ui| {
        let mut action = None;
        if ui.small_button("…").on_hover_text("Choose a mesh file (GDF or HBG)").clicked() {
            action = Some(FileAction::Browse);
        }
        let shown = if path.is_empty() { "(none)" } else { path };
        let resp = ui.add(egui::Label::new(RichText::new(shown).color(style::TEXT)).truncate().sense(Sense::click()));
        let resp = resp.on_hover_text(if path.is_empty() { "Drop a GDF or HBG file here or press …" } else { path });
        if resp.double_clicked() {
            action = Some(FileAction::Browse);
        }
        let rect = ui.max_rect();
        if dragged_over(ui, rect, &gui_core::mesh::MESH_EXTENSIONS) {
            ui.painter().rect_stroke(rect.shrink(1.0), 2.0, Stroke::new(2.0, style::SELECT), egui::StrokeKind::Inside);
        }
        if let Some(p) = dropped_on(ui, rect, &gui_core::mesh::MESH_EXTENSIONS).into_iter().next() {
            action = Some(FileAction::Dropped(p));
        }
        action
    })
}
