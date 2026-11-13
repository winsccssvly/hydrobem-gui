//! The main window (REQUIREMENTS.md, Main layout): the project tree and Details on the left, the
//! viewport and the output pane on the right, the menu row with the Run button, the status bar.
//!
//! - `files`: the project and results files, the file actions and the unsaved prompt's
//!   request, the native file dialogs, File → Export.
//! - `solve`: Run/Stop, the solve and its progress (the Run, Clear Results and frequency
//!   buttons of the top bar, the list of each frequency worker's progress).
//! - `menus`: the shortcuts, files dropped onto the window, the menu bar, the status bar, the
//!   window's title.
//! - `edits`: the tree's and Details' actions on the project, the engine's view of the inputs and
//!   the viewport's scene.
//! - `modals`: the dialogs (messages, the unsaved-changes prompt, the memory warning, Stop and
//!   Exit, the discretization of a higher-order GDF file, About).

mod edits;
mod files;
mod menus;
mod modals;
mod solve;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use eframe::egui::{self, Color32, Frame, Margin, Panel, Stroke, Ui};
use gui_core::files::project_fingerprint;
use gui_core::hydrobem_engine::SolveProgress;
use gui_core::project::Project;
use gui_core::results::SolveResult;
use gui_core::scene::Focus;
use gui_core::solve::{SolveHandle, SolveRequest};
use serde::{Deserialize, Serialize};

use self::menus::file_drag;
use crate::details::{self, Derived, MeshTarget};
use crate::output::{self, Output, OutputLayout, OutputResponse};
use crate::results_view::{self, Selection};
use crate::style;
use crate::tree::{self, Expanded, Node, ResultNode};
use crate::viewport::Viewport;
use crate::widgets::strip;

const RECENT_LIMIT: usize = 8;
const SETTINGS_KEY: &str = "hydrobem-gui-settings";

/// The extensions of CAD files: not read here (cad2bem converts them), named in the status bar.
const CAD_EXTENSIONS: [&str; 4] = ["step", "stp", "iges", "igs"];

/// What the application remembers between sessions.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub recent: Vec<PathBuf>,
    pub output_layout: OutputLayout,
    pub show_field_points: bool,
    pub smoothing: bool,
    pub show_free_surface: bool,
    /// Of a mesh shown alone, an arrow along each panel's or patch's normal.
    pub show_normals: bool,
    /// Of a patch mesh shown alone, the spans of its discretization.
    pub show_spans: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recent: Vec::new(),
            output_layout: OutputLayout::Split,
            show_field_points: false,
            smoothing: true,
            show_free_surface: true,
            show_normals: false,
            show_spans: true,
        }
    }
}

/// An action that may first need the unsaved-changes prompt.
#[derive(Clone, Debug, PartialEq)]
pub enum FileAction {
    New,
    Open(Option<PathBuf>),
    OpenResults(Option<PathBuf>),
    Exit,
}

/// What a mesh file read is for: the library (and a target that takes it), or library mesh
/// `index` read again.
#[derive(Clone, Copy, Debug)]
enum MeshImport {
    Into(Option<MeshTarget>),
    Replace(usize),
}

enum Modal {
    Message {
        title: String,
        text: String,
    },
    Unsaved(FileAction),
    /// Exit asked while a solve runs: stop it first.
    StopAndExit,
    Memory {
        text: String,
    },
    /// Higher-order GDF files without an SPL file: the discretization of their patches, asked
    /// before they are read (with what to do with each).
    Discretization {
        files: Vec<(PathBuf, MeshImport)>,
        choice: gui_core::mesh::GdfDiscretization,
    },
    About,
}

/// What to do with the path of a file dialog once it has closed.
type PickThen = Box<dyn FnOnce(&mut App, PathBuf)>;

struct Running {
    handle: SolveHandle,
    /// The engine's progress at the last frame.
    progress: SolveProgress,
    /// The frequency workers of the solve (its rows before the engine's first snapshot).
    threads: usize,
    /// The ordinary frequencies (rad/s) of the solve, in the order of the results.
    omegas: Vec<f64>,
    body_names: Vec<String>,
    /// The project's fingerprint when the solve started: the inputs of its results.
    solved_for: String,
}

pub struct App {
    pub project: Project,
    pub file: Option<PathBuf>,
    saved_fingerprint: String,
    fingerprint: String,
    /// The fingerprint of the project the results are of (solved, or read with them).
    result_for: String,
    derived: Derived,
    derived_for: String,
    pub selected: Node,
    expanded: Expanded,
    pub result: Option<Arc<SolveResult>>,
    /// The incident wave amplitude the results are shown for (m): this session only, never saved;
    /// every result shown takes it (`SolveResult::with_wave_amplitude`).
    pub wave_amplitude: f64,
    pub selection: Selection,
    pub settings: Settings,
    running: Option<Running>,
    /// Frames to wait before starting a solve, so an open editor commits first.
    pending_run: u8,
    /// A built model waiting on the memory warning.
    pending_request: Option<SolveRequest>,
    pub status: String,
    status_ok: bool,
    finished: Option<String>,
    modal: Option<Modal>,
    pub viewport: Viewport,
    scene_for: String,
    focus: Focus,
    output: Output,
    /// A field point or mesh record picked in the viewport: its table row is selected.
    select_rows: Option<(usize, usize)>,
    /// The table tab shown in the last frame.
    table_tab: usize,
    title: String,
    allow_close: bool,
    /// Stop and Exit was chosen: exit once the solve has stopped.
    exit_when_stopped: bool,
    /// For the GUI smoke test: answers the unsaved prompt without a click.
    pub test_answer_unsaved: Option<bool>,
    /// The animation's phase ωt (rad) and the time of its last step.
    pub animation_phase: f64,
    animation_clock: Option<std::time::Instant>,
    /// The list of each frequency's progress is open (the button left of Run, while solving).
    pub show_frequencies: bool,
    /// A native file dialog open on a thread of its own (the window keeps drawing), and what to do
    /// with the path chosen.
    picking: Option<(Receiver<Option<PathBuf>>, PickThen)>,
    /// The window's context, for a repaint when a file dialog closes (None before the window).
    ctx: Option<egui::Context>,
}

/// The mark of results out of date (`App::results_outdated`), and what it means.
const OUTDATED: &str = "Results out of date";
const OUTDATED_TEXT: &str = "the inputs changed since the Run; these results are of the inputs at the Run. Run again \
     for results of the inputs as they are (or undo the change).";

/// What File → Export (or the table's menu) writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Export {
    /// WAMIT-format files with the period (s) as the first column
    WamitPeriod,
    /// WAMIT-format files with the frequency (rad/s) as the first column
    WamitOmega,
    /// every quantity of the results as a NumPy archive
    Npz,
    /// the table shown as CSV (the table's menu)
    Csv,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        style::apply(&cc.egui_ctx);
        if let Some(rs) = &cc.wgpu_render_state {
            crate::viewport::renderer::install(rs);
        }
        let settings = cc.storage.and_then(|s| eframe::get_value::<Settings>(s, SETTINGS_KEY)).unwrap_or_default();
        let mut app = App::with_settings(settings);
        app.ctx = Some(cc.egui_ctx.clone());
        app
    }

    pub fn with_settings(settings: Settings) -> App {
        let project = Project::default();
        let fingerprint = project_fingerprint(&project);
        App {
            project,
            file: None,
            saved_fingerprint: fingerprint.clone(),
            fingerprint,
            result_for: String::new(),
            derived: Derived::default(),
            derived_for: String::new(),
            selected: Node::Meshes,
            expanded: Expanded::default(),
            result: None,
            wave_amplitude: 1.0,
            selection: Selection::new(),
            settings,
            running: None,
            pending_run: 0,
            pending_request: None,
            status: String::new(),
            status_ok: true,
            finished: None,
            modal: None,
            viewport: Viewport::default(),
            scene_for: String::new(),
            focus: Focus::All,
            output: Output::default(),
            select_rows: None,
            table_tab: 0,
            title: String::new(),
            allow_close: false,
            exit_when_stopped: false,
            test_answer_unsaved: None,
            animation_phase: 0.0,
            animation_clock: None,
            show_frequencies: false,
            picking: None,
            ctx: None,
        }
    }

    /// The frame of the animated response of the selected result view, the phase advanced by the
    /// time since the last frame while it plays.
    fn animation_step(&mut self, ctx: &egui::Context) -> Option<gui_core::animation::Response> {
        let (Node::Result(node), Some(result)) = (self.selected, self.result.as_deref()) else {
            self.animation_clock = None;
            return None;
        };
        let Some(response) = results_view::animation_response(node, result, &self.selection, &self.project) else {
            self.animation_clock = None;
            return None;
        };
        let now = std::time::Instant::now();
        if self.selection.paused {
            self.animation_clock = None;
        } else {
            if let Some(last) = self.animation_clock {
                let dt = now.duration_since(last).as_secs_f64().min(0.1);
                let speed = results_view::speed_factor(&self.selection);
                self.animation_phase = (self.animation_phase + response.omega * dt * speed) % std::f64::consts::TAU;
            }
            self.animation_clock = Some(now);
            ctx.request_repaint();
        }
        Some(response)
    }

    pub fn is_modified(&self) -> bool {
        self.fingerprint != self.saved_fingerprint
    }

    /// The engine's progress of the running solve: the preparation, each limit and frequency.
    pub fn frequency_progress(&self) -> Option<&SolveProgress> {
        self.running.as_ref().map(|r| &r.progress)
    }

    pub fn is_solving(&self) -> bool {
        self.running.is_some()
    }

    pub fn output(&self) -> &Output {
        &self.output
    }

    /// The table tab shown (0 the first, `output().table`).
    pub fn table_tab(&self) -> usize {
        self.table_tab
    }

    /// Selects a row of the table shown in the next frame and scrolls to it (as a pick does).
    pub fn select_table_row(&mut self, row: usize) {
        self.select_rows = Some((row, row));
    }

    /// The engine's panels by role of the present project, as the viewport draws them.
    pub fn derived_roles(&self) -> Result<&gui_core::engine_model::RolePanels, &str> {
        self.derived.roles.as_ref().map_err(String::as_str)
    }

    /// The choices of the result views.
    pub fn selection_mut(&mut self) -> &mut crate::results_view::Selection {
        &mut self.selection
    }

    /// What the open dialog is, for the tests.
    pub fn modal_text(&self) -> Option<String> {
        match &self.modal {
            Some(Modal::Message { title, text }) => Some(format!("{title}: {text}")),
            Some(Modal::Unsaved(_)) => Some("unsaved".into()),
            Some(Modal::StopAndExit) => Some("stop and exit".into()),
            Some(Modal::Memory { text }) => Some(text.clone()),
            Some(Modal::Discretization { .. }) => Some("discretization".into()),
            Some(Modal::About) => Some("about".into()),
            None => None,
        }
    }

    pub fn close_modal(&mut self) {
        self.modal = None;
    }

    /// The discretization asked for higher-order GDF files without an SPL file, while it is asked.
    pub fn gdf_discretization_mut(&mut self) -> Option<&mut gui_core::mesh::GdfDiscretization> {
        match &mut self.modal {
            Some(Modal::Discretization { choice, .. }) => Some(choice),
            _ => None,
        }
    }

    /// Asks for the discretization of a higher-order GDF file without an SPL file before reading
    /// it: one question for every such file of an import.
    fn ask_discretization(&mut self, path: PathBuf, then: MeshImport) {
        match &mut self.modal {
            Some(Modal::Discretization { files, .. }) => files.push((path, then)),
            _ => {
                self.modal = Some(Modal::Discretization {
                    files: vec![(path, then)],
                    choice: gui_core::mesh::GdfDiscretization::default(),
                })
            }
        }
    }

    /// Reads a mesh file into the library (and gives it to its target) or in place of library mesh
    /// `index`, with the discretization chosen for a higher-order GDF file without SPL file.
    fn read_mesh(&mut self, path: &Path, then: MeshImport, choice: Option<&gui_core::mesh::GdfDiscretization>) {
        match then {
            MeshImport::Into(target) => match details::import_mesh(&mut self.project, target, path, choice) {
                Ok(index) => {
                    let entry = &self.project.meshes[index];
                    self.status = format!("Imported {} as {}", path.display(), entry.name);
                    if let Some(r) = &entry.mesh.report {
                        self.status += &format!(" with cad2bem's report ({})", r.state);
                    }
                    self.viewport.refit = true;
                    if target.is_none() {
                        self.selected = Node::Mesh(index);
                    }
                }
                Err(e) => self.message("Cannot read the mesh", e),
            },
            MeshImport::Replace(index) => {
                let read = match choice {
                    Some(choice) => gui_core::mesh::Mesh::load_gdf_with(path, choice),
                    None => gui_core::mesh::Mesh::load(path),
                };
                match read {
                    Ok(mesh) => {
                        self.project.replace_mesh(index, mesh);
                        self.status = format!("Read {}", path.display());
                        self.viewport.refit = true;
                    }
                    Err(e) => self.message("Cannot read the mesh", e),
                }
            }
        }
    }

    fn message(&mut self, title: &str, text: impl Into<String>) {
        self.modal = Some(Modal::Message { title: title.into(), text: text.into() });
    }

    fn remember(&mut self, path: &Path) {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        self.settings.recent.retain(|p| p != &path);
        self.settings.recent.insert(0, path);
        self.settings.recent.truncate(RECENT_LIMIT);
    }

    /// A new project replaced the old one: fresh state, the camera fitted.
    fn replace_project(&mut self, project: Project, file: Option<PathBuf>) {
        self.project = project;
        self.file = file;
        self.status_ok = true;
        self.fingerprint = project_fingerprint(&self.project);
        self.saved_fingerprint = self.fingerprint.clone();
        self.result = None;
        self.finished = None;
        self.selection = Selection::new();
        self.selected = Node::Meshes;
        self.expanded = Expanded::default();
        self.viewport.refit = true;
        self.viewport.highlight = None;
        self.derived_for.clear();
        self.scene_for.clear();
    }

    // ---- Frame ---------------------------------------------------------------------------------

    /// One frame of the whole window.
    pub fn frame(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // A close (window button, Alt+F4, File → Exit) asks about unsaved changes first.
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && (self.is_modified() || self.is_solving())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.modal.is_none() {
                self.request(FileAction::Exit);
            }
            if self.allow_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        self.poll_pick();
        self.poll_solve();
        // Stop and Exit: the solve has stopped, the exit goes on (the unsaved prompt first)
        if self.exit_when_stopped && !self.is_solving() {
            self.exit_when_stopped = false;
            self.request(FileAction::Exit);
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        file_drag(&ctx);
        if self.modal.is_none() {
            self.shortcuts(&ctx);
            self.dropped_files(&ctx);
        }
        self.window_title(&ctx);
        self.project.sync_placed_lids();
        self.project.update_field_profile();
        // the results shown at the session's wave amplitude (a new solve, a results file, an edit)
        let rescaled = self
            .result
            .as_ref()
            .filter(|r| r.wave_amplitude != self.wave_amplitude)
            .map(|r| Arc::new(r.with_wave_amplitude(self.wave_amplitude)));
        if rescaled.is_some() {
            self.result = rescaled;
        }
        self.fingerprint = project_fingerprint(&self.project);
        self.refresh_derived();
        if !self.selected.valid(&self.project, self.result.as_deref()) {
            self.selected = Node::Meshes;
        }

        Panel::top("menu")
            .frame(
                Frame::new()
                    .fill(style::TOP_BAR)
                    .inner_margin(Margin::symmetric(4, 1))
                    .stroke(Stroke::new(1.0, style::GRID_LINE)),
            )
            .show(ui, |ui| self.menu_bar(ui));
        Panel::bottom("status")
            .frame(Frame::new().fill(style::WINDOW).inner_margin(Margin::symmetric(4, 2)))
            .show(ui, |ui| self.status_bar(ui));

        let mut tree_actions = Vec::new();
        let mut details_actions = Vec::new();
        let mut overlay = None;
        let mut body_overlay = None;
        let mut output = Output::default();
        Panel::left("left")
            .resizable(true)
            .default_size(420.0)
            .size_range(260.0..=900.0)
            .frame(Frame::new().fill(Color32::WHITE))
            .show(ui, |ui| {
                Panel::bottom("details")
                    .resizable(true)
                    .default_size(340.0)
                    .size_range(120.0..=2000.0)
                    .frame(Frame::new().fill(Color32::WHITE))
                    .show(ui, |ui| {
                        strip(ui, &format!("Details of {}", self.selected.title(&self.project)));
                        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            match (self.selected, &self.result) {
                                (Node::Result(r), Some(result)) => {
                                    if self.result_for != self.fingerprint {
                                        crate::widgets::caution(ui, &format!("{OUTDATED}: {OUTDATED_TEXT}"));
                                    }
                                    let (o, f) = results_view::show(
                                        ui,
                                        r,
                                        result,
                                        &mut self.selection,
                                        &mut self.settings.smoothing,
                                        &mut self.wave_amplitude,
                                    );
                                    output = o;
                                    match f {
                                        // points given one by one: a square at each
                                        Some(results_view::Overlay::Field(f)) if result.field_grid.list.is_some() => {
                                            body_overlay = Some(gui_core::scene::BodyOverlay {
                                                normals: vec![[0.0, 0.0, 1.0]; f.points.len()],
                                                points: f.points,
                                                areas: Vec::new(),
                                                faces: Default::default(),
                                                values: f.values,
                                                label: f.label,
                                                plane: false,
                                                cover: Default::default(),
                                            })
                                        }
                                        Some(results_view::Overlay::Field(f)) => {
                                            overlay = Some(gui_core::scene::FieldOverlay {
                                                grid: result.field_grid.clone(),
                                                points: f.points,
                                                values: f.values,
                                                label: f.label,
                                            })
                                        }
                                        Some(results_view::Overlay::Body(b)) => body_overlay = Some(b),
                                        None => {}
                                    }
                                }
                                (Node::Result(_), None) => {
                                    crate::widgets::section(ui, "Results", |ui| {
                                        crate::widgets::message(
                                            ui,
                                            "No results yet: press Run (top right, or F5) to solve.",
                                        )
                                    });
                                }
                                (node, _) => {
                                    details::show(ui, &mut self.project, node, &self.derived, &mut details_actions);
                                    output = details::output(&self.project, node, &self.derived);
                                }
                            }
                        });
                    });
                egui::CentralPanel::no_frame().show(ui, |ui| {
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        tree::show(
                            ui,
                            &self.project,
                            self.result.as_deref(),
                            self.selected,
                            &mut self.expanded,
                            &mut tree_actions,
                        );
                    });
                });
            });
        self.output = output;
        self.apply_tree(tree_actions);
        self.apply_details(details_actions);
        self.fingerprint = project_fingerprint(&self.project);
        self.refresh_derived();
        let animated = self.animation_step(&ctx);
        self.refresh_scene(overlay.as_ref(), body_overlay.as_ref(), animated.as_ref());

        let mut response = OutputResponse::default();
        let select_rows = self.select_rows.take();
        egui::CentralPanel::no_frame().show(ui, |ui| {
            Panel::bottom("output")
                .resizable(true)
                .default_size(232.0)
                .size_range(80.0..=2000.0)
                .frame(Frame::new().fill(Color32::WHITE))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    output::show(ui, &self.output, self.settings.output_layout, select_rows, &mut response);
                });
            egui::CentralPanel::no_frame().show(ui, |ui| {
                let player = animated.as_ref().map(|r| crate::viewport::Player {
                    omega: r.omega,
                    phase: self.animation_phase,
                    paused: self.selection.paused,
                });
                self.viewport.player = player;
                self.viewport.problem = self.derived.roles.as_ref().err().filter(|e| !e.is_empty()).cloned();
                let picked = self.viewport.show(ui);
                // the player's buttons and drags: the next frame draws the new phase
                if let (Some(before), Some(after)) = (player, self.viewport.player)
                    && before != after
                {
                    self.animation_phase = after.phase;
                    self.selection.paused = after.paused;
                    ui.ctx().request_repaint();
                }
                if self.viewport.clicked_away {
                    // only a table there is takes the flag (else it would clear a later one)
                    if self.output.table_of(self.table_tab).is_some() {
                        output::clear_selection(ui.ctx(), &self.output);
                    }
                    // the status bar no longer names what was picked
                    if ["Panel ", "Patch ", "Field point "].iter().any(|p| self.status.starts_with(p)) {
                        self.status.clear();
                    }
                    ui.ctx().request_repaint();
                }
                if let (Some(r), Node::Mesh(i)) = (self.viewport.picked_record, self.selected) {
                    let what =
                        if self.project.meshes.get(i).is_some_and(|e| e.mesh.is_patch()) { "Patch" } else { "Panel" };
                    self.status = format!("{what} {} of the mesh", r + 1);
                    let per = self.output.rows_per_record(self.table_tab);
                    self.select_rows = Some((r * per, r * per + per - 1));
                }
                if let Some(picked) = picked {
                    let p = self.viewport.scene.field_points[picked];
                    self.status = format!("Field point {}: x = {}, y = {}, z = {}", picked + 1, p[0], p[1], p[2]);
                    if matches!(self.selected, Node::Result(ResultNode::Field(_))) {
                        self.select_rows = Some((picked, picked));
                    }
                }
            });
        });
        if !response.edits.is_empty() || response.paste_skipped > 0 {
            let count = response.edits.len();
            let refused = response
                .edits
                .into_iter()
                .filter(|edit| !details::apply_edit(&mut self.project, self.selected, *edit))
                .count()
                + response.paste_skipped;
            if count + response.paste_skipped > 1 {
                self.status =
                    format!("Pasted {} values; {refused} not accepted.", count + response.paste_skipped - refused);
            } else if refused > 0 {
                self.status = "The value was not accepted.".into();
            }
        }
        if let Some(row) = response.clicked_row
            && matches!(self.selected, Node::Result(ResultNode::Field(_)))
        {
            self.viewport.highlight = Some(row);
        }
        self.table_tab = response.tab;
        // the records of a mesh selected in its table are highlighted on it (a pick in the viewport
        // reaches the table in the next frame)
        if !matches!(self.selected, Node::Mesh(_)) {
            self.viewport.records.clear();
        } else if self.viewport.picked_record.is_none() && !self.viewport.clicked_away {
            let per = self.output.rows_per_record(response.tab);
            self.viewport.records = response.selected_rows.map_or(Vec::new(), |(a, b)| (a / per..=b / per).collect());
        }
        if response.export_csv {
            self.export(Export::Csv);
        }

        self.modals(&ctx);
        if self.pending_run > 0 {
            self.pending_run -= 1;
            if self.pending_run == 0 {
                self.start_run(&ctx);
            } else {
                ctx.request_repaint();
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }
}
