//! Values drawn on the scene: body and field overlays, animated values, the heatmap, the
//! field-point plane and markers.

use super::*;

/// The field at the phase ωt on its grid: neighbouring points joined into a surface, coloured by
/// the instantaneous value (blue below, red above zero) and, for elevations, raised by it times the
/// display scale `amplitude`. The colours and their legend are the values themselves (not scaled).
pub(super) fn add_animated_field(
    b: &mut Builder,
    field: &crate::animation::ComplexField,
    phase: f64,
    amplitude: f64,
) -> ColorScale {
    let values: Vec<f64> = field.values.iter().map(|v| crate::animation::real_at(*v, phase)).collect();
    let peak = field.values.iter().map(|v| v.norm()).fold(0.0, f64::max);
    let index: HashMap<(i64, i64), usize> =
        field.points.iter().enumerate().map(|(i, p)| (grid_index(*p, &field.grid), i)).collect();
    let vertex = |i: usize| -> (V3, Rgb) {
        let p = field.points[i];
        let v = values[i];
        let z = if field.elevation { p[2] + v * amplitude } else { p[2] };
        let t = if peak > 0.0 { 0.5 + 0.5 * v / peak } else { 0.5 };
        ([p[0], p[1], z], heat_color(t.clamp(0.0, 1.0)))
    };
    for p in &field.points {
        b.include(*p);
    }
    let g = &field.grid;
    for (&(gx, gy), &a) in &index {
        let corners = [
            Some(a),
            index.get(&g.wrap(gx + 1, gy)).copied(),
            index.get(&g.wrap(gx + 1, gy + 1)).copied(),
            index.get(&g.wrap(gx, gy + 1)).copied(),
        ];
        let [Some(a), Some(c1), Some(c2), Some(c3)] = corners else { continue };
        let (p, q, r, t) = (vertex(a), vertex(c1), vertex(c2), vertex(c3));
        b.shaded_triangle([p.0, q.0, r.0], [p.1, q.1, r.1], 0.9);
        b.shaded_triangle([p.0, r.0, t.0], [p.1, r.1, t.1], 0.9);
    }
    ColorScale { min: -peak, max: peak, label: field.label.clone(), smoothed: false, levels: 64, note: SOLVER_VALUES }
}

/// Whether values from lo to hi differ by more than rounding: a constant field (the incident
/// wave's velocity amplitude) is one colour, not its float noise stretched over the palette.
fn spread(lo: f64, hi: f64) -> bool {
    hi - lo > 1e-6 * hi.abs().max(lo.abs()) && hi > lo
}

/// The median distance from each point to its nearest neighbour (up to 4000 points).
fn spacing(points: &[V3]) -> Option<f64> {
    if points.len() < 2 || points.len() > 4000 {
        return None;
    }
    let mut nearest: Vec<f64> = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            points
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, q)| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt())
                .fold(f64::INFINITY, f64::min)
        })
        .filter(|d| *d > 0.0 && d.is_finite())
        .collect();
    if nearest.is_empty() {
        return None;
    }
    nearest.sort_by(f64::total_cmp);
    Some(nearest[nearest.len() / 2])
}

/// Values at the body output points: a square of each point's area on the hull, just off it into
/// the fluid, in the jet palette.
pub(super) fn add_body_overlay(b: &mut Builder, overlay: &BodyOverlay) -> ColorScale {
    if overlay.plane {
        return add_plane_overlay(b, overlay);
    }
    let (lo, hi) = overlay.values.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, c), v| (a.min(*v), c.max(*v)));
    let levels = 16;
    // points without an area (specified body points, lid and surface points): squares of the
    // median spacing to the nearest point, else a fixed share of the scene
    let fallback = spacing(&overlay.points).map_or(b.extent().max(1.0) * 0.03, |d| d / 2.0);
    let lift = b.extent().max(1.0) * 0.002;
    for (k, p) in overlay.points.iter().enumerate() {
        let (Some(n), Some(v)) = (overlay.normals.get(k), overlay.values.get(k)) else { continue };
        let area = overlay.areas.get(k).copied().unwrap_or(0.0);
        let h = if area > 0.0 { area.sqrt() / 2.0 } else { fallback };
        // two tangents of the normal
        let t0 = if n[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
        let u = cross(*n, t0);
        let ul = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt().max(1e-12);
        let u = u.map(|x| x / ul * h);
        let w = cross(*n, u);
        let c = [p[0] + n[0] * lift, p[1] + n[1] * lift, p[2] + n[2] * lift];
        let corner =
            |a: f64, d: f64| [c[0] + a * u[0] + d * w[0], c[1] + a * u[1] + d * w[1], c[2] + a * u[2] + d * w[2]];
        let t = if spread(lo, hi) { (v - lo) / (hi - lo) } else { 0.5 };
        let bin = ((t * (levels - 1) as f64) as i64).clamp(0, levels as i64 - 1);
        let color = heat_color(bin as f64 / (levels - 1) as f64);
        match overlay.faces.get(k).filter(|f| !f.is_empty()) {
            // the faces themselves, lifted off the hull along the normal
            Some(faces) => {
                for q in faces {
                    b.quad(q.map(|x| [x[0] + n[0] * lift, x[1] + n[1] * lift, x[2] + n[2] * lift]), color, 1.0);
                }
            }
            None => b.quad([corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)], color, 1.0),
        }
    }
    ColorScale { min: lo, max: hi, label: overlay.label.clone(), smoothed: false, levels, note: SOLVER_VALUES }
}

/// Values at points of horizontal surfaces (lids, pressure surfaces): per height, a Delaunay
/// triangulation of the points in plan, each triangle coloured between the values at its corners
/// (each point shows its own value). A triangle is drawn only when its centre lies on the surfaces
/// (`cover`): none bridges a hole or the gap between two surfaces; nothing is drawn beyond the
/// outermost points. Without the surfaces, a triangle with an edge longer than 4 times the spacing
/// of its points is left out instead. A height with fewer than three points shows squares.
/// Display only: it adds no solver values.
fn add_plane_overlay(b: &mut Builder, overlay: &BodyOverlay) -> ColorScale {
    let (lo, hi) = overlay.values.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, c), v| (a.min(*v), c.max(*v)));
    let color = |v: f64| heat_color(if spread(lo, hi) { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.5 });
    let lift = b.extent().max(1.0) * 0.002;
    let cover = (!overlay.cover.is_empty()).then(|| PlanCover::new(&overlay.cover));
    // the points by height (micrometres)
    let mut levels: std::collections::BTreeMap<i64, Vec<usize>> = Default::default();
    for (k, p) in overlay.points.iter().enumerate() {
        if overlay.values.get(k).is_some() {
            levels.entry((p[2] * 1e6).round() as i64).or_default().push(k);
        }
    }
    for indices in levels.values() {
        let at = |i: usize| overlay.points[indices[i]];
        let value = |i: usize| overlay.values[indices[i]];
        let up = |p: V3| [p[0], p[1], p[2] + lift];
        let plan: Vec<delaunator::Point> =
            (0..indices.len()).map(|i| delaunator::Point { x: at(i)[0], y: at(i)[1] }).collect();
        let tri = if indices.len() >= 3 { delaunator::triangulate(&plan).triangles } else { Vec::new() };
        if tri.is_empty() {
            // too few points (or all in a line): a small square of each
            let h = spacing(&overlay.points).map_or(b.extent().max(1.0) * 0.01, |d| d / 4.0);
            for i in 0..indices.len() {
                let p = up(at(i));
                let quad = [
                    [p[0] - h, p[1] - h, p[2]],
                    [p[0] + h, p[1] - h, p[2]],
                    [p[0] + h, p[1] + h, p[2]],
                    [p[0] - h, p[1] + h, p[2]],
                ];
                b.quad(quad, color(value(i)), 1.0);
            }
            continue;
        }
        let length = |i: usize, j: usize| (plan[i].x - plan[j].x).hypot(plan[i].y - plan[j].y);
        // the distance of each point to its nearest neighbour: the Delaunay graph holds it
        let mut nearest = vec![f64::INFINITY; plan.len()];
        for t in tri.as_chunks::<3>().0 {
            for (i, j) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                let d = length(i, j);
                nearest[i] = nearest[i].min(d);
                nearest[j] = nearest[j].min(d);
            }
        }
        for t in tri.as_chunks::<3>().0 {
            let keep = match &cover {
                Some(cover) => {
                    let c = [0, 1].map(|d| t.iter().map(|&i| at(i)[d]).sum::<f64>() / 3.0);
                    cover.contains(c)
                }
                None => {
                    let longest = length(t[0], t[1]).max(length(t[1], t[2])).max(length(t[2], t[0]));
                    longest <= 4.0 * nearest[t[0]].max(nearest[t[1]]).max(nearest[t[2]])
                }
            };
            if !keep {
                continue;
            }
            b.colored_triangle(t.map(|i| up(at(i))), t.map(|i| color(value(i))));
        }
    }
    ColorScale {
        min: lo,
        max: hi,
        label: overlay.label.clone(),
        smoothed: true,
        levels: 64,
        note: "interpolated between points",
    }
}

/// Quadrilaterals in plan, binned in a grid, to ask whether a point lies on one of them.
struct PlanCover<'a> {
    quads: &'a [V3x4],
    origin: [f64; 2],
    cell: f64,
    columns: usize,
    bins: HashMap<(usize, usize), Vec<usize>>,
}

impl<'a> PlanCover<'a> {
    fn new(quads: &'a [V3x4]) -> Self {
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        let mut sizes = Vec::with_capacity(quads.len());
        for q in quads {
            let (mut a, mut b) = ([f64::MAX; 2], [f64::MIN; 2]);
            for p in q {
                for d in 0..2 {
                    a[d] = a[d].min(p[d]);
                    b[d] = b[d].max(p[d]);
                }
            }
            sizes.push((b[0] - a[0]).max(b[1] - a[1]));
            for d in 0..2 {
                lo[d] = lo[d].min(a[d]);
                hi[d] = hi[d].max(b[d]);
            }
        }
        sizes.sort_by(f64::total_cmp);
        let cell = sizes.get(sizes.len() / 2).copied().unwrap_or(1.0).max(1e-6) * 2.0;
        let columns = (((hi[0] - lo[0]) / cell).ceil() as usize).max(1) + 1;
        let mut cover = PlanCover { quads, origin: lo, cell, columns, bins: HashMap::new() };
        for (k, q) in quads.iter().enumerate() {
            let (a, b) = q.iter().fold(([f64::MAX; 2], [f64::MIN; 2]), |(a, b), p| {
                ([a[0].min(p[0]), a[1].min(p[1])], [b[0].max(p[0]), b[1].max(p[1])])
            });
            let (i0, j0) = cover.bin(a);
            let (i1, j1) = cover.bin(b);
            for i in i0..=i1 {
                for j in j0..=j1 {
                    cover.bins.entry((i, j)).or_default().push(k);
                }
            }
        }
        cover
    }

    fn bin(&self, p: [f64; 2]) -> (usize, usize) {
        let at = |d: usize| (((p[d] - self.origin[d]) / self.cell).floor().max(0.0)) as usize;
        (at(0).min(self.columns), at(1))
    }

    /// Whether `p` (plan) lies in one of the quadrilaterals (either winding, a small tolerance).
    fn contains(&self, p: [f64; 2]) -> bool {
        let Some(candidates) = self.bins.get(&self.bin(p)) else { return false };
        candidates.iter().any(|&k| {
            let q = &self.quads[k];
            let side = |a: V3, b: V3| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
            let s: Vec<f64> = (0..4).map(|i| side(q[i], q[(i + 1) % 4])).collect();
            let scale = self.cell * self.cell * 1e-9;
            s.iter().all(|v| *v >= -scale) || s.iter().all(|v| *v <= scale)
        })
    }
}

/// Body output values at one phase on their faces (or squares), each moved with its body when
/// that body moves; a fixed scale over the period (signed for a component, from 0 for a length).
pub(super) fn add_animated_body(
    b: &mut Builder,
    project: &Project,
    frame: &crate::animation::Frame,
    v: &crate::animation::BodyValues,
) -> ColorScale {
    let peak = v.peak().max(1e-30);
    let (lo, hi) = if v.component.is_some() { (-peak, peak) } else { (0.0, peak) };
    let levels = 16;
    let lift = b.extent().max(1.0) * 0.002;
    let mut order: Vec<usize> = (0..v.points.len()).collect();
    order.sort_by_key(|&k| v.bodies.get(k).copied().flatten());
    let mut k0 = 0;
    while k0 < order.len() {
        let body = v.bodies.get(order[k0]).copied().flatten();
        let marks = b.marks();
        let mut k1 = k0;
        while k1 < order.len() && v.bodies.get(order[k1]).copied().flatten() == body {
            let k = order[k1];
            k1 += 1;
            let (Some(p), Some(n)) = (v.points.get(k), v.normals.get(k)) else { continue };
            let t = (v.at(k, frame.phase) - lo) / (hi - lo);
            let bin = ((t * (levels - 1) as f64) as i64).clamp(0, levels as i64 - 1);
            let color = heat_color(bin as f64 / (levels - 1) as f64);
            let up = |x: V3| [x[0] + n[0] * lift, x[1] + n[1] * lift, x[2] + n[2] * lift];
            match v.faces.get(k).filter(|f| !f.is_empty()) {
                Some(faces) => faces.iter().for_each(|q| b.quad(q.map(up), color, 1.0)),
                None => {
                    let h = v.sizes.get(k).copied().unwrap_or(0.0).max(1e-6);
                    let t0 = if n[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
                    let u = cross(*n, t0);
                    let ul = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt().max(1e-12);
                    let u = u.map(|x| x / ul * h);
                    let w = cross(*n, u);
                    let corner = |a: f64, d: f64| {
                        up([p[0] + a * u[0] + d * w[0], p[1] + a * u[1] + d * w[1], p[2] + a * u[2] + d * w[2]])
                    };
                    b.quad([corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)], color, 1.0);
                }
            }
        }
        if let Some(i) = body
            && let (Some(project_body), Some((_, motion))) =
                (project.bodies.get(i), frame.response.bodies.iter().find(|(j, _)| *j == i))
        {
            b.move_since(marks, &crate::animation::Placement::of(project_body, motion, frame.phase, frame.amplitude));
        }
        k0 = k1;
    }
    ColorScale { min: lo, max: hi, label: v.label.clone(), smoothed: false, levels, note: SOLVER_VALUES }
}

/// The (i, j) node of a point on the grid (Cartesian: column and row); far off for a grid that
/// cannot place it.
pub fn grid_index(p: V3, grid: &FieldPoints) -> (i64, i64) {
    grid.index_of(p).unwrap_or((i64::MIN / 4, i64::MIN / 4))
}

/// The cell of the lattice from (i0, j0) to (i1, j1) (fractional node indices) at height z. Rings
/// end at the first and the last one: a polar or body-fitted heatmap starts exactly at its first
/// ring, next to the body.
fn lattice_quad(grid: &FieldPoints, i: [f64; 2], j: [f64; 2], z: f64) -> Option<[V3; 4]> {
    let i = if grid.layout == GridLayout::Cartesian {
        i
    } else {
        let last = (grid.dims().0 - 1).max(0) as f64;
        i.map(|v| v.clamp(0.0, last))
    };
    let at = |a: f64, b: f64| grid.plan_at(a, b).map(|p| [p[0], p[1], z]);
    Some([at(i[0], j[0])?, at(i[1], j[0])?, at(i[1], j[1])?, at(i[0], j[1])?])
}

/// The heatmap of values at the field points, on the grid's lattice: the points are its nodes, and
/// each cell between four nodes that were all solved is filled between them (with `smoothing` by
/// bilinear interpolation, in `HEATMAP_SUB` x `HEATMAP_SUB` pieces each coloured by its value;
/// without, in one colour, the mean of its corners). A node is shown at its own value; nothing is
/// drawn beyond the outermost nodes or in a cell with a point the engine left out. A grid of one
/// row or column (no cells) shows a small square of each point's value instead.
pub(super) fn add_heatmap(b: &mut Builder, field: &FieldOverlay, smoothing: bool) -> ColorScale {
    let g = &field.grid;
    let nodes: HashMap<(i64, i64), (f64, f64)> =
        field.points.iter().zip(&field.values).map(|(p, v)| (grid_index(*p, g), (*v, p[2]))).collect();
    let (lo, hi) = field.values.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, c), v| (a.min(*v), c.max(*v)));
    let normalize = |v: f64| if spread(lo, hi) { (v - lo) / (hi - lo) } else { 0.5 };
    let levels = if smoothing { 64 } else { 16 };
    let colour = |v: f64| {
        let bin = ((normalize(v) * (levels - 1) as f64) as i64).clamp(0, levels as i64 - 1);
        heat_color(bin as f64 / (levels - 1) as f64)
    };
    let (ni, nj) = g.dims();
    let cells_j = if g.periodic() { nj } else { nj - 1 };
    let sub = if smoothing { HEATMAP_SUB } else { 1 };
    let mut drawn = false;
    for i in 0..ni - 1 {
        for j in 0..cells_j.max(0) {
            let corner = |di: i64, dj: i64| nodes.get(&g.wrap(i + di, j + dj)).copied();
            let (Some(a), Some(c10), Some(c11), Some(c01)) = (corner(0, 0), corner(1, 0), corner(1, 1), corner(0, 1))
            else {
                continue;
            };
            let z = (a.1 + c10.1 + c11.1 + c01.1) / 4.0;
            let bilinear = |u: f64, v: f64| {
                a.0 * (1.0 - u) * (1.0 - v) + c10.0 * u * (1.0 - v) + c11.0 * u * v + c01.0 * (1.0 - u) * v
            };
            for su in 0..sub {
                for sv in 0..sub {
                    let (u0, u1) = (su as f64 / sub as f64, (su + 1) as f64 / sub as f64);
                    let (v0, v1) = (sv as f64 / sub as f64, (sv + 1) as f64 / sub as f64);
                    let value = bilinear((u0 + u1) / 2.0, (v0 + v1) / 2.0);
                    let (x, y) = (i as f64, j as f64);
                    if let Some(quad) = lattice_quad(g, [x + u0, x + u1], [y + v0, y + v1], z) {
                        b.quad(quad, colour(value), 0.82);
                        drawn = true;
                    }
                }
            }
        }
    }
    if !drawn && (ni < 2 || cells_j < 1) {
        // no cell (one row, column or ring of points): a square in plan about each point, half
        // the distance to its nearest neighbour wide (a ring's lattice has no width to span)
        let n = field.points.len();
        for (k, (p, v)) in field.points.iter().zip(&field.values).enumerate() {
            let nearest = (0..n)
                .filter(|&m| m != k)
                .map(|m| {
                    let q = field.points[m];
                    ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt()
                })
                .filter(|d| *d > 1e-9)
                .fold(f64::INFINITY, f64::min);
            let h = if nearest.is_finite() { nearest / 4.0 } else { 0.5 };
            let quad = [
                [p[0] - h, p[1] - h, p[2]],
                [p[0] + h, p[1] - h, p[2]],
                [p[0] + h, p[1] + h, p[2]],
                [p[0] - h, p[1] + h, p[2]],
            ];
            b.quad(quad, colour(*v), 0.82);
        }
    }
    let note = if smoothing { "interpolated between points" } else { "cell means" };
    ColorScale { min: lo, max: hi, label: field.label.clone(), smoothed: smoothing, levels, note }
}

/// The note of a colour bar of the solver values themselves (no heatmap cells).
const SOLVER_VALUES: &str = "solver values";

/// The pieces per side a heatmap cell is filled in when interpolated.
pub(super) const HEATMAP_SUB: usize = 6;

/// The plane of a polar or body-fitted grid: the cells between its nodes, translucent.
pub(super) fn add_lattice_plane(b: &mut Builder, grid: &FieldPoints) {
    let (ni, nj) = grid.dims();
    let cells_j = if grid.periodic() { nj } else { nj - 1 };
    let lift = b.extent().max(1.0) * 1e-3;
    for j in 0..cells_j.max(0) {
        for i in 0..(ni - 1).max(0) {
            let (i, j) = (i as f64, j as f64);
            if let Some(q) = lattice_quad(grid, [i, i + 1.0], [j, j + 1.0], grid.z + lift) {
                b.quad(q, FIELD_PLANE, 0.12);
            }
        }
    }
    for p in grid.plan_nodes() {
        b.include([p[0], p[1], grid.z]);
    }
}

pub(super) fn add_markers(b: &mut Builder, grid: &FieldPoints, points: &[V3]) {
    // the distance to the neighbouring nodes of the first one (a list: none)
    let first = grid.plan_at(0.0, 0.0);
    let gap = |i: f64, j: f64| match (first, grid.plan_at(i, j)) {
        (Some(a), Some(c)) if grid.list.is_none() => (c[0] - a[0]).hypot(c[1] - a[1]),
        _ => 0.0,
    };
    let (dx, dy) =
        (if grid.dims().0 > 1 { gap(1.0, 0.0) } else { 0.0 }, if grid.dims().1 > 1 { gap(0.0, 1.0) } else { 0.0 });
    let mut spacing = (if dx > 0.0 { dx } else { dy }).min(if dy > 0.0 { dy } else { dx });
    if spacing.is_nan() || spacing <= 0.0 {
        spacing = b.extent().max(1.0) * 0.05;
    }
    let (h, lift) = (spacing * 0.18, spacing * 0.01);
    for p in points {
        Builder::line(&mut b.scene.markers, [p[0] - h, p[1], p[2] + lift], [p[0] + h, p[1], p[2] + lift], MARKER);
        Builder::line(&mut b.scene.markers, [p[0], p[1] - h, p[2] + lift], [p[0], p[1] + h, p[2] + lift], MARKER);
    }
    b.scene.marker_size = h;
    b.scene.field_points = points.to_vec();
}
