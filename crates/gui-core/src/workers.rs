//! The progress of a solve by worker, as the engine reports it (`hb_model_progress`): its
//! frequency workers (frequency_threads of them, each with its fixed number) and what each is on,
//! the phase of the solve and the steps of its second order. The GUI only words them.

use hydrobem_engine::{
    Phase, RunState, SecondOrderProgress, SecondOrderStep, SolveProgress, Stage, WorkerProgress, WorkerState,
    WorkerTask,
};

/// The rows of the worker table: the engine's workers, or `threads` idle ones (at least one) before
/// its first snapshot of the solve.
pub fn worker_rows(p: &SolveProgress, threads: usize) -> Vec<WorkerProgress> {
    if p.workers.is_empty() { vec![WorkerProgress::default(); threads.max(1)] } else { p.workers.clone() }
}

/// The share of a worker's item done for its bar: its fraction while it runs, 0 when idle, 1 once
/// the solve has ended.
pub fn worker_fraction(w: &WorkerProgress) -> f64 {
    match w.state {
        WorkerState::Running => w.fraction,
        WorkerState::Idle => 0.0,
        WorkerState::Done => 1.0,
    }
}

/// What a worker does and on what. `omegas` are the ordinary frequencies (rad/s) in the order of
/// the results; when they do not match the engine's, ω is left out.
pub fn worker_status(p: &SolveProgress, w: &WorkerProgress, omegas: &[f64]) -> String {
    match w.state {
        WorkerState::Idle => return "Idle".into(),
        WorkerState::Done => return "Done".into(),
        WorkerState::Running => {}
    }
    let verb = |stage: Option<Stage>| match stage {
        Some(Stage::Assembly) => "Assembling",
        Some(Stage::Lu) => "Factorizing",
        Some(Stage::Outputs) => "Computing the outputs of",
        _ => "Solving",
    };
    match w.task {
        // the work all threads share: the statics
        None => "Preparing the influence data (all workers)".into(),
        Some(WorkerTask::Limit(k)) => {
            let infinite = p.limits.get(k).is_some_and(|l| l.infinite);
            format!("{} limit ω = {}", verb(w.stage), if infinite { "∞" } else { "0" })
        }
        Some(WorkerTask::Frequency(k)) => {
            let n = p.frequencies.len();
            match omegas.get(k).filter(|_| omegas.len() == n) {
                Some(om) => format!("{} frequency {} of {n}, ω = {}", verb(w.stage), k + 1, omega_text(*om)),
                None => format!("{} frequency {} of {n}", verb(w.stage), k + 1),
            }
        }
        Some(WorkerTask::SecondOrder { step, item: None }) => format!("{} (all workers)", step_label(step)),
        Some(WorkerTask::SecondOrder { step, item: Some(i) }) => {
            let n = p.second_order.iter().rev().find(|s| s.step == step).map_or(0, |s| s.items.len());
            match item_noun(step) {
                Some(noun) if n > 1 => format!("{}: {noun} {} of {n}", step_label(step), i + 1),
                _ => step_label(step).into(),
            }
        }
    }
}

/// The status bar's text and share: the preparation, the frequencies (limits counted), the step of
/// the second order that runs, the writing of the results.
pub fn phase_status(p: &SolveProgress) -> (f64, String) {
    let frequencies = || (p.done as f64 / p.total.max(1) as f64, format!("Frequency {} / {}", p.done, p.total));
    match p.phase {
        None | Some(Phase::Preparation) => (p.preparation, "Preparing".into()),
        Some(Phase::SecondOrder) => match running_step(p) {
            Some(s) => (s.fraction, format!("Second order: {}", step_label(s.step))),
            None => (0.0, "Second order".into()),
        },
        Some(Phase::Finishing) => (1.0, "Writing the results".into()),
        Some(_) => frequencies(),
    }
}

/// The step of the second order that runs (the last begun), else the last one.
pub fn running_step(p: &SolveProgress) -> Option<&SecondOrderProgress> {
    p.second_order.iter().rev().find(|s| s.state == RunState::Running).or(p.second_order.last())
}

/// The name of a second-order step in the GUI.
pub fn step_label(step: SecondOrderStep) -> &'static str {
    match step {
        SecondOrderStep::QuadraticPressureIntegration => "Quadratic QTF, pressure integration",
        SecondOrderStep::QuadraticControlSurface => "Quadratic QTF, control surface",
        SecondOrderStep::PotentialFirstOrder => "Potential QTF, first-order sub-problem",
        SecondOrderStep::PotentialRadiation => "Potential QTF, radiation at the pair frequencies",
        SecondOrderStep::PotentialFits => "Potential QTF, far-field fits",
        SecondOrderStep::PotentialOuter => "Potential QTF, outer integral",
        SecondOrderStep::PotentialPairs => "Potential QTF, inner free surface and body",
        SecondOrderStep::PotentialDirect => "Potential QTF, direct method",
    }
}

/// What the items of a step are (None: a step of one item).
fn item_noun(step: SecondOrderStep) -> Option<&'static str> {
    match step {
        SecondOrderStep::QuadraticPressureIntegration | SecondOrderStep::QuadraticControlSurface => Some("row ω_a"),
        SecondOrderStep::PotentialFirstOrder | SecondOrderStep::PotentialRadiation => Some("frequency"),
        SecondOrderStep::PotentialPairs | SecondOrderStep::PotentialDirect => Some("pair frequency"),
        SecondOrderStep::PotentialOuter => Some("batch of radii"),
        SecondOrderStep::PotentialFits => None,
    }
}

/// ω in rad/s with four significant digits.
fn omega_text(w: f64) -> String {
    let digits = if w > 0.0 { (3 - w.log10().floor() as i32).clamp(0, 6) } else { 3 };
    format!("{w:.*} rad/s", digits as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydrobem_engine::{FrequencyProgress, LimitProgress};

    fn freq(state: RunState, fraction: f64) -> FrequencyProgress {
        FrequencyProgress { state, fraction, ..FrequencyProgress::default() }
    }

    fn worker(task: Option<WorkerTask>, stage: Option<Stage>, fraction: f64) -> WorkerProgress {
        WorkerProgress { state: WorkerState::Running, task, stage, fraction }
    }

    fn progress() -> SolveProgress {
        SolveProgress {
            running: true,
            preparation: 1.0,
            done: 1,
            total: 3,
            phase: Some(Phase::FirstOrder),
            limits: vec![LimitProgress { infinite: true, progress: freq(RunState::Running, 0.2) }],
            frequencies: vec![freq(RunState::Running, 0.4), freq(RunState::Done, 1.0)],
            ..SolveProgress::default()
        }
    }

    #[test]
    fn the_rows_are_the_engines_workers_or_idle_ones_before_its_snapshot() {
        let mut p = SolveProgress::default();
        assert_eq!(worker_rows(&p, 3), vec![WorkerProgress::default(); 3]);
        assert_eq!(worker_status(&p, &worker_rows(&p, 3)[0], &[]), "Idle");
        p.workers = vec![worker(Some(WorkerTask::Frequency(0)), Some(Stage::Lu), 0.4)];
        assert_eq!(worker_rows(&p, 3), p.workers);
        // the bars: the item's share while running, empty when idle, full once done
        let mut w = p.workers[0];
        assert_eq!(worker_fraction(&w), 0.4);
        w.state = WorkerState::Idle;
        assert_eq!(worker_fraction(&w), 0.0);
        w.state = WorkerState::Done;
        assert_eq!(worker_fraction(&w), 1.0);
    }

    #[test]
    fn the_status_says_the_stage_and_the_task() {
        let p = progress();
        let omegas = [0.4, 1.25];
        let status = |w: WorkerProgress| worker_status(&p, &w, &omegas);
        assert_eq!(
            status(worker(Some(WorkerTask::Frequency(0)), Some(Stage::Lu), 0.4)),
            "Factorizing frequency 1 of 2, ω = 0.4000 rad/s"
        );
        assert_eq!(status(worker(Some(WorkerTask::Limit(0)), Some(Stage::Assembly), 0.2)), "Assembling limit ω = ∞");
        assert_eq!(status(worker(None, None, 0.0)), "Preparing the influence data (all workers)");
        // ω left out when the frequencies do not match the engine's
        let w = worker(Some(WorkerTask::Frequency(1)), None, 0.0);
        assert_eq!(worker_status(&p, &w, &[0.4]), "Solving frequency 2 of 2");
        let idle = WorkerProgress { state: WorkerState::Idle, ..WorkerProgress::default() };
        assert_eq!(status(idle), "Idle");
    }

    #[test]
    fn the_second_order_is_shown_by_step_and_item() {
        let mut p = progress();
        p.phase = Some(Phase::SecondOrder);
        p.second_order = vec![
            SecondOrderProgress {
                step: SecondOrderStep::QuadraticPressureIntegration,
                state: RunState::Done,
                fraction: 1.0,
                items: vec![freq(RunState::Done, 1.0); 4],
            },
            SecondOrderProgress {
                step: SecondOrderStep::PotentialOuter,
                state: RunState::Running,
                fraction: 0.25,
                items: vec![freq(RunState::Running, 0.5); 8],
            },
        ];
        let step = |step, item| worker(Some(WorkerTask::SecondOrder { step, item }), None, 0.5);
        assert_eq!(
            worker_status(&p, &step(SecondOrderStep::PotentialOuter, Some(2)), &[]),
            "Potential QTF, outer integral: batch of radii 3 of 8"
        );
        assert_eq!(
            worker_status(&p, &step(SecondOrderStep::PotentialFits, None), &[]),
            "Potential QTF, far-field fits (all workers)"
        );
        assert_eq!(phase_status(&p), (0.25, "Second order: Potential QTF, outer integral".to_string()));
        p.phase = Some(Phase::Finishing);
        assert_eq!(phase_status(&p).1, "Writing the results");
        p.phase = Some(Phase::FirstOrder);
        assert_eq!(phase_status(&p).1, "Frequency 1 / 3");
    }
}
