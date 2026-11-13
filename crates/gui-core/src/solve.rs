//! A solve: the engine model of the project built on the UI thread (so the UI keeps editing the
//! project), solved on a worker thread with progress reports and cancellation.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread::JoinHandle;
use std::time::Instant;

use hydrobem_engine::{CancelToken, ErrorKind, Location, MemoryEstimate, Model, Progress, SolveProgress};

use crate::engine_model::{BuildOptions, build_model};
use crate::field_points::FieldPointSelection;
use crate::project::{FieldPoints, PanelSurface, Project};
use crate::results::SolveResult;

#[derive(Clone, Debug, PartialEq)]
pub struct SolveError {
    pub message: String,
    /// Where the engine located the error, when it did.
    pub location: Option<Box<Location>>,
    pub cancelled: bool,
}

impl SolveError {
    pub fn new(message: impl Into<String>) -> SolveError {
        SolveError { message: message.into(), location: None, cancelled: false }
    }

    /// A panic of the solve thread, with its message when it has one.
    fn panicked(payload: &(dyn std::any::Any + Send)) -> SolveError {
        let text = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "no message".into());
        SolveError::new(format!("Internal error of the solve (please report it): {text}"))
    }

    fn engine(e: hydrobem_engine::Error) -> SolveError {
        let l = *e.location();
        let located = [l.body, l.mesh, l.element, l.field_point, l.lid, l.frequency].iter().any(Option::is_some);
        SolveError {
            message: e.message().to_string(),
            location: located.then(|| Box::new(l)),
            cancelled: e.kind() == ErrorKind::Cancelled,
        }
    }
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// The engine model of one solve.
pub struct SolveRequest {
    pub model: Model,
    /// The active bodies, in the model's order.
    pub body_names: Vec<String>,
    /// Every body of the model, the suppressed ones too (the error locations count them).
    pub model_bodies: Vec<String>,
    /// The grid at solve time.
    pub field_grid: FieldPoints,
    /// The submitted points.
    pub field_points: FieldPointSelection,
}

/// What the GUI checks first (no body, no frequency, a body without its mesh), then the engine's
/// message for a value it rejects.
pub fn make_solve_request(project: &Project) -> Result<SolveRequest, SolveError> {
    if project.active_bodies().next().is_none() {
        return Err(SolveError::new("Add at least one non-suppressed body."));
    }
    if project.waves.frequencies.inputs.is_empty() {
        return Err(SolveError::new("Add at least one wave frequency."));
    }
    for body in project.active_bodies() {
        // a body of plates alone takes the mesh of its first plate (the engine's rule)
        if !body.has_geometry() {
            return Err(SolveError::new(format!(
                "{}: import a body GDF mesh, or give it a free surface plate.",
                body.name
            )));
        }
        if body.has_roll_or_pitch() {
            return Err(SolveError::new(format!(
                "{}: the engine accepts body yaw only; set Roll and Pitch to zero.",
                body.name
            )));
        }
        if let Some(k) = body.dipoles.iter().position(PanelSurface::missing_mesh) {
            return Err(SolveError::new(format!("{}: choose the mesh of Dipole {}.", body.name, k + 1)));
        }
        if let Some(k) = body.pressure_surfaces.iter().position(PanelSurface::missing_mesh) {
            return Err(SolveError::new(format!("{}: choose the mesh of Pressure Surface {}.", body.name, k + 1)));
        }
        if let Some(k) = body.free_surface_plates.iter().position(PanelSurface::missing_mesh) {
            return Err(SolveError::new(format!("{}: choose the mesh of Free Surface Plate {}.", body.name, k + 1)));
        }
        if let Some(k) = body.dipole_dampers.iter().position(PanelSurface::missing_mesh) {
            return Err(SolveError::new(format!("{}: choose the mesh of Dipole Damper {}.", body.name, k + 1)));
        }
    }
    if let Some(k) = project.fixed_dampers.iter().position(PanelSurface::missing_mesh) {
        return Err(SolveError::new(format!("Fixed Dampers: choose the mesh of Fixed Damper {}.", k + 1)));
    }
    let built = build_model(project, &BuildOptions::default()).map_err(SolveError::engine)?;
    Ok(SolveRequest {
        model: built.model,
        body_names: built.body_names,
        model_bodies: built.model_bodies,
        field_grid: project.field_points.clone(),
        field_points: built.field_points,
    })
}

/// The memory the solve will use (the model is validated and its geometry built; nothing is
/// solved).
pub fn estimate_memory(request: &mut SolveRequest) -> Result<MemoryEstimate, SolveError> {
    request.model.memory_estimate().map_err(SolveError::engine)
}

/// The request solved although its memory estimate exceeds the free memory (the user's answer to
/// the GUI's warning): the engine's own check (`options.memory_check`) is turned off in this
/// request's model only, so the engine does not refuse it for the same reason. The project keeps
/// the engine's default (on); the results file of this solve records the check as off.
pub fn without_memory_check(request: &mut SolveRequest) -> Result<(), SolveError> {
    let m = &mut request.model;
    let o = m.object("options").map_err(SolveError::engine)?;
    m.set(o, "memory_check", false).map_err(SolveError::engine)
}

/// One report of a running solve, or its end.
pub enum SolveEvent {
    Progress(Progress),
    Finished(Box<Result<SolveResult, SolveError>>),
}

/// A solve running on a worker thread.
pub struct SolveHandle {
    events: Receiver<SolveEvent>,
    cancel: CancelToken,
    thread: Option<JoinHandle<()>>,
    pub started: Instant,
}

impl SolveHandle {
    /// Starts the solve of `request`. `wake` is called after every event (to repaint the UI).
    pub fn start(request: SolveRequest, wake: impl Fn() + Send + Sync + 'static) -> SolveHandle {
        let (tx, events) = channel();
        let cancel = request.model.cancel_token();
        let started = Instant::now();
        let thread = std::thread::Builder::new()
            .name("hydrobem-solve".into())
            .spawn(move || {
                let progress_tx = tx.clone();
                let wake_progress = &wake;
                // a panic (an engine bug) ends the solve with an error, not with a silent thread
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let SolveRequest { mut model, body_names, model_bodies, field_grid, field_points } = request;
                    let panels = crate::results::solved_panels(&mut model);
                    let bidirectional = crate::results::asks_bidirectional_drift(&model);
                    let surface = crate::results::solved_surface(&mut model);
                    let planes = crate::results::solved_planes(&mut model);
                    model
                        .solve(|p: &Progress| {
                            let _ = progress_tx.send(SolveEvent::Progress(*p));
                            wake_progress();
                        })
                        .map(|results| {
                            let mut result = SolveResult::new(
                                results.clone(),
                                &body_names,
                                field_points.points.clone(),
                                field_points.sources(),
                            );
                            result.field_grid = field_grid;
                            result.model_bodies = model_bodies;
                            result.panels = panels;
                            result.surface = surface;
                            result.planes = planes;
                            result.bidirectional_drift = bidirectional;
                            result.generated_field_points = field_points.generated;
                            result.total_seconds = started.elapsed().as_secs_f64();
                            result
                        })
                        .map_err(SolveError::engine)
                }))
                .unwrap_or_else(|panic| Err(SolveError::panicked(panic.as_ref())));
                let _ = tx.send(SolveEvent::Finished(Box::new(outcome)));
                wake();
            })
            .expect("a thread for the solve");
        SolveHandle { events, cancel, thread: Some(thread), started }
    }

    /// Requests cancellation; the solve then finishes with a cancelled error.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// The engine's progress of the solve at this instant: the preparation, each limit and each
    /// frequency with its state, stage and share done (None when the engine cannot say).
    pub fn progress(&self) -> Option<SolveProgress> {
        self.cancel.progress().ok()
    }

    /// The events since the last call.
    pub fn poll(&mut self) -> Vec<SolveEvent> {
        let mut events = Vec::new();
        let ended = loop {
            match self.events.try_recv() {
                Ok(e) => events.push(e),
                Err(TryRecvError::Empty) => break false,
                Err(TryRecvError::Disconnected) => break true,
            }
        };
        let finished = events.iter().any(|e| matches!(e, SolveEvent::Finished(_)));
        if ended && !finished && self.thread.is_some() {
            // the thread is gone without a result: the solve still ends
            events
                .push(SolveEvent::Finished(Box::new(Err(SolveError::new("the solve thread ended without a result")))));
        }
        if (finished || ended)
            && let Some(t) = self.thread.take()
        {
            let _ = t.join();
        }
        events
    }

    /// Blocks until the solve ends (tests, batch use).
    pub fn wait(mut self) -> Result<SolveResult, SolveError> {
        loop {
            match self.events.recv() {
                Ok(SolveEvent::Finished(outcome)) => {
                    if let Some(t) = self.thread.take() {
                        let _ = t.join();
                    }
                    return *outcome;
                }
                Ok(SolveEvent::Progress(_)) => {}
                Err(_) => return Err(SolveError::new("the solve thread ended without a result")),
            }
        }
    }
}

impl Drop for SolveHandle {
    fn drop(&mut self) {
        if let Some(t) = self.thread.take() {
            self.cancel.cancel();
            let _ = t.join();
        }
    }
}
