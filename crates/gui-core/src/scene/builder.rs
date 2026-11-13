//! The builder of a scene: its triangles, lines and bounds, and the colour helpers.

use super::*;

pub(super) fn lighten(c: Rgb) -> Rgb {
    c.map(|v| v.saturating_add(80))
}

/// The jet colour of t in 0..1 (blue → cyan → yellow → red).
pub fn heat_color(t: f64) -> Rgb {
    let channel = |center: f64| (255.0 * (1.5 - (4.0 * t - center).abs()).clamp(0.0, 1.0)) as u8;
    [channel(3.0), channel(2.0), channel(1.0)]
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(super) fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn f32x3(v: V3) -> [f32; 3] {
    v.map(|x| x as f32)
}

pub(super) fn rgba(c: Rgb, alpha: f64) -> [f32; 4] {
    [f32::from(c[0]) / 255.0, f32::from(c[1]) / 255.0, f32::from(c[2]) / 255.0, alpha as f32]
}

pub(super) struct Builder {
    pub(super) scene: Scene,
    pub(super) min: V3,
    pub(super) max: V3,
    pub(super) has_points: bool,
}

impl Builder {
    pub(super) fn include(&mut self, p: V3) {
        if !self.has_points {
            (self.min, self.max, self.has_points) = (p, p, true);
            return;
        }
        for ((lo, hi), v) in self.min.iter_mut().zip(self.max.iter_mut()).zip(p) {
            *lo = lo.min(v);
            *hi = hi.max(v);
        }
    }

    pub(super) fn min(&self) -> V3 {
        if self.has_points { self.min } else { [-1.0; 3] }
    }

    pub(super) fn max(&self) -> V3 {
        if self.has_points { self.max } else { [1.0; 3] }
    }

    pub(super) fn extent(&self) -> f64 {
        let d = sub(self.max(), self.min());
        d[0].max(d[1]).max(d[2])
    }

    pub(super) fn triangle(&mut self, a: V3, b: V3, c: V3, color: Rgb, alpha: f64) {
        let n = cross(sub(b, a), sub(c, a));
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = if len > 0.0 { n.map(|x| x / len) } else { [0.0, 0.0, 1.0] };
        let target = if alpha >= 0.999 { &mut self.scene.opaque } else { &mut self.scene.transparent };
        for p in [a, b, c] {
            target.push(Vertex { position: f32x3(p), normal: f32x3(n), color: rgba(color, alpha) });
        }
    }

    /// An opaque triangle coloured at its corners (interpolated between them), facing up.
    pub(super) fn colored_triangle(&mut self, p: [V3; 3], colors: [Rgb; 3]) {
        for k in 0..3 {
            self.scene.opaque.push(Vertex {
                position: f32x3(p[k]),
                normal: [0.0, 0.0, 1.0],
                color: rgba(colors[k], 1.0),
            });
            self.include(p[k]);
        }
    }

    pub(super) fn quad(&mut self, p: [V3; 4], color: Rgb, alpha: f64) {
        self.triangle(p[0], p[1], p[2], color, alpha);
        self.triangle(p[0], p[2], p[3], color, alpha);
        for v in p {
            self.include(v);
        }
    }

    /// A quadrilateral of a curved surface: shaded by the surface normals at its corners, outlined
    /// only along its patch's boundary.
    pub(super) fn smooth_quad(&mut self, s: &crate::mesh::SmoothQuad, color: Rgb, alpha: f64) {
        let target = if alpha >= 0.999 { &mut self.scene.opaque } else { &mut self.scene.transparent };
        for k in [0, 1, 2, 0, 2, 3] {
            target.push(Vertex {
                position: f32x3(s.corners[k]),
                normal: f32x3(s.normals[k]),
                color: rgba(color, alpha),
            });
        }
        for v in s.corners {
            self.include(v);
        }
        for k in 0..4 {
            if s.edges[k] {
                Builder::line(&mut self.scene.edges, s.corners[k], s.corners[(k + 1) % 4], lighten(color));
            }
        }
    }

    pub(super) fn cuboid(&mut self, center: V3, size: V3, color: Rgb, alpha: f64) {
        let h = size.map(|s| s / 2.0);
        let c: [V3; 8] = std::array::from_fn(|i| {
            let sx = if matches!(i % 4, 1 | 2) { 1.0 } else { -1.0 };
            let sy = if matches!(i % 4, 2 | 3) { 1.0 } else { -1.0 };
            let sz = if i >= 4 { 1.0 } else { -1.0 };
            [center[0] + sx * h[0], center[1] + sy * h[1], center[2] + sz * h[2]]
        });
        for f in [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]] {
            self.triangle(c[f[0]], c[f[1]], c[f[2]], color, alpha);
            self.triangle(c[f[0]], c[f[2]], c[f[3]], color, alpha);
        }
    }

    /// A triangle coloured per vertex (interpolated), not in the bounds.
    pub(super) fn shaded_triangle(&mut self, p: [V3; 3], colors: [Rgb; 3], alpha: f64) {
        let n = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = if len > 0.0 { n.map(|x| x / len) } else { [0.0, 0.0, 1.0] };
        let target = if alpha >= 0.999 { &mut self.scene.opaque } else { &mut self.scene.transparent };
        for (v, c) in p.into_iter().zip(colors) {
            target.push(Vertex { position: f32x3(v), normal: f32x3(n), color: rgba(c, alpha) });
        }
    }

    /// How many triangle vertices and edge vertices there are so far.
    pub(super) fn marks(&self) -> [usize; 3] {
        [self.scene.opaque.len(), self.scene.transparent.len(), self.scene.edges.len()]
    }

    /// Moves what was added since `marks` rigidly.
    pub(super) fn move_since(&mut self, marks: [usize; 3], placement: &crate::animation::Placement) {
        let point = |p: [f32; 3]| f32x3(placement.point(p.map(f64::from)));
        let vector = |v: [f32; 3]| f32x3(placement.vector(v.map(f64::from)));
        for v in self.scene.opaque[marks[0]..].iter_mut().chain(&mut self.scene.transparent[marks[1]..]) {
            v.position = point(v.position);
            v.normal = vector(v.normal);
        }
        for v in &mut self.scene.edges[marks[2]..] {
            v.position = point(v.position);
        }
    }

    pub(super) fn line(target: &mut Vec<LineVertex>, a: V3, b: V3, color: Rgb) {
        target.push(LineVertex { position: f32x3(a), color: rgba(color, 1.0) });
        target.push(LineVertex { position: f32x3(b), color: rgba(color, 1.0) });
    }

    /// Adds a legend entry once.
    pub(super) fn note(&mut self, name: &'static str, color: Rgb) {
        if !self.scene.legend.iter().any(|(n, _)| *n == name) {
            self.scene.legend.push((name, color));
        }
    }

    /// Body-local panels placed by the body, filled, outlined in a lighter shade.
    pub(super) fn panels(&mut self, body: &Body, panels: &[[V3; 4]], color: Rgb, alpha: f64) {
        if !panels.is_empty() {
            let name = match color {
                INTERIOR => "Interior Surface",
                DIPOLE => "Dipole",
                PRESSURE => "Pressure Surface",
                DAMPER => "Dipole Damper",
                TANK => "Tank",
                _ => "Body",
            };
            self.note(name, color);
        }
        let outlines = panels.len() <= OUTLINE_LIMIT;
        for q in panels {
            let p = q.map(|v| body.to_global(v));
            self.quad(p, color, alpha);
            if outlines {
                for i in 0..4 {
                    Builder::line(&mut self.scene.edges, p[i], p[(i + 1) % 4], lighten(color));
                }
            }
        }
    }
}
