//! Animation of a linear response (REQUIREMENTS.md, Animation). Every quantity is the real part
//! of its complex amplitude times e^{+iωt}, the engine's time factor, at the wave amplitude of the
//! results (Details → Waves), times a display scale (1: the real size). This is a display of the
//! solved amplitudes and phases: nothing is solved again.

use hydrobem_engine::Complex64;

use crate::project::{Body, FieldPoints, V3};

/// A linear response to animate, at the wave amplitude of the results (of a radiation mode, per
/// unit of its coordinate).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Response {
    /// rad/s
    pub omega: f64,
    /// The bodies that move: the project's body index and its rigid motion (surge, sway, heave in
    /// m; roll, pitch, yaw in rad), body axes about its rotation centre R.
    pub bodies: Vec<(usize, [Complex64; 6])>,
    /// Values at the field points of a grid.
    pub field: Option<ComplexField>,
    /// The incident wave drawn on the free surface when there is no field: its wave number (rad/m),
    /// heading (deg) and amplitude (m, the wave amplitude of the results).
    pub incident: Option<(f64, f64, f64)>,
    /// Values at the body output points, coloured on the hull (body pressure, velocities).
    pub body_values: Option<BodyValues>,
}

/// Complex values at the body output points, coloured on their faces in time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyValues {
    pub points: Vec<V3>,
    /// Unit normals into the fluid.
    pub normals: Vec<V3>,
    /// The faces each point colours (global, at rest); none: a square of `sizes` at the point.
    pub faces: crate::results::PointFaces,
    /// The half side of the square of a point without faces.
    pub sizes: Vec<f64>,
    /// The project's body of each point: it moves with that body.
    pub bodies: Vec<Option<usize>>,
    /// A scalar in the first component (pressure), or a vector (velocity).
    pub values: Vec<[Complex64; 3]>,
    /// The component coloured: one of x, y, z (signed), or None the length of the vector.
    pub component: Option<usize>,
    pub label: String,
}

impl BodyValues {
    /// The coloured value of point `k` at the phase ωt.
    pub fn at(&self, k: usize, phase: f64) -> f64 {
        let v = &self.values[k];
        match self.component {
            Some(c) => real_at(v[c.min(2)], phase),
            None => v.iter().map(|c| real_at(*c, phase).powi(2)).sum::<f64>().sqrt(),
        }
    }

    /// The largest coloured value over a period: the colour scale is fixed while it plays.
    pub fn peak(&self) -> f64 {
        self.values
            .iter()
            .map(|v| match self.component {
                Some(c) => v[c.min(2)].norm(),
                None => v.iter().map(|c| c.norm_sqr()).sum::<f64>().sqrt(),
            })
            .fold(0.0, f64::max)
    }
}

/// Complex values at the points of a field-point grid.
#[derive(Clone, Debug, PartialEq)]
pub struct ComplexField {
    pub grid: FieldPoints,
    pub points: Vec<V3>,
    pub values: Vec<Complex64>,
    /// Elevations (the points at z = 0 rise and fall with them); below z = 0 the values
    /// (p/ρg) colour the points only.
    pub elevation: bool,
    pub label: String,
}

/// One frame of an animation: the phase ωt and the display scale of the displacements (1: the real
/// size at the results' wave amplitude).
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    pub response: &'a Response,
    pub phase: f64,
    pub amplitude: f64,
}

/// Re{v e^{i phase}}.
pub fn real_at(v: Complex64, phase: f64) -> f64 {
    (v * Complex64::from_polar(1.0, phase)).re
}

type Matrix = [[f64; 3]; 3];

fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

fn apply(m: &Matrix, v: V3) -> V3 {
    std::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

fn transpose(m: &Matrix) -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| m[j][i]))
}

/// Rotation about x, then y, then z by the angles (rad).
fn rotation(angles: V3) -> Matrix {
    let (sx, cx) = angles[0].sin_cos();
    let (sy, cy) = angles[1].sin_cos();
    let (sz, cz) = angles[2].sin_cos();
    let x = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let y = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let z = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    multiply(&z, &multiply(&y, &x))
}

/// A body's rigid displacement at one instant, global: x → c + Q (x − c) + t.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    center: V3,
    rotation: Matrix,
    translation: V3,
}

impl Placement {
    /// The displacement of `body` by its motion at the phase ωt for the wave amplitude: body axes
    /// (the yaw of the body) about its rotation centre. The angles are applied as a finite
    /// rotation, so that a large display amplitude still moves the hull rigidly.
    pub fn of(body: &Body, motion: &[Complex64; 6], phase: f64, amplitude: f64) -> Placement {
        let m: [f64; 6] = std::array::from_fn(|k| real_at(motion[k], phase) * amplitude);
        let yaw = rotation([0.0, 0.0, body.yaw.to_radians()]);
        let local = rotation([m[3], m[4], m[5]]);
        Placement {
            center: body.to_global(body.reference),
            rotation: multiply(&yaw, &multiply(&local, &transpose(&yaw))),
            translation: apply(&yaw, [m[0], m[1], m[2]]),
        }
    }

    pub fn point(&self, p: V3) -> V3 {
        let d = apply(&self.rotation, [p[0] - self.center[0], p[1] - self.center[1], p[2] - self.center[2]]);
        std::array::from_fn(|i| self.center[i] + d[i] + self.translation[i])
    }

    pub fn vector(&self, v: V3) -> V3 {
        apply(&self.rotation, v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn the_real_part_follows_the_time_factor() {
        // e^{+iωt}: a quarter period later the imaginary part has turned into the negative real one
        let v = Complex64::new(0.0, 1.0);
        assert!((real_at(v, 0.0)).abs() < 1e-12);
        assert!((real_at(v, std::f64::consts::FRAC_PI_2) + 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_body_moves_in_its_axes_about_its_centre() {
        let body = Body { position: [10.0, 0.0, 0.0], yaw: 90.0, reference: [1.0, 0.0, 0.0], ..Body::default() };
        let surge = [
            Complex64::new(1.0, 0.0),
            Complex64::default(),
            Complex64::default(),
            Complex64::default(),
            Complex64::default(),
            Complex64::default(),
        ];
        // surge of the body is along global y when it is yawed by 90°
        let p = Placement::of(&body, &surge, 0.0, 2.0);
        assert!(close(p.point([10.0, 0.0, 0.0]), [10.0, 2.0, 0.0]));
        // yaw by 90° about R (body-local (1, 0, 0), global (10, 1, 0)) turns the origin to (11, 1, 0)
        let mut yaw = [Complex64::default(); 6];
        yaw[5] = Complex64::new(std::f64::consts::FRAC_PI_2, 0.0);
        let p = Placement::of(&body, &yaw, 0.0, 1.0);
        assert!(close(p.point([10.0, 0.0, 0.0]), [11.0, 1.0, 0.0]));
        assert!(close(p.vector([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]));
    }
}
