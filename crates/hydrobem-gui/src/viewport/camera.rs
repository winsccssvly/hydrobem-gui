//! The orbit camera of the viewport (z up): left drag rotates, middle drag pans, the wheel zooms.

use gui_core::project::V3;

pub type Mat4 = [[f32; 4]; 4];

const FOV_Y: f64 = 42.0;
pub const DEFAULT_YAW: f64 = -0.7;
pub const DEFAULT_PITCH: f64 = 0.45;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Iso,
    Front,
    Top,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub yaw: f64,
    pub pitch: f64,
    pub distance: f64,
    pub target: V3,
    /// The extent of the scene last fitted (for the clipping planes).
    pub extent: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self { yaw: DEFAULT_YAW, pitch: DEFAULT_PITCH, distance: 10.0, target: [0.0; 3], extent: 5.0 }
    }
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(a: V3) -> V3 {
    let l = dot(a, a).sqrt().max(1e-300);
    a.map(|x| x / l)
}

impl Camera {
    /// Centres the scene bounds and backs off to see all of it.
    pub fn fit(&mut self, (min, max): (V3, V3)) {
        self.target = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0, (min[2] + max[2]) / 2.0];
        let d = sub(max, min);
        self.extent = d[0].max(d[1]).max(d[2]);
        self.distance = 2.5f64.max(self.extent * 2.2);
    }

    pub fn set_view(&mut self, view: View, bounds: (V3, V3)) {
        (self.yaw, self.pitch) = match view {
            View::Iso => (DEFAULT_YAW, DEFAULT_PITCH),
            View::Front => (-std::f64::consts::FRAC_PI_2, 0.0),
            View::Top => (-std::f64::consts::FRAC_PI_2, 1.44),
            View::Right => (std::f64::consts::PI, 0.0),
        };
        self.fit(bounds);
    }

    pub fn eye(&self) -> V3 {
        let h = self.distance * self.pitch.cos();
        [
            self.target[0] + h * self.yaw.cos(),
            self.target[1] + h * self.yaw.sin(),
            self.target[2] + self.distance * self.pitch.sin(),
        ]
    }

    /// The direction the camera looks.
    pub fn look(&self) -> V3 {
        normalize(sub(self.target, self.eye()))
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw -= f64::from(dx) * 0.009;
        self.pitch = (self.pitch + f64::from(dy) * 0.009).clamp(-1.45, 1.45);
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let look = self.look();
        let right = normalize(cross(look, [0.0, 0.0, 1.0]));
        let up = cross(right, look);
        let s = self.distance / 650.0;
        for i in 0..3 {
            self.target[i] += right[i] * (-f64::from(dx) * s) + up[i] * (f64::from(dy) * s);
        }
    }

    /// `delta` in scroll points (positive away from the user zooms in); one wheel notch (40
    /// points) zooms by exp(-120 / 900).
    pub fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance * (-f64::from(delta) / 300.0).exp()).clamp(1e-4, 1e9);
    }

    /// The view-projection matrix (column-major, wgpu clip space with depth 0..1).
    pub fn view_projection(&self, aspect: f64) -> Mat4 {
        let near = (self.distance * 1e-3).max(1e-4);
        let far = self.distance * 10.0 + self.extent * 20.0;
        let f = 1.0 / (FOV_Y.to_radians() / 2.0).tan();
        // right-handed look-at
        let eye = self.eye();
        let fwd = self.look();
        let mut up = [0.0, 0.0, 1.0];
        if dot(fwd, up).abs() > 0.9999 {
            up = [0.0, 1.0, 0.0];
        }
        let s = normalize(cross(fwd, up));
        let u = cross(s, fwd);
        let view = [
            [s[0], u[0], -fwd[0], 0.0],
            [s[1], u[1], -fwd[1], 0.0],
            [s[2], u[2], -fwd[2], 0.0],
            [-dot(s, eye), -dot(u, eye), dot(fwd, eye), 1.0],
        ];
        let proj = [
            [f / aspect, 0.0, 0.0, 0.0],
            [0.0, f, 0.0, 0.0],
            [0.0, 0.0, far / (near - far), -1.0],
            [0.0, 0.0, near * far / (near - far), 0.0],
        ];
        let mut m = [[0.0f32; 4]; 4];
        for (c, column) in m.iter_mut().enumerate() {
            for (r, value) in column.iter_mut().enumerate() {
                *value = (0..4).map(|k| proj[k][r] * view[c][k]).sum::<f64>() as f32;
            }
        }
        m
    }
}

/// A point projected to the viewport (pixels from its top left), `None` behind the camera.
pub fn project(m: &Mat4, p: V3, width: f32, height: f32) -> Option<(f32, f32, f32)> {
    let p = [p[0] as f32, p[1] as f32, p[2] as f32, 1.0];
    let clip: [f32; 4] = std::array::from_fn(|r| (0..4).map(|c| m[c][r] * p[c]).sum());
    (clip[3] > 0.0)
        .then(|| ((clip[0] / clip[3] + 1.0) / 2.0 * width, (1.0 - clip[1] / clip[3]) / 2.0 * height, clip[2] / clip[3]))
}

/// The field point drawn nearest to (x, y), within 8 px.
pub fn pick(m: &Mat4, points: &[V3], x: f32, y: f32, width: f32, height: f32) -> Option<usize> {
    let mut best = None;
    let mut best_d = 8.0f32 * 8.0;
    for (i, p) in points.iter().enumerate() {
        if let Some((px, py, _)) = project(m, *p, width, height) {
            let d = (px - x).powi(2) + (py - y).powi(2);
            if d < best_d {
                (best_d, best) = (d, Some(i));
            }
        }
    }
    best
}

/// The stored record of the quadrilateral drawn nearest to the eye at (x, y), if one is there.
pub fn pick_record(m: &Mat4, quads: &[(usize, [V3; 4])], x: f32, y: f32, width: f32, height: f32) -> Option<usize> {
    let mut best: Option<(f32, usize)> = None;
    for (record, q) in quads {
        let Some(p) = q.iter().map(|v| project(m, *v, width, height)).collect::<Option<Vec<_>>>() else { continue };
        for [i, j, k] in [[0, 1, 2], [0, 2, 3]] {
            let (a, b, c) = (p[i], p[j], p[k]);
            // barycentric coordinates on the screen
            let d = (b.1 - c.1) * (a.0 - c.0) + (c.0 - b.0) * (a.1 - c.1);
            if d.abs() < 1e-9 {
                continue;
            }
            let la = ((b.1 - c.1) * (x - c.0) + (c.0 - b.0) * (y - c.1)) / d;
            let lb = ((c.1 - a.1) * (x - c.0) + (a.0 - c.0) * (y - c.1)) / d;
            let lc = 1.0 - la - lb;
            if la >= 0.0 && lb >= 0.0 && lc >= 0.0 {
                let z = la * a.2 + lb * b.2 + lc * c.2;
                if best.is_none_or(|(nearest, _)| z < nearest) {
                    best = Some((z, *record));
                }
            }
        }
    }
    best.map(|(_, r)| r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_projects_to_the_centre() {
        let mut c = Camera::default();
        c.fit(([-1.0, -1.0, -1.0], [1.0, 1.0, 0.0]));
        let m = c.view_projection(1.5);
        let (x, y, z) = project(&m, c.target, 300.0, 200.0).unwrap();
        assert!((x - 150.0).abs() < 1e-3 && (y - 100.0).abs() < 1e-3 && z > 0.0 && z < 1.0);
        assert_eq!(pick(&m, &[[5.0, 5.0, 5.0], c.target], 151.0, 99.0, 300.0, 200.0), Some(1));
    }

    #[test]
    fn the_nearest_quadrilateral_is_picked() {
        let mut c = Camera::default();
        c.fit(([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]));
        let m = c.view_projection(1.0);
        // two squares about the target, one nearer the eye
        let look = c.look();
        let square = |shift: f64| -> [V3; 4] {
            let at = |dx: f64, dz: f64| {
                [c.target[0] - look[0] * shift + dx, c.target[1] - look[1] * shift, c.target[2] - look[2] * shift + dz]
            };
            [at(-0.3, -0.3), at(0.3, -0.3), at(0.3, 0.3), at(-0.3, 0.3)]
        };
        let quads = vec![(4, square(0.0)), (9, square(0.5))];
        assert_eq!(pick_record(&m, &quads, 50.0, 50.0, 100.0, 100.0), Some(9));
        assert_eq!(pick_record(&m, &quads[..1], 50.0, 50.0, 100.0, 100.0), Some(4));
        assert_eq!(pick_record(&m, &quads, 1.0, 1.0, 100.0, 100.0), None);
    }

    #[test]
    fn up_is_up_on_screen() {
        let c = Camera { yaw: -std::f64::consts::FRAC_PI_2, pitch: 0.0, ..Camera::default() };
        let m = c.view_projection(1.0);
        let (_, low, _) = project(&m, [0.0, 0.0, 0.0], 100.0, 100.0).unwrap();
        let (_, high, _) = project(&m, [0.0, 0.0, 1.0], 100.0, 100.0).unwrap();
        assert!(high < low);
        // front view: +x to the right
        let (left, _, _) = project(&m, [0.0, 0.0, 0.0], 100.0, 100.0).unwrap();
        let (right, _, _) = project(&m, [1.0, 0.0, 0.0], 100.0, 100.0).unwrap();
        assert!(right > left);
    }
}
