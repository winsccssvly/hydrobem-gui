//! The layouts of the field-point grid: Cartesian (x, y), Polar (radius and angle about a centre)
//! and Body-Fitted (rings at a distance outside a body's section, by angle about its centre). Each
//! is a lattice of (i, j) nodes whose plan position is continuous in fractional indices, so the
//! heatmap, its interpolation, the animated surface and the markers treat the three alike.

use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::project::{Body, FieldPoints, V3};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridLayout {
    #[default]
    Cartesian,
    Polar,
    BodyFitted,
}

impl GridLayout {
    pub const ALL: [GridLayout; 3] = [GridLayout::Cartesian, GridLayout::Polar, GridLayout::BodyFitted];

    pub fn label(self) -> &'static str {
        match self {
            GridLayout::Cartesian => "Grid",
            GridLayout::Polar => "Polar Grid",
            GridLayout::BodyFitted => "Body-Fitted Grid",
        }
    }
}

/// The samples of a profile: one every half degree.
pub const PROFILE_ANGLES: usize = 720;

/// A body's section at the grid's depth seen from its centre: the distance to its outer boundary
/// at `PROFILE_ANGLES` evenly spaced angles from +x, anticlockwise. A section that is not
/// star-shaped about its centre keeps its outermost crossing on each ray.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub centre: [f64; 2],
    pub radii: Vec<f64>,
}

impl Profile {
    /// The section of `quads` (global coordinates) at height `z`; None when they do not cross it.
    /// It is cut a millionth of the body's size below z: a hull that ends at the free surface
    /// (its waterline on z = 0) has its waterline as the section at z = 0. With `convex` the
    /// section's convex hull instead: one outline about several columns (a semi-submersible).
    pub fn of_section(quads: &[[V3; 4]], z: f64, convex: bool) -> Option<Profile> {
        let (lo, hi) = quads.iter().flatten().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
            (a.min(v[0]).min(v[1]).min(v[2]), b.max(v[0]).max(v[1]).max(v[2]))
        });
        let z = z - 1e-6 * (hi - lo).max(1.0);
        let mut segments: Vec<([f64; 2], [f64; 2])> = Vec::new();
        for q in quads {
            let mut cut = Vec::new();
            for k in 0..4 {
                let (a, b) = (q[k], q[(k + 1) % 4]);
                let (da, db) = (a[2] - z, b[2] - z);
                if (da <= 0.0) != (db <= 0.0) {
                    let t = da / (da - db);
                    cut.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
                }
            }
            if cut.len() >= 2 {
                segments.push((cut[0], cut[1]));
            }
        }
        if convex {
            let hull = convex_hull(segments.iter().flat_map(|(a, b)| [*a, *b]).collect());
            segments = (0..hull.len()).map(|k| (hull[k], hull[(k + 1) % hull.len()])).collect();
        }
        // the centre: the mean of the segments' midpoints weighted by their lengths (of a convex
        // outline: the centroid of its area)
        let (mut cx, mut cy, mut weight) = (0.0, 0.0, 0.0);
        for (a, b) in &segments {
            if convex {
                let cross = a[0] * b[1] - b[0] * a[1];
                cx += (a[0] + b[0]) * cross / 3.0;
                cy += (a[1] + b[1]) * cross / 3.0;
                weight += cross;
            } else {
                let l = (b[0] - a[0]).hypot(b[1] - a[1]);
                cx += l * (a[0] + b[0]) / 2.0;
                cy += l * (a[1] + b[1]) / 2.0;
                weight += l;
            }
        }
        if weight <= 0.0 {
            return None;
        }
        // to the nanometre: short decimals that the GUI state's JSON reads back exactly
        let nano = |v: f64| (v * 1e9).round() / 1e9;
        let centre = [nano(cx / weight), nano(cy / weight)];
        let radii: Vec<f64> = (0..PROFILE_ANGLES)
            .map(|k| {
                let angle = TAU * k as f64 / PROFILE_ANGLES as f64;
                let d = [angle.cos(), angle.sin()];
                nano(segments.iter().filter_map(|(a, b)| ray_hit(centre, d, *a, *b)).fold(0.0, f64::max))
            })
            .collect();
        radii.iter().any(|r| *r > 0.0).then_some(Profile { centre, radii })
    }

    /// The distance at an angle (rad), linear between the samples.
    pub fn radius(&self, angle: f64) -> f64 {
        let n = self.radii.len();
        if n == 0 {
            return 0.0;
        }
        let u = angle.rem_euclid(TAU) / TAU * n as f64;
        let k = u.floor() as usize % n;
        let f = u - u.floor();
        self.radii[k] * (1.0 - f) + self.radii[(k + 1) % n] * f
    }

    pub fn max_radius(&self) -> f64 {
        self.radii.iter().copied().fold(0.0, f64::max)
    }

    /// The area inside the outline, m².
    pub fn area(&self) -> f64 {
        let step = TAU / self.radii.len().max(1) as f64;
        0.5 * self.radii.iter().map(|r| r * r).sum::<f64>() * step
    }

    /// Whether a body's section at `z` is in separate parts (several columns) or deeply notched:
    /// followed as a star it covers under 70 % of its convex hull. Its rings are then better
    /// about the convex hull.
    pub fn is_split(body: &Body, z: f64) -> bool {
        match (Profile::of_body(body, z, false), Profile::of_body(body, z, true)) {
            (Some(star), Some(hull)) => star.area() < 0.7 * hull.area(),
            _ => false,
        }
    }

    /// The section (or its convex hull) of a body's whole mesh, placed, at height `z`.
    pub fn of_body(body: &Body, z: f64, convex: bool) -> Option<Profile> {
        let mesh = body.mesh.as_ref()?;
        let quads: Vec<[V3; 4]> = mesh.expanded().into_iter().map(|q| q.map(|v| body.to_global(v))).collect();
        Profile::of_section(&quads, z, convex)
    }
}

/// The convex hull of points, anticlockwise (Andrew's monotone chain); collinear points left out.
fn convex_hull(mut points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let turn = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut hull: Vec<[f64; 2]> = Vec::new();
    for pass in [points.clone(), points.into_iter().rev().collect()] {
        let start = hull.len();
        for p in pass {
            while hull.len() >= start + 2 && turn(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        // the last point of each chain starts the other
        hull.pop();
    }
    hull
}

/// The distance along the ray o + t d (t >= 0) to the segment a-b, if it meets it.
fn ray_hit(o: [f64; 2], d: [f64; 2], a: [f64; 2], b: [f64; 2]) -> Option<f64> {
    let e = [b[0] - a[0], b[1] - a[1]];
    let den = d[0] * e[1] - d[1] * e[0];
    if den.abs() < 1e-14 {
        return None;
    }
    let w = [a[0] - o[0], a[1] - o[1]];
    let t = (w[0] * e[1] - w[1] * e[0]) / den;
    let s = (w[0] * d[1] - w[1] * d[0]) / den;
    (t >= 0.0 && (-1e-9..=1.0 + 1e-9).contains(&s)).then_some(t)
}

/// The step between `count` values from start to end; 1 when there is a single value (the cell
/// size of a one-point axis).
fn step_of(start: f64, end: f64, count: i64) -> f64 {
    if count > 1 { (end - start) / (count - 1) as f64 } else { 1.0 }
}

impl FieldPoints {
    /// The node counts along i and j: Cartesian x and y; otherwise radial and angular.
    pub fn dims(&self) -> (i64, i64) {
        match self.layout {
            GridLayout::Cartesian => (self.x_count, self.y_count),
            GridLayout::Polar => (self.radial_count, self.angle_count),
            GridLayout::BodyFitted => (self.radial_count + self.inside_rings.max(0), self.angle_count),
        }
    }

    /// Body-Fitted: the distance from the centre of ring k where the outline lies at `outline`.
    /// The inside rings divide the outline's radius evenly (k = -1 the centre); the others lie
    /// from First Ring Offset outside it, evenly to Last Ring Offset.
    fn fitted_ring(&self, k: i64, outline: f64) -> f64 {
        let n = self.inside_rings.max(0);
        if k < n {
            outline * (k + 1) as f64 / (n + 1) as f64
        } else {
            outline
                + self.radial_start
                + (k - n) as f64 * step_of(self.radial_start, self.radial_end, self.radial_count)
        }
    }

    /// The distance of a fractional ring index: linear between the rings.
    fn fitted_radius(&self, i: f64, outline: f64) -> f64 {
        let k = i.floor();
        let t = i - k;
        let k = k as i64;
        self.fitted_ring(k, outline) * (1.0 - t) + self.fitted_ring(k + 1, outline) * t
    }

    /// Whether the angles make a whole turn: node j = count is node 0 again.
    pub fn periodic(&self) -> bool {
        self.layout != GridLayout::Cartesian && (self.angle_end - self.angle_start).abs() >= 360.0 - 1e-9
    }

    /// The angle between neighbouring nodes, deg: a whole turn has no node at its end.
    fn angle_step(&self) -> f64 {
        let span = self.angle_end - self.angle_start;
        if self.periodic() {
            span.signum() * 360.0 / self.angle_count.max(1) as f64
        } else {
            step_of(self.angle_start, self.angle_end, self.angle_count)
        }
    }

    /// The plan position of node (i, j). Fractional indices interpolate continuously and indices
    /// beyond the counts extend the layout; None for a body-fitted grid without its body's section.
    pub fn plan_at(&self, i: f64, j: f64) -> Option<[f64; 2]> {
        let radius = self.radial_start + i * step_of(self.radial_start, self.radial_end, self.radial_count);
        let angle = (self.angle_start + j * self.angle_step()).to_radians();
        match self.layout {
            GridLayout::Cartesian => Some([
                self.x_start + i * step_of(self.x_start, self.x_end, self.x_count),
                self.y_start + j * step_of(self.y_start, self.y_end, self.y_count),
            ]),
            GridLayout::Polar => Some([self.centre[0] + radius * angle.cos(), self.centre[1] + radius * angle.sin()]),
            GridLayout::BodyFitted => {
                let p = self.profile.as_ref()?;
                let r = self.fitted_radius(i, p.radius(angle));
                Some([p.centre[0] + r * angle.cos(), p.centre[1] + r * angle.sin()])
            }
        }
    }

    /// The node nearest to a point (its plan position), as the grid generated it.
    pub fn index_of(&self, p: V3) -> Option<(i64, i64)> {
        let index = |value: f64, start: f64, step: f64| ((value - start) / step).round() as i64;
        let nonzero = |s: f64| if s.abs() < 1e-12 { 1.0 } else { s };
        let dr = nonzero(step_of(self.radial_start, self.radial_end, self.radial_count));
        let angular = |dx: f64, dy: f64| {
            let turn = dy.atan2(dx).to_degrees() - self.angle_start;
            let step = nonzero(self.angle_step());
            // measured from the first angle in the direction of the steps
            let from_start = if step > 0.0 { turn.rem_euclid(360.0) } else { -(-turn).rem_euclid(360.0) };
            let j = index(from_start, 0.0, step);
            if self.periodic() { j.rem_euclid(self.angle_count.max(1)) } else { j }
        };
        match self.layout {
            GridLayout::Cartesian => Some((
                index(p[0], self.x_start, nonzero(step_of(self.x_start, self.x_end, self.x_count))),
                index(p[1], self.y_start, nonzero(step_of(self.y_start, self.y_end, self.y_count))),
            )),
            GridLayout::Polar => {
                let (dx, dy) = (p[0] - self.centre[0], p[1] - self.centre[1]);
                Some((index(dx.hypot(dy), self.radial_start, dr), angular(dx, dy)))
            }
            GridLayout::BodyFitted => {
                let profile = self.profile.as_ref()?;
                let (dx, dy) = (p[0] - profile.centre[0], p[1] - profile.centre[1]);
                let (r, outline) = (dx.hypot(dy), profile.radius(dy.atan2(dx)));
                // the nearest ring (the inside rings are not evenly spaced with the others)
                let ring = (-1..=self.dims().0)
                    .min_by(|a, b| {
                        (self.fitted_ring(*a, outline) - r).abs().total_cmp(&(self.fitted_ring(*b, outline) - r).abs())
                    })
                    .unwrap_or(0);
                Some((ring, angular(dx, dy)))
            }
        }
    }

    /// The node (i, j) names on the lattice: j taken round a whole turn.
    pub fn wrap(&self, i: i64, j: i64) -> (i64, i64) {
        if self.periodic() { (i, j.rem_euclid(self.angle_count.max(1))) } else { (i, j) }
    }

    /// Whether (i, j) is a node of the lattice (after wrapping).
    pub fn has_node(&self, i: i64, j: i64) -> bool {
        let (ni, nj) = self.dims();
        let (i, j) = self.wrap(i, j);
        (0..ni).contains(&i) && (0..nj).contains(&j)
    }

    /// The plan nodes of one level in generation order: j outer, i inner (Cartesian: rows of x).
    pub fn plan_nodes(&self) -> Vec<[f64; 2]> {
        let (ni, nj) = self.dims();
        if ni <= 0 || nj <= 0 {
            return Vec::new();
        }
        (0..nj).flat_map(|j| (0..ni).filter_map(move |i| self.plan_at(i as f64, j as f64))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(half: f64) -> Vec<[V3; 4]> {
        // the four sides of a box from z = -1 to 1
        let c = [[-half, -half], [half, -half], [half, half], [-half, half]];
        (0..4)
            .map(|k| {
                let (a, b) = (c[k], c[(k + 1) % 4]);
                [[a[0], a[1], -1.0], [b[0], b[1], -1.0], [b[0], b[1], 1.0], [a[0], a[1], 1.0]]
            })
            .collect()
    }

    #[test]
    fn a_box_section_seen_from_its_centre() {
        let p = Profile::of_section(&square(2.0), 0.0, false).expect("the box crosses z = 0");
        assert!(p.centre[0].abs() < 1e-12 && p.centre[1].abs() < 1e-12);
        assert!((p.radius(0.0) - 2.0).abs() < 1e-9);
        assert!((p.radius(std::f64::consts::FRAC_PI_4) - 2.0 * 2f64.sqrt()).abs() < 1e-9);
        assert!(Profile::of_section(&square(2.0), 5.0, false).is_none());
        // a convex box is its own hull, with the same centre and radii
        assert_eq!(Profile::of_section(&square(2.0), 0.0, true), Some(p.clone()));
        assert!((p.area() - 16.0).abs() < 0.01, "{}", p.area());
    }

    #[test]
    fn three_columns_take_one_convex_outline() {
        // three boxes 1 m wide, 10 m from the origin at 90, 210 and 330 deg
        let quads: Vec<[V3; 4]> = [90.0f64, 210.0, 330.0]
            .iter()
            .flat_map(|a| {
                let (s, c) = a.to_radians().sin_cos();
                square(0.5).into_iter().map(move |q| q.map(|v| [v[0] + 10.0 * c, v[1] + 10.0 * s, v[2]]))
            })
            .collect();
        let section = Profile::of_section(&quads, 0.0, false).unwrap();
        let hull = Profile::of_section(&quads, 0.0, true).unwrap();
        // between two columns (30 deg) the section is not met, the hull's side is
        assert_eq!(section.radius(30f64.to_radians()), 0.0);
        let side = hull.radius(30f64.to_radians());
        assert!(side > 5.0 && side < 6.0, "{side}");
        // through a column both reach its far face, y = 10.5 (the axis-aligned boxes lift the
        // hull's centroid a little)
        assert!((hull.centre[1] + hull.radius(90f64.to_radians()) - 10.5).abs() < 1e-6);
        assert!((section.radius(90f64.to_radians()) - 10.5).abs() < 1e-6);
        assert!(hull.centre[0].abs() < 1e-6 && hull.centre[1].abs() < 0.2, "{:?}", hull.centre);
        // the star covers a small part of the hull: a section in parts (Convex Outline by default)
        assert!(section.area() < 0.7 * hull.area(), "{} {}", section.area(), hull.area());
    }

    #[test]
    fn polar_and_fitted_nodes_find_their_indices() {
        let mut g = FieldPoints {
            enabled: true,
            layout: GridLayout::Polar,
            centre: [1.0, -2.0],
            radial_start: 3.0,
            radial_end: 6.0,
            radial_count: 4,
            angle_start: 0.0,
            angle_end: 360.0,
            angle_count: 8,
            ..FieldPoints::default()
        };
        assert!(g.periodic());
        let check = |g: &FieldPoints| {
            let nodes = g.plan_nodes();
            assert_eq!(nodes.len(), 32);
            for (k, n) in nodes.iter().enumerate() {
                assert_eq!(g.index_of([n[0], n[1], 0.0]), Some(((k % 4) as i64, (k / 4) as i64)), "{n:?}");
            }
        };
        check(&g);
        // node 8 is node 0 again; the first ring lies 3 m from the centre
        assert_eq!(g.wrap(0, 8), (0, 0));
        let a = g.plan_at(0.0, 0.0).unwrap();
        assert!((a[0] - 4.0).abs() < 1e-12 && (a[1] + 2.0).abs() < 1e-12);
        // half a turn: no wrap, the end included
        g.angle_end = 180.0;
        g.angle_count = 5;
        assert!(!g.periodic());
        assert_eq!(g.index_of([1.0, 4.0, 0.0]), Some((3, 2)));

        let mut f = FieldPoints {
            layout: GridLayout::BodyFitted,
            radial_start: 0.5,
            radial_end: 2.0,
            radial_count: 4,
            angle_count: 8,
            profile: Profile::of_section(&square(2.0), 0.0, false),
            ..g.clone()
        };
        f.angle_end = 360.0;
        check(&f);
        // the first ring 0.5 m outside the box side along +x, 0.5 m beyond its corner
        let side = f.plan_at(0.0, 0.0).unwrap();
        assert!((side[0] - 2.5).abs() < 1e-9 && side[1].abs() < 1e-9, "{side:?}");
        let corner = f.plan_at(0.0, 1.0).unwrap();
        assert!((corner[0] - (2.0 + 0.5 / 2f64.sqrt())).abs() < 1e-6, "{corner:?}");
    }

    #[test]
    fn inside_rings_divide_the_outline_and_find_their_indices() {
        let f = FieldPoints {
            enabled: true,
            layout: GridLayout::BodyFitted,
            radial_start: 0.5,
            radial_end: 2.0,
            radial_count: 4,
            angle_count: 8,
            inside_rings: 3,
            profile: Profile::of_section(&square(2.0), 0.0, false),
            ..FieldPoints::default()
        };
        assert_eq!(f.dims(), (7, 8));
        // along +x the outline is 2 m out: rings at 0.5, 1, 1.5, then 2.5, 3, 3.5, 4
        let xs: Vec<f64> = (0..7).map(|i| f.plan_at(i as f64, 0.0).unwrap()[0]).collect();
        for (x, want) in xs.iter().zip([0.5, 1.0, 1.5, 2.5, 3.0, 3.5, 4.0]) {
            assert!((x - want).abs() < 1e-9, "{xs:?}");
        }
        // between the last inside ring and the first outer one, linearly
        assert!((f.plan_at(2.5, 0.0).unwrap()[0] - 2.0).abs() < 1e-9);
        for (k, n) in f.plan_nodes().iter().enumerate() {
            assert_eq!(f.index_of([n[0], n[1], 0.0]), Some(((k % 7) as i64, (k / 7) as i64)), "{n:?}");
        }
    }
}
