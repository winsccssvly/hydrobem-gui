//! The details of the environment, options, first and second order, waves and field points.

use eframe::egui::Ui;
use gui_core::field_layout::{GridLayout, Profile};
use gui_core::format::format_bytes;
use gui_core::project::{DepthType, Domain, Formulation, FreeSurfaceMethod, NewmanAverage, Project, WaveInputType};

use super::{Derived, DetailsAction, MeshTarget, library, mesh_rows};
use crate::widgets::{
    check, check_enabled, choice, choice_of, integer, message, number, read_only, row, section, warning,
};

/// `environment`: the water depth, density and gravity, the walls of the domain.
pub(super) fn environment(ui: &mut Ui, project: &mut Project) {
    let (density_before, gravity_before) = (project.environment.density, project.environment.gravity);
    let e = &mut project.environment;
    section(ui, "Environment", |ui| {
        let options = ["Infinite".to_string(), "Finite".to_string()];
        let mut index = usize::from(e.depth_type == DepthType::Finite);
        if choice(ui, "environment", "Depth Type", &options, &mut index) {
            e.depth_type = if index == 1 { DepthType::Finite } else { DepthType::Infinite };
        }
        if e.depth_type == DepthType::Finite {
            number(ui, "environment", "Depth", "m", &mut e.depth, true);
        }
        number(ui, "environment", "Density", "kg/m³", &mut e.density, true);
        number(ui, "environment", "Gravity", "m/s²", &mut e.gravity, true);
        choice_of(ui, "environment", "Domain", &Domain::ALL, Domain::label, &mut e.domain);
        if e.domain == Domain::Channel {
            number(ui, "environment", "Channel Width", "m", &mut e.channel_width, true);
            number(ui, "environment", "Channel Reflect", "", &mut e.channel_reflect, true);
        }
    });
    if project.environment.density != density_before || project.environment.gravity != gravity_before {
        project.update_mesh_properties();
    }
}

/// `options`: how the engine solves (formulation, the logarithmic integral, threads).
pub(super) fn options(ui: &mut Ui, project: &mut Project, derived: &Derived) {
    let threads = project.options.frequency_threads.clamp(1, 1024);
    let a = &mut project.options;
    section(ui, "Options", |ui| {
        let b = gui_core::hydrobem_engine::build_info();
        read_only(
            ui,
            "Core",
            &format!("hydrobem_engine {} · API {}", gui_core::engine_info::engine_label(), b.api_version),
        );
        check(ui, "Logarithmic Integration", &mut a.logarithmic_integration, None);
        let log = a.logarithmic_integration;
        number(ui, "options", "Log Tolerance", "", &mut a.log_tolerance, log);
        integer(ui, "options", "Frequency Threads", &mut a.frequency_threads, true);
        integer(ui, "options", "Inner Threads", &mut a.inner_threads, true);
        let memory = match &derived.memory {
            Some(Ok(e)) => format!(
                "{} (shared {} + {} × {} per frequency + outputs {}); {} free",
                format_bytes(e.peak as f64),
                format_bytes(e.statics as f64),
                threads,
                format_bytes(e.per_frequency as f64),
                format_bytes(e.outputs as f64),
                format_bytes(crate::system::available_memory() as f64)
            ),
            Some(Err(e)) => format!("not available: {e}"),
            None => "…".into(),
        };
        read_only(ui, "Estimated Memory", &memory);
    });
}

/// An item whose value may be the engine's `"auto"` (None): an Auto check and the number.
fn auto_number(ui: &mut Ui, label: &str, unit: &str, value: &mut Option<f64>, start: f64, tooltip: &str) {
    let mut auto = value.is_none();
    if check(ui, &format!("{label}: Auto"), &mut auto, Some(tooltip)) {
        *value = if auto { None } else { Some(start) };
    }
    let mut v = value.unwrap_or(start);
    if number(ui, ("auto", label), label, unit, &mut v, !auto) && v > 0.0 {
        *value = Some(v);
    }
}

/// As `auto_number`, an integer of `range`.
fn auto_integer(ui: &mut Ui, label: &str, value: &mut Option<i64>, start: i64, range: (i64, i64), tooltip: &str) {
    let mut auto = value.is_none();
    if check(ui, &format!("{label}: Auto"), &mut auto, Some(tooltip)) {
        *value = if auto { None } else { Some(start) };
    }
    let mut v = value.unwrap_or(start);
    if integer(ui, ("auto", label), label, &mut v, !auto) {
        *value = Some(v.clamp(range.0, range.1));
    }
}

/// The engine's `first_order` and `second_order`: what the solve computes.
pub(super) fn first_order(ui: &mut Ui, project: &mut Project) {
    let a = &mut project.first_order;
    section(ui, "First Order", |ui| {
        check(ui, "Radiation", &mut a.radiation, Some("Added mass and damping: the radiation problems of the modes."));
        check(ui, "Diffraction", &mut a.diffraction, Some("Wave loads: the diffraction problem of every heading."));
        choice_of(
            ui,
            "first_order",
            "Diffraction Formulation",
            &Formulation::ALL,
            Formulation::label,
            &mut a.diffraction_formulation,
        );
        check(
            ui,
            "Motion",
            &mut a.motion,
            Some("Needs radiation, diffraction, mass, centre of gravity and inertia of every body."),
        );
        let field = project.field_points.enabled;
        check_enabled(
            ui,
            "Field Pressure",
            &mut a.field_pressure,
            field,
            "At the field points (First Order → Field Points): the free-surface elevation at z = 0, the pressure p/ρg below it.",
        );
        check_enabled(ui, "Field Velocity", &mut a.field_velocity, field, "The fluid velocity at the field points.");
        if field && !a.field_pressure && !a.field_velocity {
            message(ui, "The field points compute nothing: turn on Field Pressure or Field Velocity.");
        }
        check(
            ui,
            "Body Pressure",
            &mut a.body_pressure,
            Some("p / (rho g) on the bodies: at the panels, or at the body points."),
        );
        check(ui, "Body Velocity", &mut a.body_velocity, Some("The fluid velocity on the bodies, at the same points."));
    });
}

/// The engine's `second_order`: the mean drifts, the QTFs and the potential QTF's free surface.
pub(super) fn second_order(ui: &mut Ui, project: &mut Project, actions: &mut Vec<DetailsAction>) {
    let library = library(project);
    let a = &mut project.second_order;
    section(ui, "Drift", |ui| {
        check(
            ui,
            "Pressure Integration",
            &mut a.drift_pressure_integration,
            Some("By pressure integration over each body (body axes)."),
        );
        check(
            ui,
            "Control Surface",
            &mut a.drift_control_surface,
            Some("By the momentum flux through each body's control surface (add one to the body)."),
        );
        check(
            ui,
            "Momentum Conservation",
            &mut a.drift_momentum_conservation,
            Some("Surge, sway and yaw of all bodies in global axes, from the Kochin functions."),
        );
        let mut auto = a.drift_momentum_conservation_angles.is_none();
        let on = a.drift_momentum_conservation;
        if check(
            ui,
            "Momentum Conservation Angles: Auto",
            &mut auto,
            Some("The engine chooses the number of angles from the wave number and the bodies."),
        ) {
            a.drift_momentum_conservation_angles = if auto { None } else { Some(360) };
        }
        let mut n = a.drift_momentum_conservation_angles.unwrap_or(360);
        if integer(ui, "outputs", "Momentum Conservation Angles", &mut n, on && !auto) {
            a.drift_momentum_conservation_angles = Some(n.clamp(1, 65536));
        }
        check(
            ui,
            "Bidirectional",
            &mut a.drift_bidirectional,
            Some("Every pair of two headings of every drift, not only each heading with itself."),
        );
    });
    section(ui, "QTF", |ui| {
        message(
            ui,
            "The second-order force and moment at the difference and sum frequencies of every pair of the frequencies. The total is a quadratic part plus a potential part.",
        );
        check(
            ui,
            "Quadratic Pressure Integration",
            &mut a.qtf_quadratic_pressure_integration,
            Some("The quadratic part by pressure integration over each body (where that drift is available)."),
        );
        check(
            ui,
            "Quadratic Control Surface",
            &mut a.qtf_quadratic_control_surface,
            Some("The quadratic part over each body's control surface."),
        );
        check(
            ui,
            "Potential Direct",
            &mut a.qtf_potential_direct,
            Some(
                "The force of the second-order potential, solved (low order; open sea or one wall; floating bodies need Motion).",
            ),
        );
        check(
            ui,
            "Potential Indirect",
            &mut a.qtf_potential_indirect,
            Some("The force of the second-order potential by the indirect method (low order, as Direct)."),
        );
        check(
            ui,
            "Newman",
            &mut a.qtf_newman,
            Some("The difference-frequency QTF from the mean drifts switched on, at the equal headings."),
        );
        choice_of(ui, "qtf", "Newman Average", &NewmanAverage::ALL, NewmanAverage::label, &mut a.qtf_newman_average);
        check(ui, "Difference", &mut a.qtf_difference, Some("ω_a − ω_b: the slow drift."));
        check(ui, "Sum", &mut a.qtf_sum, Some("ω_a + ω_b (off: no radiation problems at the sums)."));
        check(
            ui,
            "Bidirectional",
            &mut a.qtf_bidirectional,
            Some("Every pair of two headings of the QTFs, not only each heading with itself."),
        );
        if a.any_qtf() && !a.qtf_difference && !a.qtf_sum {
            message(ui, "Turn on the difference or the sum frequencies: the engine refuses neither.");
        }
        if a.qtf_newman && !(a.drift_pressure_integration || a.drift_control_surface || a.drift_momentum_conservation) {
            message(ui, "Newman's approximation is made from the mean drifts: turn one on.");
        }
    });
    if !a.potential_qtf() {
        return;
    }
    section(ui, "Free Surface (Potential QTF)", |ui| {
        choice_of(
            ui,
            "qtf",
            "Free Surface Method",
            &FreeSurfaceMethod::ALL,
            FreeSurfaceMethod::label,
            &mut a.free_surface_method,
        );
        match a.free_surface_method {
            FreeSurfaceMethod::Panels | FreeSurfaceMethod::Cells => {
                auto_number(
                    ui,
                    "Free Surface Panel Size",
                    "m",
                    &mut a.free_surface_panel_size,
                    1.0,
                    "Auto: a third of the shortest combined wavelength, at most a/4.",
                );
                auto_number(
                    ui,
                    "Free Surface Inner Radius",
                    "m",
                    &mut a.free_surface_inner_radius,
                    10.0,
                    "Auto: 4 a, a the largest distance from the centre of the bodies to a waterline.",
                );
            }
            FreeSurfaceMethod::Mesh => {
                message(ui, "A mesh of panels in z = 0, global coordinates, from the waterlines outwards.");
                mesh_rows(ui, &library, &mut a.free_surface_mesh, MeshTarget::FreeSurface, actions);
                let mut n = a.free_surface_quadrature_points;
                if integer(ui, "qtf", "Free Surface Quadrature Points", &mut n, true) {
                    a.free_surface_quadrature_points = n.clamp(1, 8);
                }
            }
        }
        let mut n = a.quadrature_annuli;
        if integer(ui, "qtf", "Quadrature Annuli", &mut n, true) {
            a.quadrature_annuli = n.max(0);
        }
        auto_number(
            ui,
            "Quadrature Radius Step",
            "m",
            &mut a.quadrature_radius_step,
            10.0,
            "Auto: two of the shortest wavelengths.",
        );
        let mut n = a.quadrature_radial_nodes;
        if integer(ui, "qtf", "Quadrature Radial Nodes", &mut n, true) {
            a.quadrature_radial_nodes = n.max(1);
        }
        auto_integer(
            ui,
            "Quadrature Azimuthal Nodes",
            &mut a.quadrature_azimuthal_nodes,
            64,
            (1, 1_000_000),
            "Auto: from the outer radius and the shortest combined wavelength.",
        );
        auto_integer(
            ui,
            "Outer Circle Segments",
            &mut a.outer_circle_segments,
            64,
            (1, 1_000_000),
            "Auto: the points of the far-field fit.",
        );
        auto_integer(ui, "Expansion Order", &mut a.expansion_order, 30, (1, 160), "Auto: ⌈kR⌉ + 15, at most 160.");
    });
}

pub(super) fn frequencies(ui: &mut Ui, project: &mut Project) {
    let f = &mut project.waves.frequencies;
    section(ui, "Frequencies", |ui| {
        let options = ["Omegas".to_string(), "Periods".to_string()];
        let mut index = usize::from(f.input_type == WaveInputType::Period);
        if choice(ui, "frequencies", "Frequency Input", &options, &mut index) {
            f.set_input_type(if index == 1 { WaveInputType::Period } else { WaveInputType::Frequency });
        }
        check(ui, "Edit in Table", &mut f.edit_in_table, None);
        let table = f.edit_in_table;
        let unit = if f.input_type == WaveInputType::Frequency { "rad/s" } else { "s" };
        let mut rebuild = number(ui, "frequencies", "Start", unit, &mut f.start, !table);
        rebuild |= number(ui, "frequencies", "End", unit, &mut f.end, !table);
        if integer(ui, "frequencies", "Count", &mut f.count, true) {
            if table {
                f.resize();
            } else {
                rebuild = true;
            }
        }
        if rebuild || (!table && f.inputs.len() != f.count.max(0) as usize) {
            f.rebuild();
        }
    });
    section(ui, "Added Mass Limits", |ui| {
        check(
            ui,
            "Zero Frequency (ω = 0)",
            &mut f.zero_limit,
            Some("The added mass at omega = 0, solved besides the wave frequencies."),
        );
        check(
            ui,
            "Infinite Frequency (ω = ∞)",
            &mut f.infinite_limit,
            Some("The added mass at omega = inf, solved besides the wave frequencies."),
        );
    });
}

pub(super) fn field_points(ui: &mut Ui, project: &mut Project, derived: &Derived) {
    // every tank of the bodies, by its body and place
    let tanks: Vec<(gui_core::project::TankRef, String)> = project
        .bodies
        .iter()
        .flat_map(|b| {
            (0..b.tanks.len())
                .map(|k| (gui_core::project::TankRef { body: b.name.clone(), tank: k + 1 }, format!("Tank {}", k + 1)))
        })
        .collect();
    // Grid, Polar Grid, Body-Fitted Grid, then Point List
    let layouts = GridLayout::ALL;
    let bodies: Vec<String> = project.bodies.iter().map(|b| b.name.clone()).collect();
    let mut input = if project.field_points.list.is_some() {
        layouts.len()
    } else {
        layouts.iter().position(|l| *l == project.field_points.layout).unwrap_or(0)
    };
    let mut switched = false;
    section(ui, "Field Points", |ui| {
        check(ui, "Enabled", &mut project.field_points.enabled, None);
        let mut options: Vec<String> = layouts.iter().map(|l| l.label().to_string()).collect();
        options.push("Point List".to_string());
        switched = choice(ui, "grid", "Input", &options, &mut input);
    });
    if switched && input < layouts.len() {
        let layout = layouts[input];
        let first_time = project.field_points.layout != layout;
        project.field_points.layout = layout;
        // rings about the body when first chosen; about the convex hull of a section in parts
        if first_time && layout != GridLayout::Cartesian {
            if layout == GridLayout::BodyFitted {
                let z = project.field_points.z;
                let split = project.field_body().is_some_and(|b| Profile::is_split(b, z));
                let g = &mut project.field_points;
                g.convex = split;
                // the water between the columns, inside the outline, by inside rings
                if split && g.inside_rings == 0 {
                    g.inside_rings = AUTO_INSIDE_RINGS;
                }
            }
            fit_rings(project);
        }
    }
    let split = project.field_points.layout == GridLayout::BodyFitted
        && project.field_body().is_some_and(|b| Profile::is_split(b, project.field_points.z));
    let g = &mut project.field_points;
    if switched {
        // the list starts as the grid's points (each in the grid's tank); back to a grid drops it
        g.list = (input == layouts.len()).then(|| {
            let was = std::mem::replace(&mut g.enabled, true);
            let list = g
                .generate()
                .into_iter()
                .map(|point| gui_core::project::ListedPoint { point, tank: g.tank.clone() })
                .collect();
            g.enabled = was;
            list
        });
    }
    if let Some(list) = &mut g.list {
        section(ui, "Point List", |ui| {
            message(
                ui,
                "Each point by itself, global coordinates, with the tank it lies in: edit them in the table (Tank 0 the exterior, k the k-th tank of the bodies in order).",
            );
            let mut n = list.len() as i64;
            if integer(ui, "list", "Number of Points", &mut n, true) {
                let last = list.last().cloned().unwrap_or_default();
                list.resize(n.clamp(0, 100_000) as usize, last);
            }
            let s = &derived.field_points;
            read_only(ui, "Excluded Inside Bodies", &s.inside_bodies.to_string());
            read_only(ui, "Excluded by the Engine", &s.rejected.to_string());
            read_only(ui, "Submitted Points", &s.points.len().to_string());
        });
        return;
    }
    let mut fit = false;
    section(ui, g.layout.label(), |ui| {
        match g.layout {
            GridLayout::Cartesian => {
                number(ui, "grid", "X Start", "m", &mut g.x_start, true);
                number(ui, "grid", "X End", "m", &mut g.x_end, true);
                integer(ui, "grid", "X Count", &mut g.x_count, true);
                number(ui, "grid", "Y Start", "m", &mut g.y_start, true);
                number(ui, "grid", "Y End", "m", &mut g.y_end, true);
                integer(ui, "grid", "Y Count", &mut g.y_count, true);
            }
            layout => {
                let fitted = layout == GridLayout::BodyFitted;
                let mut index = bodies.iter().position(|b| *b == g.fitted_body).unwrap_or(0);
                if !bodies.is_empty() && choice(ui, "grid", "Body", &bodies, &mut index) {
                    g.fitted_body = bodies[index].clone();
                }
                if fitted {
                    message(ui, "Rings at a distance outside the body's section at Z, by angle about its centre.");
                    check(
                        ui,
                        "Convex Outline",
                        &mut g.convex,
                        Some("Rings about the convex hull of the section: one outline about several columns."),
                    );
                    if g.profile.is_none() {
                        message(ui, "The body has no section at Z: no points.");
                    } else if !g.convex && split {
                        warning(
                            ui,
                            "The section at Z is in separate parts (several columns): followed as one star its rings cross the water between them. Convex Outline rings them as one, Inside Rings fill the water between.",
                        );
                    }
                    let mut inside = g.inside_rings;
                    if integer(ui, "grid", "Inside Rings", &mut inside, true) {
                        g.inside_rings = inside.clamp(0, 1000);
                    }
                    number(ui, "grid", "First Ring Offset", "m", &mut g.radial_start, true);
                    number(ui, "grid", "Last Ring Offset", "m", &mut g.radial_end, true);
                } else {
                    number(ui, "grid", "Centre X", "m", &mut g.centre[0], true);
                    number(ui, "grid", "Centre Y", "m", &mut g.centre[1], true);
                    number(ui, "grid", "First Radius", "m", &mut g.radial_start, true);
                    number(ui, "grid", "Last Radius", "m", &mut g.radial_end, true);
                }
                integer(ui, "grid", "Ring Count", &mut g.radial_count, true);
                number(ui, "grid", "Angle Start", "deg", &mut g.angle_start, true);
                number(ui, "grid", "Angle End", "deg", &mut g.angle_end, true);
                integer(ui, "grid", "Angle Count", &mut g.angle_count, true);
                if g.periodic() {
                    message(ui, "A whole turn: the last angle is the first one, not repeated.");
                }
                fit = row(
                    ui,
                    "Fit to Body",
                    Some("Rings from a tenth of the body's largest radius outside its section to twice that radius."),
                    |ui| ui.button("Fit").clicked(),
                );
            }
        }
        number(ui, "grid", "Z", "m", &mut g.z, true);
        let mut n = g.extra_levels.len() as i64;
        if integer(ui, "grid", "Further Depths", &mut n, true) {
            let fill = g.extra_levels.last().copied().unwrap_or(g.z) - 1.0;
            g.extra_levels.resize(n.clamp(0, 32) as usize, fill);
        }
        for (k, z) in g.extra_levels.iter_mut().enumerate() {
            number(ui, "grid", &format!("Z {}", k + 2), "m", z, true);
        }
        let mut options = vec!["Exterior".to_string()];
        options.extend(tanks.iter().map(|(t, label)| format!("{label} ({})", t.body)));
        let mut index = g.tank.as_ref().and_then(|t| tanks.iter().position(|(x, _)| x == t)).map_or(0, |k| k + 1);
        if choice(ui, "grid", "In Tank", &options, &mut index) {
            g.tank = index.checked_sub(1).map(|k| tanks[k].0.clone());
        }
        let s = &derived.field_points;
        read_only(ui, "Generated Points", &s.generated.to_string());
        read_only(ui, "Excluded Inside Bodies", &s.inside_bodies.to_string());
        match &s.engine_error {
            None => read_only(ui, "Excluded by the Engine", &s.rejected.to_string()),
            Some(e) => read_only(ui, "Excluded by the Engine", &format!("not checked: {e}")),
        }
        read_only(ui, "Submitted Points", &s.points.len().to_string());
        if s.near_bodies > 0 {
            row(ui, "Near a Body (warning)", Some("closer to a panel than its size: less accurate"), |ui| {
                ui.label(s.near_bodies.to_string());
            });
        }
    });
    if fit {
        fit_rings(project);
    }
}

/// Polar or body-fitted rings sized to the field body's section at Z (the counts kept): Polar
/// about its centre from 1.1 to 3 times its largest radius, Body-Fitted from a tenth of that
/// radius outside it to twice the radius.
/// The inside rings a Body-Fitted Grid takes when it rings a section in parts (Convex Outline on by
/// itself): without them the water between the columns has no points.
const AUTO_INSIDE_RINGS: i64 = 10;

fn fit_rings(project: &mut Project) {
    if project.field_points.layout == GridLayout::BodyFitted {
        project.update_field_profile();
        if let Some(r) = project.field_points.profile.as_ref().map(|p| p.max_radius()) {
            let g = &mut project.field_points;
            g.radial_start = 0.1 * r;
            g.radial_end = 2.0 * r;
        }
    } else {
        project.fit_polar_grid();
    }
}
