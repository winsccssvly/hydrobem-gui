//! The output pane: the generated table (selection, copy as TSV, CSV export, editing) and the
//! graphs (egui_plot), side by side or as tabs.

use eframe::egui::{self, Align, Align2, Color32, FontId, Id, Layout, Sense, Stroke, Ui, vec2};
use egui_extras::{Column as TableColumn, TableBuilder};
use egui_plot::{Corner, Legend, Line, MarkerShape, Plot, PlotPoints, Points, Polygon};
use gui_core::format::{csv_number, csv_text, format_number, parse_number};

use crate::style;

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Int(i64),
    Num(f64),
    /// A number with fixed decimals (phases).
    Fixed(f64, usize),
    Text(String),
}

impl Cell {
    pub fn display(&self) -> String {
        match self {
            Cell::Int(i) => i.to_string(),
            Cell::Num(v) => format_number(*v),
            Cell::Fixed(v, d) => format!("{v:.d$}"),
            Cell::Text(t) => t.clone(),
        }
    }

    fn csv(&self) -> String {
        match self {
            Cell::Int(i) => i.to_string(),
            Cell::Num(v) | Cell::Fixed(v, _) => csv_number(*v),
            Cell::Text(t) => csv_text(t),
        }
    }

    fn is_number(&self) -> bool {
        !matches!(self, Cell::Text(_))
    }
}

#[derive(Clone, Debug)]
pub struct Column {
    /// "Wave Frequency [rad/s]": the unit goes on a second line.
    pub header: String,
    pub editable: bool,
}

impl Column {
    pub fn new(header: impl Into<String>) -> Column {
        Column { header: header.into(), editable: false }
    }

    pub fn editable(header: impl Into<String>, editable: bool) -> Column {
        Column { header: header.into(), editable }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TableData {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Clone, Debug)]
pub enum PlotSpec {
    Single {
        x_label: String,
        y_label: String,
        x: Vec<f64>,
        y: Vec<f64>,
        /// Single values beside the curve, as points named in a legend (the added mass at
        /// ω = 0 and ω = ∞).
        marks: Vec<PlotMark>,
    },
    /// Two stacked graphs over one x axis; `phase` fixes the lower one to -180..180 deg.
    Paired {
        x_label: String,
        top_label: String,
        bottom_label: String,
        x: Vec<f64>,
        top: Vec<f64>,
        bottom: Vec<f64>,
        phase: bool,
    },
    /// A matrix as coloured cells: `values[i][j]` at (`x[j]`, `y[i]`); the cells span half way to
    /// their neighbours. Row i is the table's row i.
    Heatmap {
        x_label: String,
        y_label: String,
        /// What the colours are, with its unit (the colour bar's title).
        value_label: String,
        x: Vec<f64>,
        y: Vec<f64>,
        values: Vec<Vec<f64>>,
    },
}

/// A value drawn as one point beside a curve, named in the graph's legend.
#[derive(Clone, Debug, PartialEq)]
pub struct PlotMark {
    pub x: f64,
    pub y: f64,
    pub label: String,
}

#[derive(Clone, Debug, Default)]
pub struct Output {
    pub title: String,
    pub table: Option<TableData>,
    pub plot: Option<PlotSpec>,
    /// Further tables of the node, as tabs after `table`.
    pub tabs: Option<Tabs>,
}

/// The tables of a node shown as tabs: the first is `Output::table`.
#[derive(Clone, Debug, Default)]
pub struct Tabs {
    /// The names of all the tabs, the first's included.
    pub names: Vec<String>,
    pub more: Vec<TableData>,
    /// Per tab the rows of one mesh record (the vertices of a panel: 4), for the viewport's
    /// highlight; 1 when not given.
    pub rows_per_record: Vec<usize>,
}

impl Output {
    /// The table of tab `tab` (the first without tabs).
    pub fn table_of(&self, tab: usize) -> Option<&TableData> {
        match (tab, &self.tabs) {
            (0, _) | (_, None) => self.table.as_ref(),
            (k, Some(t)) => t.more.get(k - 1).or(self.table.as_ref()),
        }
    }

    /// The rows of one record in tab `tab`.
    pub fn rows_per_record(&self, tab: usize) -> usize {
        self.tabs.as_ref().and_then(|t| t.rows_per_record.get(tab)).copied().unwrap_or(1).max(1)
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.as_ref().map_or(1, |t| t.more.len() + 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellEdit {
    pub row: usize,
    pub column: usize,
    pub value: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OutputLayout {
    #[default]
    Split,
    Tabs,
}

/// What the user did in the output pane this frame.
#[derive(Default)]
pub struct OutputResponse {
    /// Cell edits: one typed value, or the values of a paste.
    pub edits: Vec<CellEdit>,
    /// Pasted cells left out (beyond the table, not editable, not a number).
    pub paste_skipped: usize,
    /// A row selected by a click (field-point picking follows it).
    pub clicked_row: Option<usize>,
    /// The rows of the table's selection (first, last), when it has one.
    pub selected_rows: Option<(usize, usize)>,
    /// The table tab shown.
    pub tab: usize,
    pub export_csv: bool,
}

/// What a press in the table selects and a drag extends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drag {
    Cells,
    /// whole rows, from the number column
    Rows,
    /// whole columns, from the header
    Columns,
    /// the number column's header: everything
    All,
}

/// The table's selection: a rectangle of cells between the anchor and the cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Selection {
    anchor: (usize, usize),
    cursor: (usize, usize),
    active: bool,
}

impl Selection {
    fn bounds(&self) -> ((usize, usize), (usize, usize)) {
        let (r0, r1) = (self.anchor.0.min(self.cursor.0), self.anchor.0.max(self.cursor.0));
        let (c0, c1) = (self.anchor.1.min(self.cursor.1), self.anchor.1.max(self.cursor.1));
        ((r0, c0), (r1, c1))
    }

    fn contains(&self, r: usize, c: usize) -> bool {
        let ((r0, c0), (r1, c1)) = self.bounds();
        self.active && (r0..=r1).contains(&r) && (c0..=c1).contains(&c)
    }
}

/// The table as tab-separated text of the selection's bounding box (values as displayed);
/// with `headers` a one-line header row first.
pub fn selection_tsv(table: &TableData, from: (usize, usize), to: (usize, usize), headers: bool) -> String {
    let mut lines = Vec::new();
    if headers {
        lines.push(table.columns[from.1..=to.1].iter().map(|c| c.header.clone()).collect::<Vec<_>>().join("\t"));
    }
    for row in &table.rows[from.0..=to.0] {
        lines.push(row[from.1..=to.1].iter().map(Cell::display).collect::<Vec<_>>().join("\t"));
    }
    lines.join("\r\n")
}

/// The edits of pasting tab-separated text (Excel's copy) at the selection's top-left cell: a
/// single value fills the whole selection. Cells beyond the table, of columns that are not editable
/// and texts that are not numbers are left out; the second number counts them.
fn paste_edits(table: &TableData, from: (usize, usize), to: (usize, usize), text: &str) -> (Vec<CellEdit>, usize) {
    let text = text.trim_end_matches(['\r', '\n']);
    let grid: Vec<Vec<&str>> = text.split('\n').map(|l| l.trim_end_matches('\r').split('\t').collect()).collect();
    let cells: Vec<(usize, usize, &str)> = match grid.as_slice() {
        [line] if line.len() == 1 => {
            (from.0..=to.0).flat_map(|r| (from.1..=to.1).map(move |c| (r, c, line[0]))).collect()
        }
        _ => grid
            .iter()
            .enumerate()
            .flat_map(|(i, line)| line.iter().enumerate().map(move |(j, v)| (from.0 + i, from.1 + j, *v)))
            .collect(),
    };
    let mut skipped = 0;
    let mut edits = Vec::new();
    for (row, column, value) in cells {
        let value = parse_number(value.trim())
            .filter(|_| row < table.rows.len() && table.columns.get(column).is_some_and(|c| c.editable));
        match value {
            Some(value) => edits.push(CellEdit { row, column, value }),
            None => skipped += 1,
        }
    }
    (edits, skipped)
}

/// The whole table as CSV: RFC 4180, CRLF, headers on one line, numbers at full precision.
pub fn table_csv(table: &TableData) -> String {
    let mut out = String::new();
    let headers: Vec<String> = table.columns.iter().map(|c| csv_text(&c.header)).collect();
    out.push_str(&headers.join(","));
    out.push_str("\r\n");
    for row in &table.rows {
        out.push_str(&row.iter().map(Cell::csv).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}

fn header_lines(header: &str) -> String {
    match header.rfind(" [") {
        Some(i) if header.ends_with(']') => format!("{}\n{}", &header[..i], &header[i + 1..]),
        _ => header.to_string(),
    }
}

/// Column widths: at least as wide as the header and the values; spare width is shared among the
/// value columns (the first column, the row number, keeps its natural width).
fn column_widths(ui: &Ui, table: &TableData, available: f32) -> Vec<f32> {
    let font = FontId::proportional(13.0);
    let measure = |t: &str| ui.fonts_mut(|f| f.layout_no_wrap(t.to_string(), font.clone(), Color32::BLACK).size().x);
    let sample = table.rows.len().min(400);
    let mut widths: Vec<f32> = table
        .columns
        .iter()
        .enumerate()
        .map(|(c, col)| {
            let header = header_lines(&col.header).lines().map(measure).fold(0.0, f32::max);
            let values = table.rows[..sample].iter().map(|r| measure(&r[c].display())).fold(0.0, f32::max);
            header.max(values) + 18.0
        })
        .collect();
    let total: f32 = widths.iter().sum();
    if total < available && widths.len() > 1 {
        let spare = (available - total) / (widths.len() - 1) as f32;
        widths.iter_mut().skip(1).for_each(|w| *w += spare);
    }
    widths
}

/// The space between a cell's border and its text.
const CELL_PADDING: f32 = 8.0;

/// A cell's grid lines: right and bottom, and the left border of the first column, the top border
/// of the header.
fn grid_lines(ui: &Ui, r: egui::Rect, first_column: bool, header: bool, line: Stroke) {
    let p = ui.painter();
    p.vline(r.right() - 0.5, r.y_range(), line);
    p.hline(r.x_range(), r.bottom() - 0.5, line);
    if first_column {
        p.vline(r.left() + 0.5, r.y_range(), line);
    }
    if header {
        p.hline(r.x_range(), r.top() + 0.5, line);
    }
}

pub struct TableView<'a> {
    pub id: Id,
    pub table: &'a TableData,
    /// Rows to select (first, last) and scroll to: a picked field point, a mesh record.
    pub select_rows: Option<(usize, usize)>,
    /// Clear the selection (a click on the graph away from its points).
    pub clear: bool,
}

impl TableView<'_> {
    pub fn show(self, ui: &mut Ui, response: &mut OutputResponse) {
        let table = self.table;
        let id = self.id;
        let mut sel: Selection = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        if self.clear {
            sel = Selection::default();
        }
        if let Some((r0, r1)) = self.select_rows
            && r0 < table.rows.len()
        {
            let r1 = r1.clamp(r0, table.rows.len() - 1);
            sel = Selection { anchor: (r0, 0), cursor: (r1, table.columns.len().saturating_sub(1)), active: true };
        }
        let editing_id = id.with("editing");
        let mut editing: Option<(usize, usize, String)> = ui.data(|d| d.get_temp(editing_id));
        let line = Stroke::new(1.0, style::GRID_LINE);
        let widths = column_widths(ui, table, ui.available_width() - ui.spacing().scroll.bar_width - 2.0);
        let rows = table.rows.len();
        let last_column = table.columns.len().saturating_sub(1);
        // a press starts a selection (a cell, a row by its number, a column by its header) that a
        // drag with the button held extends; Shift extends the current one
        let drag_id = id.with("drag");
        let mut drag: Option<Drag> = ui.data(|d| d.get_temp(drag_id));
        let (pressed, held, shift) =
            ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.modifiers.shift));
        let mut press: Option<(Drag, usize, usize)> = None;
        let mut over: Option<(usize, usize, bool)> = None;
        let mut double_clicked: Option<(usize, usize)> = None;

        let area = egui::ScrollArea::horizontal().id_salt(id.with("h")).auto_shrink([false, false]);
        let inner = area.show(ui, |ui| {
            // cells side by side without gaps, so that the grid lines and fills meet
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let mut builder = TableBuilder::new(ui)
                .id_salt(id)
                .striped(false)
                .resizable(false)
                .vscroll(true)
                .auto_shrink([false, false])
                .cell_layout(Layout::right_to_left(Align::Center));
            for w in &widths {
                builder = builder.column(TableColumn::exact(*w));
            }
            if let Some((r, _)) = self.select_rows {
                builder = builder.scroll_to_row(r, Some(Align::Center));
            }
            builder
                .header(38.0, |mut h| {
                    for (c, col) in table.columns.iter().enumerate() {
                        let mut cell = None;
                        h.col(|ui| {
                            let r = ui.max_rect();
                            // the whole cell senses the pointer (only painted, the cell has no widget)
                            cell = Some(ui.interact(r, id.with(("header", c)), Sense::click()));
                            let whole = rows > 0 && sel.contains(0, c) && sel.contains(rows - 1, c);
                            ui.painter().rect_filled(
                                r,
                                0.0,
                                if whole { style::TABLE_SELECT } else { style::TABLE_HEADER },
                            );
                            grid_lines(ui, r, c == 0, true, line);
                            ui.painter().text(
                                r.center(),
                                Align2::CENTER_CENTER,
                                header_lines(&col.header),
                                FontId::proportional(12.5),
                                style::TEXT,
                            );
                        });
                        let Some(resp) = cell else { continue };
                        if resp.contains_pointer() {
                            over = Some((0, c, true));
                            if pressed {
                                press = Some((if c == 0 { Drag::All } else { Drag::Columns }, 0, c));
                            }
                        }
                    }
                })
                .body(|body| {
                    body.rows(24.0, rows, |mut row| {
                        let r = row.index();
                        for (c, cell) in table.rows[r].iter().enumerate() {
                            let mut hit = None;
                            row.col(|ui| {
                                let rect = ui.max_rect();
                                hit = Some(ui.interact(rect, id.with(("cell", r, c)), Sense::click()));
                                if sel.contains(r, c) {
                                    ui.painter().rect_filled(rect, 0.0, style::TABLE_SELECT);
                                } else if c == 0 {
                                    ui.painter().rect_filled(rect, 0.0, style::TABLE_HEADER);
                                }
                                grid_lines(ui, rect, c == 0, false, line);
                                if let Some((er, ec, text)) =
                                    editing.as_mut().filter(|(er, ec, _)| *er == r && *ec == c)
                                {
                                    let edit_id = id.with(("edit", *er, *ec));
                                    let resp = ui
                                        .add(egui::TextEdit::singleline(text).id(edit_id).desired_width(f32::INFINITY));
                                    if resp.lost_focus() {
                                        if !ui.input(|i| i.key_pressed(egui::Key::Escape))
                                            && let Some(v) = parse_number(text)
                                        {
                                            response.edits.push(CellEdit { row: *er, column: *ec, value: v });
                                        }
                                        editing = None;
                                    }
                                    return;
                                }
                                // numbers right-aligned, text left-aligned, inside a padding
                                let (pos, align) = if cell.is_number() {
                                    (rect.right_center() - vec2(CELL_PADDING, 0.0), Align2::RIGHT_CENTER)
                                } else {
                                    (rect.left_center() + vec2(CELL_PADDING, 0.0), Align2::LEFT_CENTER)
                                };
                                let color = if c == 0 { style::SELECT } else { style::TEXT };
                                ui.painter().text(pos, align, cell.display(), FontId::proportional(13.0), color);
                            });
                            let Some(resp) = hit else { continue };
                            if resp.contains_pointer() {
                                over = Some((r, c, false));
                                if pressed {
                                    press = Some((if c == 0 { Drag::Rows } else { Drag::Cells }, r, c));
                                }
                            }
                            // a click soon after another one counts as a triple click
                            if (resp.double_clicked() || resp.triple_clicked()) && table.columns[c].editable {
                                double_clicked = Some((r, c));
                            }
                            resp.context_menu(|ui| {
                                let ((r0, c0), (r1, c1)) = sel.bounds();
                                if ui.button("Copy").clicked() {
                                    ui.ctx().copy_text(selection_tsv(table, (r0, c0), (r1, c1), false));
                                    ui.close();
                                }
                                if ui.button("Copy with Headers").clicked() {
                                    ui.ctx().copy_text(selection_tsv(table, (r0, c0), (r1, c1), true));
                                    ui.close();
                                }
                                if ui.button("Select All").clicked() {
                                    sel = Selection {
                                        anchor: (0, 0),
                                        cursor: (rows - 1, table.columns.len() - 1),
                                        active: true,
                                    };
                                    ui.close();
                                }
                                ui.separator();
                                if ui.button("Export CSV…").clicked() {
                                    response.export_csv = true;
                                    ui.close();
                                }
                            });
                        }
                    });
                });
        });
        let table_rect = inner.inner_rect;

        let end = rows.saturating_sub(1);
        if let Some((kind, r, c)) = press.filter(|_| rows > 0) {
            let extend = shift && sel.active;
            sel = match (kind, extend) {
                (Drag::All, _) => Selection { anchor: (0, 0), cursor: (end, last_column), active: true },
                (Drag::Columns, true) => Selection { anchor: (0, sel.anchor.1), cursor: (end, c), active: true },
                (Drag::Columns, false) => Selection { anchor: (0, c), cursor: (end, c), active: true },
                (Drag::Rows, true) => Selection { anchor: (sel.anchor.0, 0), cursor: (r, last_column), active: true },
                (Drag::Rows, false) => Selection { anchor: (r, 0), cursor: (r, last_column), active: true },
                (Drag::Cells, true) => Selection { cursor: (r, c), ..sel },
                (Drag::Cells, false) => Selection { anchor: (r, c), cursor: (r, c), active: true },
            };
            drag = Some(kind);
            if kind != Drag::Columns && kind != Drag::All {
                response.clicked_row = Some(r);
            }
            ui.data_mut(|d| d.insert_temp(id.with("focused"), true));
        } else if let (Some(kind), true, Some((r, c, header))) = (drag, held, over) {
            // the drag extends the selection to the cell, row or column under the pointer
            match kind {
                Drag::Cells if !header => sel.cursor = (r, c),
                Drag::Rows if !header => sel.cursor = (r, last_column),
                Drag::Columns => sel.cursor = (end, c),
                _ => {}
            }
        } else if ui.input(|i| i.pointer.any_pressed())
            && !ui.input(|i| i.pointer.interact_pos().is_some_and(|p| table_rect.contains(p)))
        {
            ui.data_mut(|d| d.insert_temp(id.with("focused"), false));
        }
        if let Some((r, c)) = double_clicked {
            editing = Some((r, c, table.rows[r][c].display()));
            // focused once: Enter, Escape or a click elsewhere then ends the edit
            ui.memory_mut(|m| m.request_focus(id.with(("edit", r, c))));
        }

        // Ctrl+C copies the selection as TSV, Ctrl+Shift+C with a header row; Ctrl+A selects all;
        // Ctrl+V pastes TSV into the editable columns from the selection's top-left cell.
        let focused = ui.data(|d| d.get_temp::<bool>(id.with("focused"))).unwrap_or(false);
        if focused && editing.is_none() && !ui.ctx().egui_wants_keyboard_input() && rows > 0 {
            let (copy, shift, all, paste) = ui.input(|i| {
                let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy))
                    || (i.modifiers.command && i.key_pressed(egui::Key::C));
                let paste = i.events.iter().find_map(|e| match e {
                    egui::Event::Paste(text) => Some(text.clone()),
                    _ => None,
                });
                (copy, i.modifiers.shift, i.modifiers.command && i.key_pressed(egui::Key::A), paste)
            });
            if let Some(text) = paste
                && sel.active
                && table.columns.iter().any(|c| c.editable)
            {
                let ((r0, c0), (r1, c1)) = sel.bounds();
                let (edits, skipped) = paste_edits(table, (r0, c0), (r1, c1), &text);
                response.edits = edits;
                response.paste_skipped = skipped;
            }
            if all {
                sel = Selection { anchor: (0, 0), cursor: (rows - 1, table.columns.len() - 1), active: true };
            }
            if copy && sel.active {
                let ((r0, c0), (r1, c1)) = sel.bounds();
                ui.ctx().copy_text(selection_tsv(table, (r0, c0), (r1.min(rows - 1), c1), shift));
            }
        }
        if !held {
            drag = None;
        }
        if sel.active && rows > 0 {
            let ((r0, _), (r1, _)) = sel.bounds();
            response.selected_rows = Some((r0.min(rows - 1), r1.min(rows - 1)));
        }
        ui.data_mut(|d| {
            if let Some(k) = drag {
                d.insert_temp(drag_id, k);
            } else {
                d.remove::<Drag>(drag_id);
            }
            d.insert_temp(id, sel);
            match editing {
                Some(e) => {
                    d.insert_temp(editing_id, e);
                }
                None => d.remove::<(usize, usize, String)>(editing_id),
            }
        });
    }
}

/// The pixel range of the grid lines: closer ones are hidden, they fade in up to the upper value.
const GRID_SPACING: (f32, f32) = (8.0, 40.0);

/// The smallest step of 1, 2 or 5 times a power of ten that is at least `step`.
fn nice_step(step: f64) -> f64 {
    let power = 10f64.powf(step.log10().floor());
    [1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * power).find(|s| *s >= step * (1.0 - 1e-9)).unwrap_or(10.0 * power)
}

/// Grid lines whose labelled step follows the size of the graph: labels about `label_px` pixels
/// apart (steps of 1, 2 or 5 times a power of ten), finer unlabelled lines between them.
fn adaptive_grid(label_px: f64) -> impl Fn(egui_plot::GridInput) -> Vec<egui_plot::GridMark> {
    move |input| {
        let per_px = input.base_step_size / f64::from(GRID_SPACING.0);
        if !(per_px.is_finite() && per_px > 0.0) {
            return Vec::new();
        }
        let major = nice_step(label_px * per_px);
        let minor = major / if (major / 10f64.powf(major.log10().floor()) - 2.0).abs() < 1e-6 { 4.0 } else { 5.0 };
        let mut marks = Vec::new();
        let (lo, hi) = (input.bounds.0.min(input.bounds.1), input.bounds.0.max(input.bounds.1));
        let mut k = (lo / minor).floor() as i64;
        while (k as f64) * minor <= hi && marks.len() < 2000 {
            let value = k as f64 * minor;
            let on_major = ((value / major).round() * major - value).abs() < minor * 1e-6;
            marks.push(egui_plot::GridMark { value, step_size: if on_major { major } else { minor } });
            k += 1;
        }
        marks
    }
}

/// A tick label with the decimals of its step: 0.5 as "0.5" (egui_plot's default rounds the step's
/// logarithm and would write "0" and "2" for 0.5 and 1.5). Large values, and small ones on an axis
/// that shows only small values (a step below 1e-3 within ±1), in exponent notation: "1.2e-03",
/// not "0.0012".
fn tick_label(mark: egui_plot::GridMark, range: &std::ops::RangeInclusive<f64>) -> String {
    let step = (mark.step_size.log10() + 1e-9).floor();
    let small = step < -3.0 && range.start().abs().max(range.end().abs()) < 1.0;
    if small && mark.value.abs() < mark.step_size * 1e-6 {
        return "0".into();
    }
    // as the tables show them: in exponent notation, to the step's digit
    if mark.value != 0.0 && (small || mark.value.abs().max(mark.step_size) >= gui_core::format::EXPONENT_FROM) {
        let digits = (mark.value.abs().log10().floor() - step).max(0.0) as usize;
        return gui_core::format::format_exponent(mark.value, digits, false);
    }
    let decimals = (-step).max(0.0) as usize;
    format!("{:.decimals$}", mark.value)
}

/// The labelled tick spacing aimed at (pixels) and the spacing over which a label fades in, along
/// x and y: 0.5 steps are labelled once they have the room.
const X_LABELS: (f64, f32, f32) = (52.0, 34.0, 42.0);
const Y_LABELS: (f64, f32, f32) = (26.0, 15.0, 18.0);

/// The point closest to the pointer, within this many pixels, is picked by a click.
const PICK_RADIUS: f32 = 16.0;

/// A graph or two stacked graphs over frequency; lines break at NaN, a marker on every point. Point
/// i is the table's row i: `point` is highlighted with its values. A click returns what it chose:
/// Some(Some(i)) near point i, Some(None) elsewhere on a graph (nothing chosen), None no click.
pub fn show_plot(ui: &mut Ui, id: Id, spec: &PlotSpec, point: Option<usize>) -> Option<Option<usize>> {
    let series = |x: &[f64], y: &[f64]| -> Vec<Vec<[f64; 2]>> {
        let mut runs = vec![Vec::new()];
        for (a, b) in x.iter().zip(y) {
            if b.is_finite() {
                runs.last_mut().expect("a run").push([*a, *b]);
            } else if !runs.last().expect("a run").is_empty() {
                runs.push(Vec::new());
            }
        }
        runs
    };
    let draw = |ui: &mut Ui,
                salt: &str,
                x_label: &str,
                y_label: &str,
                x: &[f64],
                y: &[f64],
                marks: &[PlotMark],
                phase: bool,
                height: f32|
     -> Option<Option<usize>> {
        let x_axis = egui_plot::AxisHints::new_x()
            .label(if salt == "top" { String::new() } else { x_label.to_string() })
            .formatter(tick_label)
            .label_spacing(X_LABELS.1..=X_LABELS.2);
        let y_axis = egui_plot::AxisHints::new_y()
            .label(y_label.to_string())
            .formatter(tick_label)
            .min_thickness(56.0)
            .label_spacing(Y_LABELS.1..=Y_LABELS.2);
        let mut plot = Plot::new(id.with(salt))
            .height(height)
            .custom_x_axes(vec![x_axis])
            .custom_y_axes(vec![y_axis])
            .show_background(false)
            .allow_scroll(false)
            .grid_spacing(GRID_SPACING.0..=GRID_SPACING.1)
            .x_grid_spacer(adaptive_grid(X_LABELS.0))
            .y_grid_spacer(adaptive_grid(Y_LABELS.0))
            .link_axis(id.with("x"), [true, false])
            .link_cursor(id.with("x"), [true, false]);
        if !marks.is_empty() {
            plot = plot.legend(Legend::default().position(Corner::LeftBottom));
        }
        if phase {
            plot = plot
                .include_y(-180.0)
                .include_y(180.0)
                .y_grid_spacer(egui_plot::uniform_grid_spacer(|_| [45.0, 90.0, 180.0]));
        }
        let picked = point.filter(|&i| i < x.len() && i < y.len() && y[i].is_finite());
        let shown = plot.show(ui, |p| {
            let color = style::SELECT;
            for run in series(x, y) {
                if run.is_empty() {
                    continue;
                }
                p.line(Line::new("", PlotPoints::from(run.clone())).color(color).width(1.5));
                p.points(Points::new("", PlotPoints::from(run)).color(color).radius(2.5));
            }
            for (k, m) in marks.iter().filter(|m| m.x.is_finite() && m.y.is_finite()).enumerate() {
                let (shape, color) = if k == 0 {
                    (MarkerShape::Diamond, Color32::from_rgb(214, 96, 32))
                } else {
                    (MarkerShape::Square, Color32::from_rgb(46, 150, 80))
                };
                p.points(
                    Points::new(m.label.as_str(), PlotPoints::from(vec![[m.x, m.y]]))
                        .shape(shape)
                        .filled(true)
                        .color(color)
                        .radius(5.0),
                );
            }
            if let Some(i) = picked {
                p.points(Points::new("", PlotPoints::from(vec![[x[i], y[i]]])).color(style::PICKED).radius(5.0));
            }
        });
        if let Some(i) = picked {
            // the values in a box beside the point, on the side with more room, inside the graph
            // (the axes name the quantities)
            let text = format!("x = {}\ny = {}", format_number(x[i]), format_number(y[i]));
            let at = shown.transform.position_from_point(&egui_plot::PlotPoint::new(x[i], y[i]));
            let frame = *shown.transform.frame();
            let painter = ui.painter_at(frame);
            let galley = painter.layout_no_wrap(text, FontId::proportional(12.0), style::TEXT);
            let size = galley.size() + vec2(12.0, 8.0);
            let left = if at.x + 10.0 + size.x <= frame.right() { at.x + 10.0 } else { at.x - 10.0 - size.x };
            let top = if at.y - 10.0 - size.y >= frame.top() { at.y - 10.0 - size.y } else { at.y + 10.0 };
            let rect = egui::Rect::from_min_size(egui::pos2(left, top), size);
            painter.rect(
                rect,
                3.0,
                Color32::from_white_alpha(235),
                Stroke::new(1.0, style::PICKED),
                egui::StrokeKind::Inside,
            );
            painter.galley(rect.min + vec2(6.0, 4.0), galley, style::TEXT);
        }
        let pointer = shown.response.clicked().then(|| shown.response.interact_pointer_pos()).flatten()?;
        Some(
            (0..x.len().min(y.len()))
                .filter(|&i| y[i].is_finite())
                .map(|i| {
                    (i, shown.transform.position_from_point(&egui_plot::PlotPoint::new(x[i], y[i])).distance(pointer))
                })
                .filter(|(_, d)| *d <= PICK_RADIUS)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i),
        )
    };
    match spec {
        PlotSpec::Single { x_label, y_label, x, y, marks } => {
            let h = ui.available_height();
            draw(ui, "single", x_label, y_label, x, y, marks, false, h)
        }
        PlotSpec::Heatmap { x_label, y_label, value_label, x, y, values } => {
            heatmap(ui, id.with("heatmap"), x_label, y_label, value_label, x, y, values, point)
        }
        PlotSpec::Paired { x_label, top_label, bottom_label, x, top, bottom, phase } => {
            let h = (ui.available_height() - ui.spacing().item_spacing.y) / 2.0;
            let a = draw(ui, "top", x_label, top_label, x, top, &[], false, h);
            let b = draw(ui, "bottom", x_label, bottom_label, x, bottom, &[], *phase, h);
            // a point of either graph, else a click on either
            match (a, b) {
                (Some(Some(i)), _) | (_, Some(Some(i))) => Some(Some(i)),
                _ => a.or(b),
            }
        }
    }
}

/// The edges of the cells about the values `v` (increasing): half way to each neighbour, the
/// outer ones as far out as the inner.
fn cell_edges(v: &[f64]) -> Vec<f64> {
    match v.len() {
        0 => Vec::new(),
        1 => {
            let h = 0.5 * v[0].abs().max(1.0) * 0.1;
            vec![v[0] - h, v[0] + h]
        }
        n => {
            let mut e = Vec::with_capacity(n + 1);
            e.push(v[0] - 0.5 * (v[1] - v[0]));
            e.extend(v.windows(2).map(|w| 0.5 * (w[0] + w[1])));
            e.push(v[n - 1] + 0.5 * (v[n - 1] - v[n - 2]));
            e
        }
    }
}

/// The colour bar's width beside a heatmap.
const HEAT_BAR: f32 = 96.0;

/// A matrix as coloured cells (the viewport's colours, `heat_color`) with a colour bar; NaN cells
/// grey. The cell under the pointer shows its values; row `point` is outlined. A click returns as
/// `show_plot`'s: Some(Some(row)) on a cell, Some(None) elsewhere on the graph, None no click.
#[allow(clippy::too_many_arguments)]
fn heatmap(
    ui: &mut Ui,
    id: Id,
    x_label: &str,
    y_label: &str,
    value_label: &str,
    x: &[f64],
    y: &[f64],
    values: &[Vec<f64>],
    point: Option<usize>,
) -> Option<Option<usize>> {
    let (xe, ye) = (cell_edges(x), cell_edges(y));
    let finite = values.iter().flatten().copied().filter(|v| v.is_finite());
    let (min, max) = finite.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    let (min, max) = if min.is_finite() { (min, if max > min { max } else { min + 1.0 }) } else { (0.0, 1.0) };
    let color = |v: f64| -> Color32 {
        if !v.is_finite() {
            return style::GRID_LINE;
        }
        let c = gui_core::scene::heat_color((v - min) / (max - min));
        Color32::from_rgb(c[0], c[1], c[2])
    };
    let full = ui.available_rect_before_wrap();
    let plot_rect = egui::Rect::from_min_max(full.min, egui::pos2(full.right() - HEAT_BAR, full.bottom()));
    let bar_rect = egui::Rect::from_min_max(egui::pos2(full.right() - HEAT_BAR, full.top()), full.max);
    ui.allocate_rect(full, Sense::hover());
    let shown = ui
        .scope_builder(egui::UiBuilder::new().max_rect(plot_rect), |ui| {
            let x_axis = egui_plot::AxisHints::new_x()
                .label(x_label.to_string())
                .formatter(tick_label)
                .label_spacing(X_LABELS.1..=X_LABELS.2);
            let y_axis = egui_plot::AxisHints::new_y()
                .label(y_label.to_string())
                .formatter(tick_label)
                .min_thickness(56.0)
                .label_spacing(Y_LABELS.1..=Y_LABELS.2);
            Plot::new(id)
                .height(plot_rect.height())
                .custom_x_axes(vec![x_axis])
                .custom_y_axes(vec![y_axis])
                .show_background(false)
                .allow_scroll(false)
                .show_x(false)
                .show_y(false)
                .set_margin_fraction(vec2(0.0, 0.0))
                .x_grid_spacer(adaptive_grid(X_LABELS.0))
                .y_grid_spacer(adaptive_grid(Y_LABELS.0))
                .show(ui, |p| {
                    for (i, row) in values.iter().enumerate().take(y.len()) {
                        for (j, v) in row.iter().enumerate().take(x.len()) {
                            let corners =
                                vec![[xe[j], ye[i]], [xe[j + 1], ye[i]], [xe[j + 1], ye[i + 1]], [xe[j], ye[i + 1]]];
                            p.polygon(
                                Polygon::new("", PlotPoints::from(corners))
                                    .fill_color(color(*v))
                                    .stroke(Stroke::NONE)
                                    .allow_hover(false),
                            );
                        }
                    }
                    if let Some(i) = point.filter(|&i| i < y.len()) {
                        let (l, r) = (xe[0], xe[x.len()]);
                        let band = vec![[l, ye[i]], [r, ye[i]], [r, ye[i + 1]], [l, ye[i + 1]]];
                        p.polygon(
                            Polygon::new("", PlotPoints::from(band))
                                .fill_color(Color32::TRANSPARENT)
                                .stroke(Stroke::new(2.0, style::PICKED))
                                .allow_hover(false),
                        );
                    }
                })
        })
        .inner;
    // the cell under a position, by the edges
    let cell = |at: egui::Pos2| -> Option<(usize, usize)> {
        let v = shown.transform.value_from_position(at);
        let find = |e: &[f64], t: f64| (0..e.len().saturating_sub(1)).find(|&k| t >= e[k] && t <= e[k + 1]);
        Some((find(&ye, v.y)?, find(&xe, v.x)?))
    };
    if let Some((i, j)) = shown.response.hover_pos().and_then(cell) {
        let v = values.get(i).and_then(|r| r.get(j)).copied().unwrap_or(f64::NAN);
        let text = format!(
            "{x_label}: {}\n{y_label}: {}\n{}",
            format_number(x[j]),
            format_number(y[i]),
            if v.is_finite() { format_number(v) } else { "—".into() }
        );
        let frame = *shown.transform.frame();
        let painter = ui.painter_at(frame);
        let galley = painter.layout_no_wrap(text, FontId::proportional(12.0), style::TEXT);
        let size = galley.size() + vec2(12.0, 8.0);
        let at = shown.response.hover_pos().unwrap_or(frame.center());
        let left = if at.x + 14.0 + size.x <= frame.right() { at.x + 14.0 } else { at.x - 14.0 - size.x };
        let top = if at.y - 14.0 - size.y >= frame.top() { at.y - 14.0 - size.y } else { at.y + 14.0 };
        let rect = egui::Rect::from_min_size(egui::pos2(left, top), size);
        painter.rect(
            rect,
            3.0,
            Color32::from_white_alpha(235),
            Stroke::new(1.0, style::GRID_LINE),
            egui::StrokeKind::Inside,
        );
        painter.galley(rect.min + vec2(6.0, 4.0), galley, style::TEXT);
    }
    // the colour bar: its title, the colours from the minimum (bottom) to the maximum (top)
    let p = ui.painter_at(bar_rect);
    let font = FontId::proportional(11.0);
    let title = p.layout(value_label.to_string(), font.clone(), style::TEXT, HEAT_BAR - 10.0);
    let title_height = title.size().y;
    p.galley(bar_rect.left_top() + vec2(6.0, 4.0), title, style::TEXT);
    let top = bar_rect.top() + title_height + 12.0;
    let bar = egui::Rect::from_min_max(
        egui::pos2(bar_rect.left() + 6.0, top),
        egui::pos2(bar_rect.left() + 20.0, (bar_rect.bottom() - 30.0).max(top + 20.0)),
    );
    let steps = 64;
    for k in 0..steps {
        let t = k as f64 / (steps - 1) as f64;
        let h = bar.height() / steps as f32;
        let y0 = bar.bottom() - (k + 1) as f32 * h;
        let c = gui_core::scene::heat_color(t);
        p.rect_filled(
            egui::Rect::from_min_size(egui::pos2(bar.left(), y0), vec2(bar.width(), h + 0.5)),
            0.0,
            Color32::from_rgb(c[0], c[1], c[2]),
        );
    }
    p.rect_stroke(bar, 0.0, Stroke::new(1.0, style::ARROW), egui::StrokeKind::Outside);
    for (y_at, v) in [(bar.top(), max), (bar.center().y, 0.5 * (min + max)), (bar.bottom(), min)] {
        p.line_segment(
            [egui::pos2(bar.right(), y_at), egui::pos2(bar.right() + 4.0, y_at)],
            Stroke::new(1.0, style::ARROW),
        );
        p.text(egui::pos2(bar.right() + 6.0, y_at), Align2::LEFT_CENTER, format_number(v), font.clone(), style::TEXT);
    }
    let pointer = shown.response.clicked().then(|| shown.response.interact_pointer_pos()).flatten()?;
    Some(cell(pointer).map(|(i, _)| i))
}

/// Clears the table's selection in the next frame (a click away from everything in the viewport).
pub fn clear_selection(ctx: &egui::Context, output: &Output) {
    ctx.data_mut(|d| d.insert_temp(Id::unique(("output", &output.title)).with("clear-rows"), true));
}

/// The whole output pane below the viewport. A point clicked on the graph selects its table row and
/// a row clicked in the table highlights its point.
pub fn show(
    ui: &mut Ui,
    output: &Output,
    layout: OutputLayout,
    select_rows: Option<(usize, usize)>,
    response: &mut OutputResponse,
) {
    crate::widgets::strip(ui, if output.title.is_empty() { "Generated Data" } else { &output.title });
    let id = Id::unique(("output", &output.title));
    let point_id = id.with("point");
    let pending_id = id.with("pending-row");
    let clear_id = id.with("clear-rows");
    let point: Option<usize> = ui.data(|d| d.get_temp(point_id));
    // the tab of a node with several tables, kept by its name from node to node
    let tab_id = Id::unique("output-table-tab");
    let names = output.tabs.as_ref().map_or(&[][..], |t| &t.names[..]);
    let kept: Option<String> = ui.data(|d| d.get_temp(tab_id));
    let mut tab = kept.and_then(|k| names.iter().position(|n| *n == k)).unwrap_or(0).min(output.tab_count() - 1);
    response.tab = tab;
    let mut table_view = |ui: &mut Ui, response: &mut OutputResponse| {
        if let Some(tabs) = &output.tabs {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                for (k, name) in tabs.names.iter().enumerate() {
                    if ui.selectable_value(&mut tab, k, name).clicked() {
                        ui.data_mut(|d| d.insert_temp(tab_id, name.clone()));
                    }
                }
            });
            response.tab = tab;
        }
        if let Some(t) = output.table_of(tab) {
            // a row picked on the graph (maybe while the Table tab was hidden)
            let pending: Option<usize> = ui.data_mut(|d| d.remove_temp(pending_id));
            let table_id = if tab == 0 { id.with("table") } else { id.with(("table", tab)) };
            let select_rows = select_rows.or(pending.map(|r| (r, r)));
            let clear = ui.data_mut(|d| d.remove_temp::<bool>(clear_id)).unwrap_or(false);
            TableView { id: table_id, table: t, select_rows, clear }.show(ui, response);
        }
    };
    let plot_view = |ui: &mut Ui, plot: &PlotSpec| match show_plot(ui, id.with("plot"), plot, point) {
        Some(Some(i)) => ui.data_mut(|d| {
            d.insert_temp(point_id, i);
            d.insert_temp(pending_id, i);
        }),
        // a click away from the points: nothing is chosen, on the graph or in the table
        Some(None) => ui.data_mut(|d| {
            d.remove::<usize>(point_id);
            d.remove::<usize>(pending_id);
            d.insert_temp(clear_id, true);
        }),
        None => {}
    };
    match (&output.plot, layout) {
        (None, _) => table_view(ui, response),
        (Some(plot), OutputLayout::Split) => {
            egui::Panel::right(id.with("plot-panel"))
                .resizable(true)
                .default_size(ui.available_width() * 0.4)
                .size_range(160.0..=ui.available_width() * 0.8)
                .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(egui::Margin::same(6)))
                .show(ui, |ui| plot_view(ui, plot));
            egui::CentralPanel::no_frame().show(ui, |ui| table_view(ui, response));
        }
        (Some(plot), OutputLayout::Tabs) => {
            let tab_id = Id::unique("output-tab");
            let mut graph: bool = ui.data(|d| d.get_temp(tab_id)).unwrap_or(false);
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.selectable_value(&mut graph, false, "Table");
                ui.selectable_value(&mut graph, true, "Graph");
            });
            ui.data_mut(|d| d.insert_temp(tab_id, graph));
            if graph {
                egui::Frame::new().inner_margin(egui::Margin::same(6)).show(ui, |ui| plot_view(ui, plot));
            } else {
                table_view(ui, response);
            }
        }
    }
    if let Some(row) = response.clicked_row.or(select_rows.map(|r| r.0)) {
        ui.data_mut(|d| d.insert_temp(point_id, row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> TableData {
        TableData {
            columns: vec![Column::new("Number"), Column::new("Wave Frequency [rad/s]"), Column::new("A3,3 [kg]")],
            rows: vec![
                vec![Cell::Int(1), Cell::Num(0.4), Cell::Num(f64::from(2712.9802f32))],
                vec![Cell::Int(2), Cell::Num(0.6), Cell::Num(1.0 / 3.0)],
            ],
        }
    }

    #[test]
    fn heatmap_cells_reach_half_way_to_their_neighbours() {
        assert_eq!(cell_edges(&[0.0, 1.0, 3.0]), vec![-0.5, 0.5, 2.0, 4.0]);
        assert_eq!(cell_edges(&[2.0]).len(), 2);
        assert!(cell_edges(&[]).is_empty());
    }

    #[test]
    fn csv_has_full_precision_and_crlf() {
        assert_eq!(
            table_csv(&table()),
            "Number,Wave Frequency [rad/s],\"A3,3 [kg]\"\r\n1,0.4,2712.9802\r\n2,0.6,0.3333333333333333\r\n"
        );
    }

    #[test]
    fn copy_is_tab_separated_as_displayed() {
        assert_eq!(selection_tsv(&table(), (0, 1), (1, 2), false), "0.4\t2712.980225\r\n0.6\t0.333333");
        assert_eq!(selection_tsv(&table(), (1, 0), (1, 0), true), "Number\r\n2");
        assert_eq!(header_lines("Wave Frequency [rad/s]"), "Wave Frequency\n[rad/s]");
    }

    #[test]
    fn graph_steps_are_one_two_or_five_and_labelled_with_their_decimals() {
        assert_eq!(nice_step(0.46), 0.5);
        assert_eq!(nice_step(130.0), 200.0);
        assert_eq!(nice_step(0.07), 0.1);
        let mark = |value, step_size| egui_plot::GridMark { value, step_size };
        assert_eq!(tick_label(mark(1.5, 0.5), &(0.0..=2.0)), "1.5");
        assert_eq!(tick_label(mark(3000.0, 200.0), &(0.0..=1.0)), "3000");
        assert_eq!(tick_label(mark(0.25, 0.05), &(0.0..=1.0)), "0.25");
        assert_eq!(tick_label(mark(0.002, 0.001), &(0.0..=0.004)), "0.002");
        // an axis of small values only: in exponent notation, zero as "0"
        assert_eq!(tick_label(mark(0.0012, 0.0002), &(0.0..=0.002)), "1.2e-03");
        assert_eq!(tick_label(mark(-0.00004, 0.00002), &(-0.0001..=0.0)), "-4e-05");
        assert_eq!(tick_label(mark(0.0, 0.0002), &(0.0..=0.002)), "0");
        // a small step about a value of order one keeps its decimals
        assert_eq!(tick_label(mark(1.0002, 0.0002), &(1.0..=1.001)), "1.0002");
    }

    #[test]
    fn excel_text_pastes_into_the_editable_cells() {
        let mut t = table();
        t.columns[1].editable = true;
        t.columns[2].editable = true;
        let edit = |row, column, value| CellEdit { row, column, value };
        // a block from Excel (CRLF, trailing line break) at the top-left cell of the selection
        let (edits, skipped) = paste_edits(&t, (0, 1), (0, 1), "1.5\t2\r\n3\t4e2\r\n");
        assert_eq!(edits, vec![edit(0, 1, 1.5), edit(0, 2, 2.0), edit(1, 1, 3.0), edit(1, 2, 400.0)]);
        assert_eq!(skipped, 0);
        // rows beyond the table, the read-only first column and texts are left out
        let (edits, skipped) = paste_edits(&t, (1, 0), (1, 0), "9\t0.7\tx\n8\t1\t2");
        assert_eq!(edits, vec![edit(1, 1, 0.7)]);
        assert_eq!(skipped, 5);
        // one value fills the selection
        let (edits, _) = paste_edits(&t, (0, 1), (1, 2), "5\n");
        assert_eq!(edits.len(), 4);
        assert!(edits.iter().all(|e| e.value == 5.0));
    }
}
