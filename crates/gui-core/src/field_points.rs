//! The field points a solve submits: the grid points the engine accepts by its field-point rules
//! (`Model::classify_points`: inside a body, on its waterline or in its water plane, above the
//! free surface, on the sea bed, behind a wall or outside the channel, on a lid). The GUI has no
//! rule of its own. The points in the sea are given in global coordinates; those inside a tank
//! to the tank, in its body's axes (the engine's rule).

use hydrobem_engine::{Model, PointStatus};

use crate::engine_model::{BuildOptions, build_model};
use crate::project::{Project, V3};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FieldPointSelection {
    /// The submitted points, in grid order (global coordinates).
    pub points: Vec<V3>,
    pub generated: usize,
    /// Inside a body, on its waterline or inside its water plane.
    pub inside_bodies: usize,
    /// By the other rules.
    pub rejected: usize,
    /// Submitted, but closer to a low-order panel than its size (a warning).
    pub near_bodies: usize,
    /// The engine could not check the points (invalid geometry); all are kept.
    pub engine_error: Option<String>,
    /// The tank of each submitted point as the engine names it ("<body>/<tank>", "" the sea);
    /// empty when every point is in the sea.
    pub tanks: Vec<String>,
    /// Per tank of the submitted points, its place in the model: the tank path, the zero-based
    /// solved body and the tank's zero-based place among the body's tanks (the owner of its
    /// results).
    pub tank_owners: Vec<(String, usize, usize)>,
}

/// Where the results of a submitted field point are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PointSource {
    /// The k-th point in the sea.
    Sea(usize),
    /// The k-th point of a tank: its solved body and its place among the body's tanks.
    Tank { body: usize, tank: usize, index: usize },
}

impl FieldPointSelection {
    /// The tank of submitted point k ("" the sea).
    fn tank_of(&self, k: usize) -> &str {
        self.tanks.get(k).map_or("", String::as_str)
    }

    /// The submitted points in the sea, in order (global coordinates).
    pub fn sea_points(&self) -> Vec<V3> {
        (0..self.points.len()).filter(|&k| self.tank_of(k).is_empty()).map(|k| self.points[k]).collect()
    }

    /// The submitted points of each tank, in the order of their first point, in the axes of the
    /// tank's body.
    pub fn tank_points(&self, project: &Project) -> Vec<(String, Vec<V3>)> {
        let mut out: Vec<(String, Vec<V3>)> = Vec::new();
        for (k, p) in self.points.iter().enumerate() {
            let tank = self.tank_of(k);
            if tank.is_empty() {
                continue;
            }
            let local = body_of(project, tank).map_or(*p, |b| b.to_local(*p));
            match out.iter_mut().find(|(t, _)| t == tank) {
                Some((_, list)) => list.push(local),
                None => out.push((tank.to_string(), vec![local])),
            }
        }
        out
    }

    /// Where the results of each submitted point are, in grid order (None: a tank the model does
    /// not solve).
    pub fn sources(&self) -> Vec<Option<PointSource>> {
        let mut sea = 0;
        let mut per_tank: Vec<(String, usize)> = Vec::new();
        (0..self.points.len())
            .map(|k| {
                let tank = self.tank_of(k);
                if tank.is_empty() {
                    sea += 1;
                    return Some(PointSource::Sea(sea - 1));
                }
                let index = match per_tank.iter_mut().find(|(t, _)| t == tank) {
                    Some((_, n)) => {
                        *n += 1;
                        *n - 1
                    }
                    None => {
                        per_tank.push((tank.to_string(), 1));
                        0
                    }
                };
                let (_, body, place) = self.tank_owners.iter().find(|(t, _, _)| t == tank)?;
                Some(PointSource::Tank { body: *body, tank: *place, index })
            })
            .collect()
    }
}

fn body_of<'a>(project: &'a Project, tank: &str) -> Option<&'a crate::project::Body> {
    let body = tank.split_once('/')?.0;
    project.bodies.iter().find(|b| b.name == body)
}

/// The project's grid judged by the rules of `m`, which holds its bodies and lids; `tanks` the
/// engine's name of the tank of each point (None: every point in the sea).
pub fn classify_grid(m: &mut Model, project: &Project, tanks: Option<&[String]>) -> FieldPointSelection {
    let points = project.field_points.generate();
    let mut selection = FieldPointSelection { generated: points.len(), ..FieldPointSelection::default() };
    if points.is_empty() {
        return selection;
    }
    let tanks = tanks.filter(|t| t.len() == points.len());
    let tank_of = |k: usize| tanks.map_or("", |t| t[k].as_str());
    // the verdict on each point: the sea's at once, each tank's in its body's axes
    let mut verdicts: Vec<Option<PointStatus>> = vec![None; points.len()];
    let mut groups: Vec<String> = vec![String::new()];
    for k in 0..points.len() {
        if !groups.iter().any(|g| g == tank_of(k)) {
            groups.push(tank_of(k).to_string());
        }
    }
    let solved = solved_bodies(m);
    for group in &groups {
        let members: Vec<usize> = (0..points.len()).filter(|&k| tank_of(k) == group).collect();
        if members.is_empty() {
            continue;
        }
        let classes = if group.is_empty() {
            let at: Vec<V3> = members.iter().map(|&k| points[k]).collect();
            m.classify_points(&at, None)
        } else {
            let Some((body, tank)) = group.split_once('/') else { continue };
            // a tank the model does not solve takes no point
            let Some(owner) = tank_owner(m, &solved, body, tank) else { continue };
            selection.tank_owners.push((group.clone(), owner.1, owner.2));
            let b = body_of(project, group);
            let at: Vec<V3> = members.iter().map(|&k| b.map_or(points[k], |b| b.to_local(points[k]))).collect();
            m.classify_points(&at, Some(owner.0))
        };
        match classes {
            Ok(classes) => {
                for (&k, class) in members.iter().zip(classes) {
                    verdicts[k] = Some(class.status);
                }
            }
            Err(e) => {
                selection.engine_error = Some(e.message().to_string());
                selection.points = points;
                selection.tanks = tanks.map(<[String]>::to_vec).unwrap_or_default();
                return selection;
            }
        }
    }
    for (k, (p, verdict)) in points.iter().zip(verdicts).enumerate() {
        let tank = tank_of(k);
        let keep = |s: &mut FieldPointSelection| {
            s.points.push(*p);
            if tanks.is_some() {
                s.tanks.push(tank.to_string());
            }
        };
        match verdict {
            Some(PointStatus::Fluid) => keep(&mut selection),
            // in the fluid of the tank the point is given to
            Some(PointStatus::Tank) if !tank.is_empty() => keep(&mut selection),
            Some(PointStatus::NearBody) => {
                keep(&mut selection);
                selection.near_bodies += 1;
            }
            Some(PointStatus::InsideBody | PointStatus::OnBody | PointStatus::InsideWaterplane) => {
                selection.inside_bodies += 1
            }
            _ => selection.rejected += 1,
        }
    }
    if selection.tanks.iter().all(String::is_empty) {
        selection.tanks.clear();
    }
    selection
}

/// The enabled bodies of the model, in order (the solved bodies of the results).
fn solved_bodies(m: &Model) -> Vec<hydrobem_engine::ObjectId> {
    let enabled = |id| m.get(id, "enabled").ok().flatten().and_then(|v| v.as_bool()).unwrap_or(true);
    m.children(m.root(), "bodies").unwrap_or_default().into_iter().filter(|&id| enabled(id)).collect()
}

/// The tank object, its solved body and its place among the body's tanks; None when the body is
/// not solved or the tank is off.
fn tank_owner(
    m: &Model,
    solved: &[hydrobem_engine::ObjectId],
    body: &str,
    tank: &str,
) -> Option<(hydrobem_engine::ObjectId, usize, usize)> {
    let b = m.object(&format!("bodies/{body}")).ok()?;
    let t = m.object(&format!("bodies/{body}/tanks/{tank}")).ok()?;
    if !m.get(t, "enabled").ok().flatten().and_then(|v| v.as_bool()).unwrap_or(true) {
        return None;
    }
    let solved_index = solved.iter().position(|&s| s == b)?;
    Some((t, solved_index, m.object_info(t).ok()?.index))
}

/// The field points of a model (a results file's): those in the sea, then those of each tank of
/// the solved bodies (global, by the project's bodies), with where their results are.
pub fn model_field_points(m: &Model, project: &Project) -> (Vec<V3>, Vec<Option<PointSource>>) {
    let reals = |id, item: &str| -> Vec<V3> {
        m.get(id, item)
            .ok()
            .flatten()
            .and_then(|v| v.as_reals().map(|r| r.as_chunks::<3>().0.to_vec()))
            .unwrap_or_default()
    };
    let mut points = Vec::new();
    let mut sources = Vec::new();
    if let Ok(fo) = m.object("first_order") {
        for (k, p) in reals(fo, "field_points").into_iter().enumerate() {
            points.push(p);
            sources.push(Some(PointSource::Sea(k)));
        }
    }
    for (body, b) in solved_bodies(m).into_iter().enumerate() {
        let name = m.object_info(b).map(|i| i.name).unwrap_or_default();
        let placed = project.bodies.iter().find(|x| x.name == name);
        for (tank, t) in m.children(b, "tanks").unwrap_or_default().into_iter().enumerate() {
            for (index, p) in reals(t, "field_points").into_iter().enumerate() {
                points.push(placed.map_or(p, |x| x.to_global(p)));
                sources.push(Some(PointSource::Tank { body, tank, index }));
            }
        }
    }
    (points, sources)
}

/// The project's grid judged against its meshed active bodies, lids, options and water.
pub fn select_field_points(project: &Project) -> FieldPointSelection {
    if !project.field_points.enabled {
        return FieldPointSelection::default();
    }
    let options =
        BuildOptions { field_points: false, gui_state: false, placeholder_frequency: false, meshed_bodies_only: true };
    match build_model(project, &options) {
        Ok(mut built) => classify_grid(&mut built.model, project, project.field_point_tanks().as_deref()),
        Err(e) => {
            let points = project.field_points.generate();
            FieldPointSelection {
                generated: points.len(),
                points,
                engine_error: Some(e.message().to_string()),
                ..FieldPointSelection::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Domain, FieldPoints};

    #[test]
    fn points_inside_the_cylinder_and_behind_a_wall_are_excluded() {
        let mut p = crate::engine_model::tests_support::cylinder_project();
        p.field_points = FieldPoints {
            enabled: true,
            x_start: -3.0,
            x_end: 3.0,
            x_count: 9,
            y_start: -3.0,
            y_end: 3.0,
            y_count: 9,
            z: 0.0,
            ..FieldPoints::default()
        };
        let s = select_field_points(&p);
        assert_eq!(s.generated, 81);
        assert!(s.engine_error.is_none(), "{:?}", s.engine_error);
        // the waterplane r < 1 holds the centre and the four points 0.75 m from it on the axes
        assert_eq!(s.inside_bodies, 5);
        assert_eq!(s.points.len() + s.inside_bodies + s.rejected, 81);
        assert!(s.points.iter().all(|q| q[0].hypot(q[1]) > 1.0));
        assert!(s.sources().iter().enumerate().all(|(k, src)| *src == Some(PointSource::Sea(k))));

        p.environment.domain = Domain::YWall;
        let walled = select_field_points(&p);
        assert!(walled.rejected > 0);
        assert!(walled.points.iter().all(|q| q[1] >= 0.0));
    }
}
