//! The geometry the viewport draws (REQUIREMENTS.md, Field points and viewport): bodies, interior
//! surfaces, dipole and pressure surfaces, lids, walls and the sea bed on the fluid side, the
//! field-point plane or the free-surface heatmap, field-point markers and the axes. Triangles with
//! normals and colours, lines with colours, in global coordinates.
//!
//! The heatmap's interpolation between the field points is visualization only: it does not add
//! solver samples.
//!
//! - `builder`: the scene's builder (triangles, lines, bounds, legend) and the colour helpers.
//! - `bodies`: bodies and their surfaces by role, the hull a trim cuts off, lids, mesh normals.
//! - `water`: the free surface (still or raised by the incident wave), walls and the sea bed.
//! - `overlays`: body and field values (still and animated), the heatmap, field plane, markers.

use std::collections::HashMap;

use hydrobem_engine::PanelRole;

use crate::field_layout::GridLayout;
use crate::project::{Body, DepthType, Domain, FieldPoints, InteriorMethod, PanelSurface, Project, SurfaceSource, V3};

mod bodies;
mod builder;
mod overlays;
mod water;

pub use bodies::{role_name, role_style, trim_point};
pub use builder::heat_color;
pub use overlays::grid_index;
pub use water::{FREE_SURFACE_CELLS, clip_to_fluid};

use bodies::*;
use builder::*;
use overlays::*;
use water::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

/// The colour bar of a heatmap: the range of the displayed values and its levels.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorScale {
    pub min: f64,
    pub max: f64,
    pub label: String,
    pub smoothed: bool,
    pub levels: usize,
    /// What the colours between the values are, under the bar: the heatmap's interpolation or
    /// cell means, else the solver values themselves.
    pub note: &'static str,
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub opaque: Vec<Vertex>,
    /// Translucent triangles (drawn after the opaque ones, without depth writes).
    pub transparent: Vec<Vertex>,
    pub edges: Vec<LineVertex>,
    pub markers: Vec<LineVertex>,
    pub axes: Vec<LineVertex>,
    pub bounds: (V3, V3),
    pub has_content: bool,
    pub color_scale: Option<ColorScale>,
    /// The marked field points (picking).
    pub field_points: Vec<V3>,
    /// A mesh shown alone: its quadrilaterals with their stored records (zero-based), every
    /// symmetry copy (picking and highlighting a record).
    pub records: Vec<(usize, [V3; 4])>,
    /// Whether the highlighted records are outlined (panels; the quadrilaterals of a patch mesh's
    /// tessellation are only filled).
    pub record_outlines: bool,
    pub marker_size: f64,
    /// What the scene draws, for the viewport's legend: name and colour, in drawing order.
    pub legend: Vec<(&'static str, Rgb)>,
}

/// Values at the body output points (body pressure, velocities), each drawn on its faces of the
/// hull, else as a small square of its area.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyOverlay {
    pub points: Vec<V3>,
    /// Unit normals into the fluid.
    pub normals: Vec<V3>,
    /// The area of each point, m²; 0 when the engine gives none (specified body points).
    pub areas: Vec<f64>,
    /// The faces each point colours: the panel whose centroid it is, or the quadrilaterals of the
    /// tessellated patch in its output cell (higher order); none: a square at the point.
    pub faces: crate::results::PointFaces,
    pub values: Vec<f64>,
    pub label: String,
    /// Points on horizontal surfaces (damping lids, pressure surfaces; the higher order gives
    /// them at the Gauss points of the patches, unevenly spaced): drawn as a triangulation in
    /// plan coloured between the points, not as squares, which overlap where they are dense.
    pub plane: bool,
    /// Of `plane` points: the surfaces they lie on (panels or tessellated patches, global); a
    /// triangle between points is drawn only over them. Empty: the triangles are judged by the
    /// spacing of their points instead.
    pub cover: std::sync::Arc<Vec<V3x4>>,
}

/// A quadrilateral of four points.
pub type V3x4 = [V3; 4];

/// The free-surface amplitudes to show on the grid they were solved on.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldOverlay {
    pub grid: FieldPoints,
    pub points: Vec<V3>,
    pub values: Vec<f64>,
    pub label: String,
}

type Rgb = [u8; 3];

pub const BODY_COLORS: [Rgb; 4] = [[50, 145, 187], [70, 165, 118], [170, 112, 62], [130, 94, 174]];
pub const SUPPRESSED: Rgb = [125, 130, 136];
pub const LID: Rgb = [72, 201, 126];
const LID_EDGE: Rgb = [145, 245, 180];
pub const INTERIOR: Rgb = [151, 91, 196];
pub const DIPOLE: Rgb = [232, 151, 55];
pub const PRESSURE: Rgb = [214, 78, 113];
pub const BOUNDARY: Rgb = [119, 145, 169];
pub const DAMPER: Rgb = [64, 176, 166];
pub const TANK: Rgb = [92, 150, 232];
pub const CONTROL: Rgb = [150, 150, 160];
/// A rigid plate in the free surface.
pub const PLATE: Rgb = [196, 160, 52];
pub const CUT_OFF: Rgb = [160, 164, 172];
pub const WATER: Rgb = [86, 156, 214];
pub const DISPLACED: Rgb = [236, 112, 40];
/// The patches cad2bem's checks warn about or fail.
pub const CHECK_WARN: Rgb = [228, 168, 36];
pub const CHECK_FAIL: Rgb = [214, 48, 48];
/// The lines between the spans of a patch mesh's discretization (darker than the patch outlines).
pub const SPAN: Rgb = [24, 58, 102];
pub const NORMAL: Rgb = [200, 40, 40];
const BOTTOM: Rgb = [151, 137, 113];
const FIELD_PLANE: Rgb = [71, 161, 198];
const MARKER: Rgb = [40, 52, 66];

/// Panels outlined up to this count; larger meshes are drawn filled only.
pub const OUTLINE_LIMIT: usize = 25_000;

/// What the viewport shows: everything, one body or lid (as placed), one mesh of the library
/// (in its own coordinates), or nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Focus {
    #[default]
    All,
    Body(usize),
    Lid(usize),
    Mesh(usize),
    Nothing,
}

pub struct SceneInputs<'a> {
    pub focus: Focus,
    /// The engine's panels by role; None when it could not build the problem (the GUI then draws
    /// the surfaces itself).
    pub engine: Option<&'a crate::engine_model::RolePanels>,
    pub overlay: Option<&'a FieldOverlay>,
    pub smoothing: bool,
    /// The submitted field points to mark (View → Show Field Points), when no overlay is shown.
    pub markers: Option<&'a [V3]>,
    /// Values at the body output points, coloured on the hull.
    pub body_overlay: Option<&'a BodyOverlay>,
    /// The free surface z = 0 around the bodies (View → Show Free Surface).
    pub free_surface: bool,
    /// Of a mesh shown alone, an arrow along the normal of each panel or patch (View → Show
    /// Normals).
    pub normals: bool,
    /// Of a patch mesh shown alone, the spans of its discretization (View → Show Spans).
    pub spans: bool,
    /// A generalized mode's displaced hull, drawn with its focused body.
    pub mode_shape: Option<&'a ModeShape>,
    /// A frame of an animated response: the bodies moved, its field or the incident wave drawn
    /// instead of the overlays.
    pub animation: Option<crate::animation::Frame<'a>>,
}

/// The wetted panels of a body displaced by a generalized mode (global, already scaled).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModeShape {
    pub quads: Vec<[V3; 4]>,
}

fn axes(b: &mut Builder) {
    let axis = b.extent().max(1.0) * 0.28;
    Builder::line(&mut b.scene.axes, [0.0; 3], [axis, 0.0, 0.0], [230, 40, 40]);
    Builder::line(&mut b.scene.axes, [0.0; 3], [0.0, axis, 0.0], [65, 210, 90]);
    Builder::line(&mut b.scene.axes, [0.0; 3], [0.0, 0.0, axis], [75, 130, 255]);
}

/// One element alone: a mesh of the library, a body or a lid.
fn build_focused(project: &Project, inputs: &SceneInputs) -> Scene {
    let mut b = Builder { scene: Scene::default(), min: [0.0; 3], max: [0.0; 3], has_points: false };
    match inputs.focus {
        Focus::Mesh(i) => {
            if let Some(entry) = project.meshes.get(i) {
                b.scene.records = entry.mesh.expanded_records(1, entry.mesh.panels.len() as i64);
                b.scene.record_outlines = !entry.mesh.is_patch();
                if let Some(p) = &entry.mesh.patch {
                    // with cad2bem's report, the patches its checks warn about or fail coloured
                    let report = entry.mesh.report.as_deref();
                    let color = |record: usize| match report.and_then(|r| r.patches.get(record)) {
                        Some(x) if x.state == "fail" => (CHECK_FAIL, "Check Failed"),
                        Some(x) if x.state == "warn" => (CHECK_WARN, "Check Warning"),
                        _ => (BODY_COLORS[0], "Body"),
                    };
                    // the symmetry copies follow the tessellation in its order
                    let n = p.tessellation.len().max(1);
                    for (k, s) in entry.mesh.expanded_patches().iter().enumerate() {
                        let (c, name) = color(p.tessellation[k % n].0);
                        b.note(name, c);
                        b.smooth_quad(s, c, 0.82);
                    }
                    // the potential's spans (nu x nv per patch) on the surface; none when the
                    // engine cannot evaluate the patches
                    let spans = if inputs.spans { entry.mesh.span_lines().unwrap_or_default() } else { Vec::new() };
                    if !spans.is_empty() {
                        b.note("Spans (nu x nv)", SPAN);
                    }
                    for [p0, p1] in spans {
                        Builder::line(&mut b.scene.edges, p0, p1, SPAN);
                    }
                } else {
                    let panels = entry.mesh.expanded();
                    b.panels(&crate::project::Body::default(), &panels, BODY_COLORS[0], 0.82);
                }
                if inputs.normals {
                    add_normals(&mut b, &entry.mesh);
                }
            }
        }
        Focus::Body(i) => {
            if let Some(body) = project.bodies.get(i) {
                let color = if body.suppressed { SUPPRESSED } else { BODY_COLORS[i % 4] };
                add_body(&mut b, body, color, engine_of(inputs, i));
                if let Some(shape) = inputs.mode_shape {
                    b.note("Displaced Mode", DISPLACED);
                    for q in &shape.quads {
                        b.quad(*q, DISPLACED, 0.55);
                        for k in 0..4 {
                            Builder::line(&mut b.scene.edges, q[k], q[(k + 1) % 4], lighten(DISPLACED));
                        }
                    }
                }
                if inputs.free_surface {
                    add_free_surface(&mut b);
                }
            }
        }
        Focus::Lid(i) => {
            if let Some(lid) = project.lids.get(i) {
                lid_panels(&mut b, project, lid, LID);
            }
        }
        Focus::All | Focus::Nothing => {}
    }
    b.scene.has_content = b.has_points;
    axes(&mut b);
    b.scene.bounds = (b.min(), b.max());
    b.scene
}

pub fn build_scene(project: &Project, inputs: &SceneInputs) -> Scene {
    if inputs.focus != Focus::All {
        return build_focused(project, inputs);
    }
    let mut b = Builder { scene: Scene::default(), min: [0.0; 3], max: [0.0; 3], has_points: false };
    let mut color = 0;
    for (i, body) in project.bodies.iter().enumerate() {
        if !body.visible || !body.has_geometry() {
            continue;
        }
        let c = if body.suppressed {
            SUPPRESSED
        } else {
            color += 1;
            BODY_COLORS[(color - 1) % 4]
        };
        let marks = b.marks();
        add_body(&mut b, body, c, engine_of(inputs, i));
        if let Some(frame) = inputs.animation
            && let Some((_, motion)) = frame.response.bodies.iter().find(|(k, _)| *k == i)
        {
            b.move_since(marks, &crate::animation::Placement::of(body, motion, frame.phase, frame.amplitude));
        }
    }
    for lid in &project.lids {
        if !lid.visible {
            continue;
        }
        lid_panels(&mut b, project, lid, if lid.suppressed { SUPPRESSED } else { LID });
    }
    for damper in project.fixed_dampers.iter().filter(|d| d.enabled) {
        if let Some(mesh) = &damper.mesh {
            b.note("Dipole Damper", DAMPER);
            for q in mesh.expanded() {
                b.quad(q, DAMPER, 0.6);
                for i in 0..4 {
                    Builder::line(&mut b.scene.edges, q[i], q[(i + 1) % 4], lighten(DAMPER));
                }
            }
        }
    }
    b.scene.has_content = b.has_points;
    if let Some(frame) = inputs.animation {
        // the animated response instead of the overlays and the plane
        let bodies = (b.min(), b.max(), b.has_points);
        let walls = add_boundaries(&mut b, project);
        if let Some(values) = &frame.response.body_values {
            b.scene.color_scale = Some(add_animated_body(&mut b, project, &frame, values));
        }
        match (&frame.response.field, frame.response.incident) {
            (Some(field), _) => {
                b.scene.color_scale = Some(add_animated_field(&mut b, field, frame.phase, frame.amplitude));
            }
            (None, Some((k, heading, a))) if inputs.free_surface => {
                add_incident_surface(&mut b, walls, k, heading, frame.phase, frame.amplitude * a);
            }
            _ if inputs.free_surface && bodies.2 => {
                add_free_surface_over(&mut b, walls.unwrap_or_else(|| around(bodies.0, bodies.1)), None);
            }
            _ => {}
        }
        axes(&mut b);
        b.scene.bounds = (b.min(), b.max());
        return b.scene;
    }
    if let Some(overlay) = inputs.body_overlay.filter(|o| !o.points.is_empty()) {
        b.scene.color_scale = Some(add_body_overlay(&mut b, overlay));
    }
    // the free surface's size in the open sea follows the bodies, not the field-point grid
    let bodies = (b.min(), b.max(), b.has_points);
    let mut grid_plane: Option<Plan> = None;

    // Field points first, so the walls and the sea bed also reach under the grid.
    let grid = &project.field_points;
    match inputs.overlay {
        Some(field)
            if inputs.body_overlay.is_none()
                && !field.points.is_empty()
                && field.values.len() >= field.points.len() =>
        {
            b.scene.color_scale = Some(add_heatmap(&mut b, field, inputs.smoothing));
            if inputs.markers.is_some() {
                add_markers(&mut b, &field.grid, &field.points);
            }
        }
        _ => {
            if grid.enabled && grid.list.is_some() {
                // points one by one: their markers
                if let Some(points) = inputs.markers {
                    for p in points {
                        b.include(*p);
                    }
                    add_markers(&mut b, grid, points);
                }
            } else if grid.enabled && grid.layout != GridLayout::Cartesian {
                add_lattice_plane(&mut b, grid);
                if let Some(points) = inputs.markers {
                    add_markers(&mut b, grid, points);
                }
            } else if grid.enabled && grid.x_count > 0 && grid.y_count > 0 {
                // the part of the grid in the fluid; points beyond the walls are not submitted
                let mut x = [grid.x_start.min(grid.x_end), grid.x_start.max(grid.x_end)];
                let mut y = [grid.y_start.min(grid.y_end), grid.y_start.max(grid.y_end)];
                clip_to_fluid(&mut x, &mut y, project.environment.domain, project.environment.channel_width);
                if grid.z == 0.0 {
                    grid_plane = Some((x, y));
                }
                b.cuboid(
                    [(x[0] + x[1]) / 2.0, (y[0] + y[1]) / 2.0, grid.z],
                    [(x[1] - x[0]).max(0.01), (y[1] - y[0]).max(0.01), 0.003],
                    FIELD_PLANE,
                    0.12,
                );
                b.include([x[0], y[0], grid.z]);
                b.include([x[1], y[1], grid.z]);
                if let Some(points) = inputs.markers {
                    add_markers(&mut b, grid, points);
                }
            }
        }
    }
    let walls = add_boundaries(&mut b, project);
    // between walls the free surface reaches them; the field-point plane at z = 0 replaces it there
    if inputs.free_surface && inputs.overlay.is_none() {
        match walls {
            Some(plan) => add_free_surface_over(&mut b, plan, grid_plane),
            None if bodies.2 => add_free_surface_over(&mut b, around(bodies.0, bodies.1), grid_plane),
            None => {}
        }
    }
    b.scene.has_content |= b.has_points;

    axes(&mut b);
    b.scene.bounds = (b.min(), b.max());
    b.scene
}

#[cfg(test)]
mod tests;
