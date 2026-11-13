//! The project and results files, the file dialogs, File → Export.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{TryRecvError, channel};

use gui_core::files::{
    OpenedResults, export_npz, export_wamit, load_project, open_results, project_fingerprint, save_project,
    save_results,
};
use gui_core::project::Project;

use super::{App, Export, FileAction, Modal};
use crate::output;
use crate::tree::{Node, ResultNode};

impl App {
    // ---- Files ---------------------------------------------------------------------------------

    /// Runs a file action, asking about unsaved changes first.
    pub fn request(&mut self, action: FileAction) {
        if self.is_solving() && action != FileAction::Exit {
            self.message("HydroBEM", "Stop the running solve first.");
            return;
        }
        // exit while solving: stop the solve first (asked), then exit as usual once it has stopped,
        // so that the window never waits for the engine
        if self.is_solving() {
            match self.test_answer_unsaved {
                Some(true) => self.stop_and_exit(),
                Some(false) => {}
                None => self.modal = Some(Modal::StopAndExit),
            }
            return;
        }
        if self.is_modified() {
            match self.test_answer_unsaved {
                Some(true) => {
                    if self.save(false) {
                        self.perform(action);
                    }
                }
                Some(false) => self.perform(action),
                None => self.modal = Some(Modal::Unsaved(action)),
            }
        } else {
            self.perform(action);
        }
    }

    pub(super) fn perform(&mut self, action: FileAction) {
        match action {
            FileAction::New => {
                self.replace_project(Project::default(), None);
                self.status = "New project".into();
            }
            FileAction::Open(path) => match path {
                // a results file opens with its results (Save would otherwise overwrite it)
                Some(p) if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("hbr")) => self.open_results_file(&p),
                Some(p) => self.open_project(&p),
                None => self.pick_then(
                    || {
                        rfd::FileDialog::new()
                            .add_filter("HydroBEM model", &["hbm", "hbr"])
                            .set_title("Open Project")
                            .pick_file()
                    },
                    |app, p| app.perform(FileAction::Open(Some(p))),
                ),
            },
            FileAction::OpenResults(path) => match path {
                Some(p) => self.open_results_file(&p),
                None => self.pick_then(
                    || {
                        rfd::FileDialog::new()
                            .add_filter("HydroBEM results", &["hbr"])
                            .set_title("Open Results")
                            .pick_file()
                    },
                    |app, p| app.open_results_file(&p),
                ),
            },
            FileAction::Exit => self.allow_close = true,
        }
    }

    /// Opens a native file dialog on a thread of its own, so that the window keeps drawing (the
    /// clock of a solve or a conversion goes on), and calls `then` with the path chosen. One at a
    /// time: while one is open, another is not started.
    pub(super) fn pick_then(
        &mut self,
        dialog: impl FnOnce() -> Option<PathBuf> + Send + 'static,
        then: impl FnOnce(&mut App, PathBuf) + 'static,
    ) {
        if self.picking.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(dialog());
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
        self.picking = Some((rx, Box::new(then)));
    }

    /// The path of the open file dialog, once it has closed.
    pub(super) fn poll_pick(&mut self) {
        let Some((rx, _)) = &self.picking else { return };
        match rx.try_recv() {
            Ok(path) => {
                let (_, then) = self.picking.take().expect("a dialog");
                if let Some(path) = path {
                    then(self, path);
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.picking = None,
        }
    }

    pub fn open_project(&mut self, path: &Path) {
        match load_project(path) {
            Ok((project, warnings)) => {
                self.replace_project(project, Some(path.to_path_buf()));
                self.remember(path);
                self.status = format!("Opened {}", path.display());
                if !warnings.is_empty() {
                    self.message("Opened with changes", warnings.join("\n"));
                }
            }
            Err(e) => self.message("Cannot open the project", format!("{}\n\n{e}", path.display())),
        }
    }

    /// A model handed over in a file the GUI does not own (`hydrobem_gui.show` of a model): opened
    /// as a new unsaved project, Save asking for a file, the file not remembered.
    pub fn open_untitled(&mut self, path: &Path) {
        match load_project(path) {
            Ok((project, warnings)) => {
                self.replace_project(project, None);
                self.saved_fingerprint.clear();
                self.status = "Opened an unsaved model".into();
                if !warnings.is_empty() {
                    self.message("Opened with changes", warnings.join("\n"));
                }
            }
            Err(e) => self.message("Cannot open the model", e.to_string()),
        }
    }

    pub fn open_results_file(&mut self, path: &Path) {
        match open_results(path) {
            Ok(OpenedResults { project, result, warnings }) => {
                // Save writes this results file again, or the model beside it once changed
                self.replace_project(project, Some(path.to_path_buf()));
                self.result = Some(Arc::new(result));
                self.result_for = self.fingerprint.clone();
                self.selected = Node::Result(ResultNode::Summary);
                self.remember(path);
                self.status = format!("Opened results {}", path.display());
                if !warnings.is_empty() {
                    self.message("Opened with changes", warnings.join("\n"));
                }
            }
            Err(e) => self.message("Cannot open the results", format!("{}\n\n{e}", path.display())),
        }
    }

    /// Whether Save writes the results file (.hbr): there are results, and of the project as it is
    /// (not changed since the Run). Else it writes the model (.hbm).
    pub fn saves_results(&self) -> bool {
        self.result.is_some() && self.result_for == self.fingerprint
    }

    /// Whether the results shown are out of date: of the inputs at the Run, changed since. Undoing
    /// the change makes them current again.
    pub fn results_outdated(&self) -> bool {
        self.result.is_some() && self.result_for != self.fingerprint
    }

    /// Saves the project (to a new file with `as_new`, or when it has none); false when cancelled
    /// or failed. Once solved (`saves_results`) Save writes the results file (.hbr, the model with
    /// its results), else the model file (.hbm), beside the project's file under its name. Save As
    /// then offers both: the file type chosen (its extension) says which is written.
    pub fn save(&mut self, as_new: bool) -> bool {
        let results = self.saves_results();
        let extension = if results { "hbr" } else { "hbm" };
        let path = match (&self.file, as_new) {
            (Some(p), false) => Some(p.with_extension(extension)),
            _ => {
                let mut dialog = rfd::FileDialog::new().set_title("Save As");
                if results {
                    dialog = dialog.add_filter("HydroBEM results", &["hbr"]);
                }
                dialog = dialog.add_filter("HydroBEM model", &["hbm"]);
                dialog.set_file_name(format!("{}.{extension}", self.file_stem("project"))).save_file()
            }
        };
        let Some(mut path) = path else { return false };
        if path.extension().is_none() {
            path.set_extension(extension);
        }
        if results && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("hbr")) {
            return self.save_results_to(&path);
        }
        self.save_to(&path)
    }

    /// The name of the project's file without its extension, else `fallback`.
    fn file_stem(&self, fallback: &str) -> String {
        self.file
            .as_ref()
            .and_then(|f| f.file_stem())
            .map_or_else(|| fallback.to_string(), |s| s.to_string_lossy().into_owned())
    }

    pub fn save_to(&mut self, path: &Path) -> bool {
        match save_project(&self.project, path) {
            Ok(()) => {
                self.file = Some(path.to_path_buf());
                self.status_ok = true;
                self.saved_fingerprint = project_fingerprint(&self.project);
                self.remember(path);
                self.status = format!("Saved {}", path.display());
                true
            }
            Err(e) => {
                self.message("Cannot save the project", e);
                false
            }
        }
    }

    pub fn save_results_to(&mut self, path: &Path) -> bool {
        let Some(result) = &self.result else { return false };
        match save_results(result, path) {
            Ok(()) => {
                // the results file holds the project too, when the results are of it as it is
                if self.result_for == self.fingerprint {
                    self.file = Some(path.to_path_buf());
                    self.status_ok = true;
                    self.saved_fingerprint = self.fingerprint.clone();
                }
                self.remember(path);
                self.status = format!("Saved results {}", path.display());
                true
            }
            Err(e) => {
                self.message("Cannot save the results", e);
                false
            }
        }
    }

    /// File → Export: the results as WAMIT-format files (`NAME.out` with `NAME.1`, `.2`, ...; the
    /// first column the period in s or the frequency in rad/s), every quantity of the results as a
    /// NumPy archive (`.npz`); the table's own menu exports the table shown as CSV.
    pub(super) fn export(&mut self, kind: Export) {
        let table_name: String =
            self.output.title.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' }).collect();
        let table_name = table_name.trim().to_string();
        // the table as it is now, written where the user chooses
        let bytes = match kind {
            Export::WamitPeriod | Export::WamitOmega | Export::Npz => {
                if self.result.is_none() {
                    return;
                }
                None
            }
            Export::Csv => Some(self.output.table.as_ref().map(|t| output::table_csv(t).into_bytes())),
        };
        if bytes.as_ref().is_some_and(|b| b.is_none()) {
            return;
        }
        let bytes = bytes.flatten();
        let (filter, extension, name) = match kind {
            Export::WamitPeriod => ("WAMIT output, period (s)", "out", self.file_stem("results")),
            Export::WamitOmega => ("WAMIT output, frequency (rad/s)", "out", self.file_stem("results")),
            Export::Npz => ("NumPy archive", "npz", self.file_stem("results")),
            Export::Csv => ("CSV", "csv", table_name),
        };
        self.pick_then(
            move || {
                rfd::FileDialog::new()
                    .set_title("Export")
                    .add_filter(filter, &[extension])
                    .set_file_name(format!("{name}.{extension}"))
                    .save_file()
            },
            move |app, path| match (kind, bytes) {
                (Export::WamitPeriod | Export::WamitOmega, _) => {
                    let Some(result) = &app.result else { return };
                    let out = with_extension(&path, "out");
                    match export_wamit(result, &out, kind == Export::WamitOmega) {
                        Ok(()) => app.status = format!("Exported {} with its WAMIT files", out.display()),
                        Err(e) => app.message("Cannot export the results", e),
                    }
                }
                (Export::Npz, _) => {
                    let Some(result) = &app.result else { return };
                    let out = with_extension(&path, "npz");
                    match export_npz(result, &out) {
                        Ok(()) => app.status = format!("Exported {}", out.display()),
                        Err(e) => app.message("Cannot export the results", e),
                    }
                }
                (_, Some(bytes)) => {
                    // a temporary file beside the target, renamed over it; removed when the write
                    // or the rename fails (a target open in another program)
                    let temp = with_extension(&path, &format!("{extension}.saving"));
                    match std::fs::write(&temp, bytes).and_then(|()| std::fs::rename(&temp, &path)) {
                        Ok(()) => app.status = format!("Exported {}", path.display()),
                        Err(e) => {
                            let _ = std::fs::remove_file(&temp);
                            app.message("Cannot export the table", e.to_string());
                        }
                    }
                }
                (_, None) => {}
            },
        );
    }
}

/// The file dialog of the mesh files the GUI reads (GDF, HBG).
pub(super) fn mesh_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new()
        .add_filter("Mesh (GDF, HBG)", &gui_core::mesh::MESH_EXTENSIONS)
        .add_filter("WAMIT GDF", &["gdf"])
        .add_filter("HydroBEM geometry", &["hbg"])
}

/// The file the user chose with the extension of the export: kept when it has it, else added (a
/// name the user typed with another extension keeps it: `run.v2` writes `run.v2.npz`, nothing the
/// dialog did not show is replaced).
fn with_extension(path: &std::path::Path, extension: &str) -> std::path::PathBuf {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case(extension)) {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(extension);
    name.into()
}

#[cfg(test)]
mod export_name_tests {
    use super::with_extension;
    use std::path::Path;

    #[test]
    fn an_export_keeps_the_name_the_user_chose() {
        assert_eq!(with_extension(Path::new("a/run.npz"), "npz"), Path::new("a/run.npz"));
        assert_eq!(with_extension(Path::new("a/run.NPZ"), "npz"), Path::new("a/run.NPZ"));
        assert_eq!(with_extension(Path::new("a/run"), "out"), Path::new("a/run.out"));
        assert_eq!(with_extension(Path::new("a/run.v2"), "npz"), Path::new("a/run.v2.npz"));
    }
}
