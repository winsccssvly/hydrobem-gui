//! The free surface (still or raised by the incident wave), walls and the sea bed.

use super::*;

/// A plan-view rectangle: its x and y ranges.
pub(super) type Plan = ([f64; 2], [f64; 2]);

/// The square 1.6 times the plan size of `min`..`max` about its centre (the open sea's free surface).
pub(super) fn around(min: V3, max: V3) -> Plan {
    let span = ((max[0] - min[0]).max(max[1] - min[1]) * 1.6).max(1.0);
    let (x0, y0) = ((min[0] + max[0] - span) / 2.0, (min[1] + max[1] - span) / 2.0);
    ([x0, x0 + span], [y0, y0 + span])
}

/// Nearly square cells over a plan, `cells` along its longer side: the count and step along x
/// and along y.
fn cells_over((x, y): Plan, cells: usize) -> ((usize, f64), (usize, f64)) {
    let (w, h) = ((x[1] - x[0]).max(1e-9), (y[1] - y[0]).max(1e-9));
    let step = w.max(h) / cells as f64;
    let count = |l: f64| ((l / step - 1e-6).ceil() as usize).max(1);
    let (nx, ny) = (count(w), count(h));
    ((nx, w / nx as f64), (ny, h / ny as f64))
}

/// The free surface z = 0 around what is drawn, translucent.
pub(super) fn add_free_surface(b: &mut Builder) {
    if b.has_points {
        let plan = around(b.min(), b.max());
        add_free_surface_over(b, plan, None);
    }
}

/// The free surface z = 0 over a plan, translucent; cells whose centre lies in `hole` (the
/// field-point plane drawn there instead) are left out.
pub(super) fn add_free_surface_over(b: &mut Builder, plan: Plan, hole: Option<Plan>) {
    // small cells: the transparent triangles are sorted by their centres, and a plane of a few large
    // triangles would sort wrongly against the hull's small ones (blotches where they cross)
    let ((nx, sx), (ny, sy)) = cells_over(plan, FREE_SURFACE_CELLS);
    let inside = |(hx, hy): Plan, x: f64, y: f64| hx[0] <= x && x <= hx[1] && hy[0] <= y && y <= hy[1];
    let mut any = false;
    for i in 0..nx {
        for j in 0..ny {
            let (x, y) = (plan.0[0] + sx * i as f64, plan.1[0] + sy * j as f64);
            if hole.is_some_and(|h| inside(h, x + sx / 2.0, y + sy / 2.0)) {
                continue;
            }
            // not in the bounds: the plane does not change the fit
            let (p, q, r, t) = ([x, y, 0.0], [x + sx, y, 0.0], [x + sx, y + sy, 0.0], [x, y + sy, 0.0]);
            b.triangle(p, q, r, WATER, 0.28);
            b.triangle(p, r, t, WATER, 0.28);
            any = true;
        }
    }
    if any {
        b.note("Free Surface", WATER);
    }
}

/// The cells of the free-surface plane along each side.
pub const FREE_SURFACE_CELLS: usize = 32;

/// The free surface around the bodies raised by the incident wave of wave number `k` and heading
/// (deg) at the phase ωt: η = A cos(ωt − k (x cos β + y sin β)), the engine's incident wave. Cells
/// of at most a sixteenth of the wave length (up to 200 along a side).
/// Between walls the surface covers the walls' plan.
pub(super) fn add_incident_surface(
    b: &mut Builder,
    walls: Option<Plan>,
    k: f64,
    heading: f64,
    phase: f64,
    amplitude: f64,
) {
    if !b.has_points {
        return;
    }
    let (min, max) = (b.min(), b.max());
    let size = (max[0] - min[0]).max(max[1] - min[1]).max(1.0);
    let length = if k > 0.0 { 2.0 * std::f64::consts::PI / k } else { size };
    let plan = walls.unwrap_or_else(|| {
        // one and a half wave lengths, at least the plane's size and at most eight times the scene
        let span = (size * 1.6).max((1.5 * length).min(size * 8.0));
        let (x0, y0) = ((min[0] + max[0] - span) / 2.0, (min[1] + max[1] - span) / 2.0);
        ([x0, x0 + span], [y0, y0 + span])
    });
    let longest = (plan.0[1] - plan.0[0]).max(plan.1[1] - plan.1[0]);
    let cells = ((longest / (length / 16.0)).ceil() as usize).clamp(FREE_SURFACE_CELLS, 200);
    let ((nx, sx), (ny, sy)) = cells_over(plan, cells);
    let (s, c) = heading.to_radians().sin_cos();
    let at = |x: f64, y: f64| [x, y, amplitude * (phase - k * (x * c + y * s)).cos()];
    for i in 0..nx {
        for j in 0..ny {
            let (x, y) = (plan.0[0] + sx * i as f64, plan.1[0] + sy * j as f64);
            let (p, q, r, t) = (at(x, y), at(x + sx, y), at(x + sx, y + sy), at(x, y + sy));
            b.triangle(p, q, r, WATER, 0.35);
            b.triangle(p, r, t, WATER, 0.35);
        }
    }
    b.note("Incident Wave", WATER);
}

/// Clips a plan-view rectangle to the fluid side of the domain (the engine's rule): x >= 0 beside
/// an x wall, y >= 0 beside a y wall, both for orthogonal walls, |y| <= W/2 in a channel.
pub fn clip_to_fluid(x: &mut [f64; 2], y: &mut [f64; 2], domain: Domain, channel_width: f64) {
    if matches!(domain, Domain::XWall | Domain::OrthogonalWalls) {
        x[0] = x[0].max(0.0);
        x[1] = x[1].max(x[0]);
    }
    if matches!(domain, Domain::YWall | Domain::OrthogonalWalls) {
        y[0] = y[0].max(0.0);
        y[1] = y[1].max(y[0]);
    }
    if domain == Domain::Channel {
        let half = channel_width.max(0.1) / 2.0;
        y[0] = y[0].clamp(-half, half);
        y[1] = y[1].clamp(y[0], half);
    }
}

/// Walls and the sea bed, only where the water is. The walls stand from the sea bed to a little
/// above the water; with walls, their plan (the free surface's, between them).
pub(super) fn add_boundaries(b: &mut Builder, project: &Project) -> Option<Plan> {
    let env = &project.environment;
    let extent = b.extent().max(1.0);
    let (min, max, any) = (b.min(), b.max(), b.has_points);
    let (cx, cy) = if any { ((min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0) } else { (0.0, 0.0) };
    let top = if any { max[2].max(0.0) } else { 0.0 };
    let finite = env.depth_type == DepthType::Finite;
    let bottom =
        if finite { -env.depth.max(0.01) } else { (-extent).min(if any { min[2] - extent * 0.25 } else { -extent }) };
    // a freeboard of a tenth of the water column, at most a tenth of the scene
    let top = top + (0.1 * (top - bottom)).min(0.1 * extent);
    let height = top - bottom;
    let domain_span = if env.domain == Domain::Channel { env.channel_width * 2.0 } else { 0.0 };
    let depth_span = if finite { env.depth * 1.5 } else { 0.0 };
    let span = (extent * 1.6).max(10.0).max(domain_span).max(depth_span);
    let thickness = (extent * 0.012).max(0.01);
    let mut x = [cx - span / 2.0, cx + span / 2.0];
    let mut y = [cy - span / 2.0, cy + span / 2.0];
    if env.domain == Domain::Channel {
        y = [-span, span]; // clipped to the channel width below
    }
    clip_to_fluid(&mut x, &mut y, env.domain, env.channel_width);
    if finite || env.domain != Domain::OpenSea {
        b.note("Wall / Bottom", BOUNDARY);
    }

    let x_wall = |b: &mut Builder, at: f64| {
        b.cuboid(
            [at, (y[0] + y[1]) / 2.0, (top + bottom) / 2.0],
            [thickness, (y[1] - y[0]).max(0.01), height],
            BOUNDARY,
            0.28,
        );
        b.include([at, y[0], bottom]);
        b.include([at, y[1], top]);
    };
    let y_wall = |b: &mut Builder, at: f64| {
        b.cuboid(
            [(x[0] + x[1]) / 2.0, at, (top + bottom) / 2.0],
            [(x[1] - x[0]).max(0.01), thickness, height],
            BOUNDARY,
            0.28,
        );
        b.include([x[0], at, bottom]);
        b.include([x[1], at, top]);
    };
    if matches!(env.domain, Domain::XWall | Domain::OrthogonalWalls) {
        x_wall(b, 0.0);
    }
    if matches!(env.domain, Domain::YWall | Domain::OrthogonalWalls) {
        y_wall(b, 0.0);
    }
    if env.domain == Domain::Channel {
        y_wall(b, y[0]);
        y_wall(b, y[1]);
    }
    if finite {
        b.cuboid(
            [(x[0] + x[1]) / 2.0, (y[0] + y[1]) / 2.0, bottom],
            [(x[1] - x[0]).max(0.01), (y[1] - y[0]).max(0.01), thickness],
            BOTTOM,
            0.35,
        );
        b.include([x[0], y[0], bottom]);
        b.include([x[1], y[1], bottom]);
    }
    (env.domain != Domain::OpenSea).then_some((x, y))
}
