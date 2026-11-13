//! Bodies, their surfaces by role, the hull a trim cuts off, lids and the normals of a mesh.

use super::*;

fn surface_panels(body: &Body, s: &PanelSurface) -> Vec<[V3; 4]> {
    if !s.enabled {
        return Vec::new();
    }
    match s.source {
        // made by the engine: drawn from its tessellation only
        SurfaceSource::Generated => Vec::new(),
        SurfaceSource::ExternalMesh => s.mesh.as_ref().map(|m| m.expanded()).unwrap_or_default(),
        // stored records, as the engine selects them, in every symmetry copy
        SurfaceSource::PanelRange => body
            .mesh
            .as_ref()
            .map(|m| m.expanded_ranges(s.first_panel, s.last_panel, &s.more_ranges))
            .unwrap_or_default(),
    }
}

/// The colour and opacity of a panel of the engine by its role.
pub fn role_style(role: PanelRole, body: Rgb) -> (Rgb, f64) {
    match role {
        PanelRole::Wetted => (body, 0.82),
        PanelRole::InteriorSurface => (INTERIOR, 0.72),
        PanelRole::Dipole => (DIPOLE, 0.72),
        PanelRole::DipoleDamper | PanelRole::FixedDamper => (DAMPER, 0.6),
        PanelRole::PressureSurface => (PRESSURE, 0.72),
        PanelRole::Lid => (LID, 0.72),
        PanelRole::Tank => (TANK, 0.45),
        PanelRole::ControlSurface | PanelRole::FreeSurface => (CONTROL, 0.12),
        PanelRole::FreeSurfacePlate => (PLATE, 0.8),
    }
}

/// The legend's name of a role.
pub fn role_name(role: PanelRole) -> &'static str {
    match role {
        PanelRole::Wetted => "Body",
        PanelRole::InteriorSurface => "Interior Surface",
        PanelRole::Dipole => "Dipole",
        PanelRole::DipoleDamper | PanelRole::FixedDamper => "Dipole Damper",
        PanelRole::PressureSurface => "Pressure Surface",
        PanelRole::Lid => "Lid",
        PanelRole::Tank => "Tank",
        PanelRole::ControlSurface => "Control Surface",
        PanelRole::FreeSurface => "Free Surface",
        PanelRole::FreeSurfacePlate => "Free Surface Plate",
    }
}

/// Elements of the engine, global, coloured by their role: drawn by the engine's tessellation
/// (`smooth`) when there is one, else flat by their corners.
fn add_role_panels(
    b: &mut Builder,
    panels: &[(PanelRole, [V3; 4])],
    smooth: Option<&Vec<(PanelRole, crate::mesh::SmoothQuad)>>,
    body: Rgb,
) {
    if let Some(smooth) = smooth.filter(|s| !s.is_empty()) {
        for (role, s) in smooth {
            let (color, alpha) = role_style(*role, body);
            b.note(role_name(*role), color);
            b.smooth_quad(s, color, alpha);
        }
        return;
    }
    let outlines = panels.len() <= OUTLINE_LIMIT;
    for (role, q) in panels {
        let (color, alpha) = role_style(*role, body);
        b.note(role_name(*role), color);
        b.quad(*q, color, alpha);
        if outlines {
            for i in 0..4 {
                Builder::line(&mut b.scene.edges, q[i], q[(i + 1) % 4], lighten(color));
            }
        }
    }
}

/// The rotation and elevation of a trim (the engine's rule): x = R(pitch, roll) xi + (0, 0, elevation),
/// pitch about the y axis first, then roll about the x axis.
pub fn trim_point(trim: V3, p: V3) -> V3 {
    let (c2, s2) = (trim[1].to_radians().cos(), trim[1].to_radians().sin());
    let (c3, s3) = (trim[2].to_radians().cos(), trim[2].to_radians().sin());
    let r = [c2, s2 * s3, s2 * c3, 0.0, c3, -s3, -s2, c2 * s3, c2 * c3];
    [
        r[0] * p[0] + r[1] * p[1] + r[2] * p[2],
        r[3] * p[0] + r[4] * p[1] + r[5] * p[2],
        r[6] * p[0] + r[7] * p[1] + r[8] * p[2] + trim[0],
    ]
}

/// The part of a polygon at or above z = 0 (Sutherland-Hodgman against the free surface).
pub(super) fn above_water(polygon: &[V3]) -> Vec<V3> {
    let mut out = Vec::new();
    for (i, a) in polygon.iter().enumerate() {
        let b = polygon[(i + 1) % polygon.len()];
        if a[2] >= 0.0 {
            out.push(*a);
        }
        if (a[2] >= 0.0) != (b[2] >= 0.0) {
            let t = a[2] / (a[2] - b[2]);
            out.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), 0.0]);
        }
    }
    out
}

/// What the trim cuts off a hull: its surface above the free surface, translucent grey.
fn add_cut_off(b: &mut Builder, body: &Body) {
    let (Some(trim), Some(mesh)) = (body.trim, &body.mesh) else { return };
    let panels = match crate::engine_model::wetted_ranges(body) {
        None => mesh.expanded(),
        Some(ranges) => ranges.iter().flat_map(|r| mesh.expanded_range(r[0], r[1])).collect(),
    };
    let outlines = panels.len() <= OUTLINE_LIMIT;
    let mut any = false;
    for q in &panels {
        let placed = q.map(|v| body.to_global(trim_point(trim, v)));
        let part = above_water(&placed);
        if part.len() < 3 {
            continue;
        }
        any = true;
        for k in 1..part.len() - 1 {
            b.triangle(part[0], part[k], part[k + 1], CUT_OFF, 0.22);
        }
        for p in &part {
            b.include(*p);
        }
        if outlines {
            for k in 0..part.len() {
                Builder::line(&mut b.scene.edges, part[k], part[(k + 1) % part.len()], lighten(CUT_OFF));
            }
        }
    }
    if any {
        b.note("Cut Off by Trim", CUT_OFF);
    }
}

pub(super) fn add_body(b: &mut Builder, body: &Body, color: Rgb, engine: Option<EnginePanels<'_>>) {
    add_cut_off(b, body);
    if let Some((panels, smooth)) = engine {
        add_role_panels(b, panels, smooth, color);
        return;
    }
    // The engine could not build the problem: the GUI's own drawing of the surfaces.
    let Some(mesh) = &body.mesh else { return };
    // the wetted surface: the body's range of its mesh less the records its surfaces take
    let wetted = crate::engine_model::wetted_ranges(body);
    let panels = match &wetted {
        None => mesh.expanded(),
        Some(ranges) => ranges.iter().flat_map(|r| mesh.expanded_range(r[0], r[1])).collect(),
    };
    if mesh.is_patch() {
        // a patch mesh: the wetted patches as curved surfaces, the surfaces' patches in their
        // colours below (by their tessellation)
        b.note("Body", color);
        let o = body.to_global([0.0; 3]);
        let rotate = |n: V3| {
            let g = body.to_global(n);
            [g[0] - o[0], g[1] - o[1], g[2] - o[2]]
        };
        let wet = |record: usize| {
            wetted.as_ref().is_none_or(|ranges| ranges.iter().any(|r| (r[0] - 1..r[1]).contains(&(record as i64))))
        };
        for (record, s) in mesh.expanded_patches_with_records() {
            if !wet(record) {
                continue;
            }
            let s = crate::mesh::SmoothQuad {
                corners: s.corners.map(|v| body.to_global(v)),
                normals: s.normals.map(rotate),
                edges: s.edges,
            };
            b.smooth_quad(&s, color, 0.82);
        }
    } else {
        b.panels(body, &panels, color, 0.82);
    }

    if body.interior.method != InteriorMethod::None && !panels.is_empty() {
        // The engine cannot build the problem: a selected range of the mesh, or the plan-view
        // footprint of a generated surface.
        match body.interior.method {
            InteriorMethod::Mesh => {
                if let Some(m) = &body.interior.mesh {
                    b.panels(body, &m.expanded(), INTERIOR, 0.72);
                }
            }
            InteriorMethod::PanelRange => {
                let range = mesh.expanded_ranges(
                    body.interior.first_panel,
                    body.interior.last_panel,
                    &body.interior.more_ranges,
                );
                b.panels(body, &range, INTERIOR, 0.72);
            }
            InteriorMethod::Detect => {
                // the wetted records lying in z = 0 (body axes), as the engine finds them; none
                // drawn when there are none
                let flat: Vec<[V3; 4]> =
                    panels.iter().filter(|q| q.iter().all(|v| v[2].abs() <= 1e-9)).copied().collect();
                b.panels(body, &flat, INTERIOR, 0.72);
            }
            _ => {
                let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
                for v in panels.iter().flatten() {
                    let p = body.to_global(*v);
                    for i in 0..2 {
                        lo[i] = lo[i].min(p[i]);
                        hi[i] = hi[i].max(p[i]);
                    }
                }
                b.cuboid(
                    [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, body.position[2]],
                    [(hi[0] - lo[0]).max(0.01), (hi[1] - lo[1]).max(0.01), 0.004],
                    INTERIOR,
                    0.35,
                );
            }
        }
    }
    for s in &body.dipoles {
        b.panels(body, &surface_panels(body, s), DIPOLE, 0.72);
    }
    for s in &body.pressure_surfaces {
        b.panels(body, &surface_panels(body, s), PRESSURE, 0.72);
    }
    for s in &body.dipole_dampers {
        b.panels(body, &surface_panels(body, s), DAMPER, 0.6);
    }
    for s in &body.free_surface_plates {
        b.panels(body, &surface_panels(body, s), PLATE, 0.8);
    }
    for t in body.tanks.iter().filter(|t| t.enabled) {
        b.panels(body, &mesh.expanded_ranges(t.first_panel, t.last_panel, &t.more_ranges), TANK, 0.45);
        for s in &t.free_surface_plates {
            b.panels(body, &surface_panels(body, s), PLATE, 0.8);
        }
    }
}

/// A lid's panels, global: placed by the position and yaw of its body when it has one.
pub(super) fn lid_panels(b: &mut Builder, project: &Project, lid: &crate::project::DampingLid, color: Rgb) {
    let body = lid.placed_with.as_ref().and_then(|name| project.bodies.iter().find(|x| &x.name == name));
    // a placed lid is records of its body's mesh
    let panels = match (body, &lid.mesh) {
        (Some(x), _) => match &x.mesh {
            Some(m) => m.expanded_ranges(lid.range[0], lid.range[1], &lid.more_ranges),
            None => return,
        },
        (None, Some(m)) => m.expanded(),
        (None, None) => return,
    };
    b.note("Lid", LID);
    let place = |p: V3| -> V3 {
        match body {
            Some(x) => {
                let (s, c) = x.yaw.to_radians().sin_cos();
                [c * p[0] - s * p[1] + x.position[0], s * p[0] + c * p[1] + x.position[1], p[2] + x.position[2]]
            }
            None => p,
        }
    };
    for q in panels {
        let q = q.map(place);
        b.quad(q, color, 0.72);
        for i in 0..4 {
            Builder::line(&mut b.scene.edges, q[i], q[(i + 1) % 4], LID_EDGE);
        }
    }
}

/// The engine's panels of body `i` and, of a patch mesh, their smooth shading.
type EnginePanels<'a> = (&'a Vec<(PanelRole, [V3; 4])>, Option<&'a Vec<(PanelRole, crate::mesh::SmoothQuad)>>);

pub(super) fn engine_of<'a>(inputs: &SceneInputs<'a>, i: usize) -> Option<EnginePanels<'a>> {
    let e = inputs.engine?;
    Some((e.bodies.get(&i)?, e.smooth.get(&i)))
}

/// An arrow along the normal of each stored panel or patch of `mesh` (every symmetry copy), from
/// its centre, about half its size long: of a panel the normal of its vertex order, of a patch the
/// engine's surface normal at its middle; the direction the engine takes into the fluid.
pub(super) fn add_normals(b: &mut Builder, mesh: &crate::mesh::Mesh) {
    let unit = |n: V3| {
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        (l > 1e-12).then(|| n.map(|x| x / l))
    };
    let centre = |q: &[V3; 4]| [0, 1, 2].map(|i| q.iter().map(|v| v[i]).sum::<f64>() / 4.0);
    // |d1 x d2| / 2: the area of a plane quadrilateral
    let diagonals = |q: &[V3; 4]| cross([0, 1, 2].map(|i| q[2][i] - q[0][i]), [0, 1, 2].map(|i| q[3][i] - q[1][i]));
    let area = |q: &[V3; 4]| {
        let n = diagonals(q);
        (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() / 2.0
    };
    // per panel or patch: centre, unit normal, area
    let mut arrows: Vec<(V3, V3, f64)> = Vec::new();
    if mesh.is_patch() {
        // the tessellation of a patch is a run of its record, n x n quads; the symmetry copies
        // follow the originals, each a run of the same record: runs are taken copy by copy (one
        // patch's copies would make one run), and a run's middle quad is (n/2, n/2)
        let all = mesh.expanded_patches_with_records();
        let per_copy = mesh.patch.as_ref().map_or(0, |p| p.tessellation.len()).max(1);
        let mut k0 = 0;
        while k0 < all.len() {
            let end = (k0 / per_copy + 1) * per_copy;
            let mut k1 = k0;
            while k1 < all.len().min(end) && all[k1].0 == all[k0].0 {
                k1 += 1;
            }
            let len = k1 - k0;
            let n = len.isqrt();
            let mid = if n * n == len { k0 + (n / 2) * n + n / 2 } else { (k0 + k1) / 2 };
            let mid = &all[mid].1;
            let n = [0, 1, 2].map(|i| mid.normals.iter().map(|v| v[i]).sum::<f64>());
            if let Some(n) = unit(n) {
                arrows.push((centre(&mid.corners), n, all[k0..k1].iter().map(|(_, s)| area(&s.corners)).sum()));
            }
            k0 = k1;
        }
    } else {
        for q in mesh.expanded() {
            if let Some(n) = unit(diagonals(&q)) {
                arrows.push((centre(&q), n, area(&q)));
            }
        }
    }
    if arrows.is_empty() {
        return;
    }
    b.note("Normals", NORMAL);
    let extent = b.extent().max(1e-6);
    // at most 20000 arrows: every k-th
    let step = arrows.len().div_ceil(20_000).max(1);
    for (c, n, a) in arrows.into_iter().step_by(step) {
        let size = (0.6 * a.sqrt()).clamp(0.004 * extent, 0.06 * extent);
        let tip = [0, 1, 2].map(|i| c[i] + n[i] * size);
        Builder::line(&mut b.scene.edges, c, tip, NORMAL);
        // the head: two strokes back from the tip, across the normal
        let t0 = if n[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
        let Some(u) = unit(cross(n, t0)) else { continue };
        for side in [-1.0, 1.0] {
            let back = [0, 1, 2].map(|i| tip[i] - n[i] * size * 0.3 + side * u[i] * size * 0.15);
            Builder::line(&mut b.scene.edges, tip, back, NORMAL);
        }
    }
}
