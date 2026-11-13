//! The project as an engine model and back (docs/ENGINE_API.md): the one path from the GUI's
//! inputs to the engine, for solving, for the queries and for saving. Suppressed and switched-off
//! objects are written with `enabled` false (the engine solves as if they were absent). The GUI
//! state that the problem cannot hold (lids without a mesh, visibility, editor modes, the
//! field-point grid) is JSON in the model's `tool` object `hydrobem_gui`.
//!
//! - `write`: the project written as an engine model (`build_model`) and the record ranges of a
//!   body's surfaces.
//! - `queries`: the engine's answers on the geometry without a solve (hydrostatics, the panels by
//!   role, mode shapes, a mesh alone, the interior free surface).
//! - `reader`: the reader of an engine model's objects and items, body by body.
//! - `read`: the project of an engine model (`read_model`) and its helpers.
//!
//! The GUI state's layout (the `tool` object's JSON) stays here, beside the options.

mod queries;
mod read;
mod reader;
mod write;

pub use queries::{
    DisplacedPanel, MeshInfo, RolePanels, body_hydrostatics, interior_preview, mesh_info, mode_shape, role_panels,
};
pub use read::{axis_of, evenly_spaced, move_inertia, read_model};
pub use write::{build_model, complement, remaining, surface_ranges, wetted_ranges};

use hydrobem_engine::Model;
use serde::{Deserialize, Serialize};

use crate::field_points::FieldPointSelection;
use crate::project::{Body, DampingLid, FieldPoints, PanelSurface, WaveHeadings};

/// The name of the GUI's object in the model's `tool` list.
pub const GUI_TOOL: &str = "hydrobem_gui";
pub const GUI_TOOL_VERSION: u32 = 1;

/// What `build_model` puts into the engine model besides the problem.
#[derive(Clone, Copy, Debug)]
pub struct BuildOptions {
    /// Classify the field-point grid by the engine's rules and submit the points it accepts.
    pub field_points: bool,
    /// The GUI state (the `tool` object).
    pub gui_state: bool,
    /// One frequency (and heading) when the project has none, for queries that apply the rules.
    pub placeholder_frequency: bool,
    /// Only the bodies with a mesh (a query on the geometry while a body has none yet).
    pub meshed_bodies_only: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self { field_points: true, gui_state: true, placeholder_frequency: false, meshed_bodies_only: false }
    }
}

pub struct BuiltModel {
    pub model: Model,
    /// The problem's solved bodies (the active ones), in order: those of the results.
    pub body_names: Vec<String>,
    /// Every body of the model, the disabled ones too, in order: those of the error locations.
    pub model_bodies: Vec<String>,
    /// With `BuildOptions::field_points`.
    pub field_points: FieldPointSelection,
}

// ---------------------------------------------------------------------------------------------
// The GUI state.

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct GuiState {
    version: u32,
    finite_depth: Option<f64>,
    wave_frequencies: Option<WaveGui>,
    /// The heading generator, used while it gives the model's headings.
    headings: Option<WaveHeadings>,
    field_grid: Option<FieldPoints>,
    /// Every body in tree order: the suppressed ones whole, the others for what the problem
    /// cannot hold.
    bodies: Vec<SavedBody>,
    lids: Vec<SavedLid>,
    /// Every fixed damper with its mesh (the switched-off ones too).
    fixed_dampers: Vec<SavedLibrarySurface>,
    /// The library's analytic meshes made in the GUI, by mesh name: their shapes, to edit again.
    analytic_meshes: Vec<(String, crate::analytic::AnalyticSpec)>,
    /// cad2bem's reports of the library's meshes read from geometry files, by mesh name.
    patch_reports: Vec<(String, crate::patch_report::PatchReport)>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct SavedLibrarySurface {
    surface: PanelSurface,
    mesh: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct WaveGui {
    edit_in_table: bool,
    start: f64,
    end: f64,
    count: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct SavedBody {
    body: Body,
    /// The mesh objects the body and its surfaces use (kept while off).
    mesh: Option<String>,
    dipole_meshes: Vec<Option<String>>,
    plate_meshes: Vec<Option<String>>,
    /// Per tank, the meshes of its free-surface plates.
    tank_plate_meshes: Vec<Vec<Option<String>>>,
    pressure_meshes: Vec<Option<String>>,
    damper_meshes: Vec<Option<String>>,
    control_mesh: Option<String>,
    interior_mesh: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct SavedLid {
    lid: DampingLid,
    mesh: Option<String>,
}

#[cfg(test)]
pub(crate) mod tests_support {
    use crate::mesh::Mesh;
    use crate::project::{Project, WaveFrequencies};
    use std::path::Path;
    use std::sync::Arc;

    pub fn cylinder() -> Arc<Mesh> {
        Mesh::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/cylinder.gdf")).expect("cylinder")
    }

    /// The floating cylinder R = 1 m, T = 0.5 m of examples/cylinder (its quadrant mesh) at four
    /// frequencies.
    pub fn cylinder_project() -> Project {
        let mut p = Project::default();
        let i = p.add_body();
        p.bodies[i].mesh = Some(cylinder());
        p.waves.frequencies = WaveFrequencies { start: 0.4, end: 2.0, count: 4, ..WaveFrequencies::default() };
        p.waves.frequencies.rebuild();
        p.update_mesh_properties();
        p
    }
}

#[cfg(test)]
mod tests {
    use super::read::same_values;
    use super::tests_support::cylinder_project;
    use super::*;
    use crate::mesh::Mesh;
    use crate::project::PanelSurface;
    use crate::project::{
        DepthType, Domain, Formulation, InteriorMethod, MassCalculation, Matrix6, Project, SurfaceSource, WaveInputType,
    };
    use hydrobem_engine::PanelRole;
    use std::sync::Arc;

    #[test]
    fn the_cylinder_has_the_engine_hydrostatics() {
        let p = cylinder_project();
        let b = &p.bodies[0];
        // R = 1 m, T = 0.5 m on 64 panels around: the waterplane a regular 64-gon, 32 sin(pi / 32)
        let area = 32.0 * (std::f64::consts::PI / 32.0).sin();
        assert!((b.displaced_volume - 0.5 * area).abs() < 1e-9, "{} {:?}", b.displaced_volume, b.hydrostatics_error);
        assert!((b.mass - p.environment.density * 0.5 * area).abs() < 1e-6);
        assert!((b.waterplane_area - area).abs() < 1e-9);
        assert!((b.center_of_buoyancy[2] + 0.25).abs() < 1e-9);
    }

    #[test]
    fn every_input_round_trips_through_the_model() {
        let mut p = cylinder_project();
        let mesh = p.bodies[0].mesh.clone();
        {
            let b = &mut p.bodies[0];
            b.name = "Hull".into();
            b.position = [1.0, 2.0, 0.0];
            b.yaw = 30.0;
            b.reference = [0.0, 0.0, -0.2];
            b.modes = [true, false, true, false, true, false];
            b.mass_calculation = MassCalculation::Manual;
            b.mass = 3000.0;
            b.center_of_gravity = [0.0, 0.0, -0.3];
            b.inertia = [100.0, 0.0, 0.0, 120.0, 0.0, 140.0];
            let mut stiffness = Matrix6::default();
            stiffness.set(2, 2, 5.0);
            b.additional_stiffness = Some(stiffness);
            let mut damping = Matrix6::default();
            damping.set(0, 0, 7.0);
            damping.edit_in_table = true;
            b.additional_damping = Some(damping);
            b.interior = crate::project::InteriorSurface {
                method: InteriorMethod::PanelRange,
                first_panel: 1,
                last_panel: 9,
                ..Default::default()
            };
            let off = PanelSurface { enabled: false, first_panel: 20, last_panel: 20, ..PanelSurface::default() };
            b.pressure_surfaces = vec![off.clone(), PanelSurface { first_panel: 30, last_panel: 31, ..off.clone() }];
            b.pressure_surfaces[1].enabled = true;
            b.dipoles = vec![PanelSurface {
                enabled: true,
                source: SurfaceSource::PanelRange,
                first_panel: 10,
                last_panel: 12,
                mesh: None,
                damping_factor: 0.0,
                ..PanelSurface::default()
            }];
            b.visible = false;
            b.roll = 0.0;
            b.mesh_range = Some([1, 100]);
        }
        // an unused mesh of the library is kept
        p.add_mesh("spare", tests_support::cylinder());
        let i = p.add_body();
        p.bodies[i].mesh = mesh;
        p.bodies[i].suppressed = true;
        p.bodies[i].interior.method = InteriorMethod::Graded;
        p.environment = crate::project::Environment {
            depth_type: DepthType::Finite,
            depth: 30.0,
            density: 1000.0,
            gravity: 9.81,
            ..Default::default()
        };
        p.environment.domain = Domain::XWall;
        p.first_order.motion = true;
        p.first_order.diffraction_formulation = Formulation::Total;
        p.options.inner_threads = 2;
        p.waves.headings = WaveHeadings { start: 0.0, end: 90.0, count: 3 };
        p.waves.frequencies.set_input_type(WaveInputType::Period);
        p.field_points = FieldPoints {
            enabled: true,
            x_start: 2.0,
            x_end: 6.0,
            x_count: 5,
            y_start: 2.0,
            y_end: 4.0,
            y_count: 3,
            z: 0.0,
            ..FieldPoints::default()
        };

        let built = build_model(&p, &BuildOptions::default()).expect("built");
        assert_eq!(built.body_names, ["Hull"]);
        assert_eq!(built.field_points.points.len(), 15);
        let (q, warnings) = read_model(&built.model).expect("read");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(q.bodies.len(), 2);
        let (a, b) = (&q.bodies[0], &p.bodies[0]);
        assert_eq!(
            (a.name.as_str(), a.position, a.yaw, a.reference, a.modes),
            (b.name.as_str(), b.position, b.yaw, b.reference, b.modes)
        );
        assert_eq!(
            (a.mass_calculation, a.mass, a.center_of_gravity),
            (MassCalculation::Manual, 3000.0, b.center_of_gravity)
        );
        assert_eq!(a.inertia, b.inertia);
        assert_eq!(a.additional_stiffness.as_ref().unwrap().get(2, 2), 5.0);
        assert!(a.additional_damping.as_ref().unwrap().edit_in_table);
        // a switched-off pressure surface is kept beside the problem's
        assert_eq!(a.pressure_surfaces.len(), 2);
        assert!(!a.pressure_surfaces[0].enabled && a.pressure_surfaces[1].first_panel == 30);
        assert_eq!(
            (a.interior.method, a.interior.first_panel, a.interior.last_panel),
            (b.interior.method, b.interior.first_panel, b.interior.last_panel)
        );
        assert!(a.dipoles.len() == 1 && a.dipoles[0].first_panel == 10 && a.dipoles[0].last_panel == 12);
        assert!(!a.visible);
        assert_eq!(a.mesh_range, Some([1, 100]));
        assert_eq!(q.meshes.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["spare", "Hull"]);
        assert!(q.bodies[1].suppressed && q.bodies[1].interior.method == InteriorMethod::Graded);
        let volume = 16.0 * (std::f64::consts::PI / 32.0).sin();
        assert!(q.bodies[1].mesh.is_some() && (q.bodies[1].displaced_volume - volume).abs() < 1e-9);
        assert_eq!(q.environment, p.environment);
        assert_eq!((&q.options, &q.first_order, &q.second_order), (&p.options, &p.first_order, &p.second_order));
        assert_eq!(q.waves.headings, p.waves.headings);
        assert_eq!(q.waves.frequencies.input_type, WaveInputType::Period);
        assert!(same_values(&q.waves.frequencies.inputs, &p.waves.frequencies.inputs));
        assert_eq!(q.field_points, p.field_points);
    }

    #[test]
    fn a_mesh_alone_has_its_geometry() {
        let info = mesh_info(&tests_support::cylinder()).expect("mesh info");
        let (g, q, r) = (&info.geometry, &info.quality, &info.elements);
        let area = 32.0 * (std::f64::consts::PI / 32.0).sin();
        assert!((g.volume - 0.5 * area).abs() < 1e-9, "{}", g.volume);
        assert!((g.waterplane_area - area).abs() < 1e-9);
        assert_eq!(q.size.len(), 192);
        assert_eq!(r.len(), 192);
        // the normals point into the fluid: the bottom's down
        let bottom = (0..r.len()).find(|&k| r.centroids[k][2] < -0.499).expect("a bottom panel");
        assert!((r.normals[bottom][2] + 1.0).abs() < 1e-9);
        // the stored quarter; the geometry counts its four symmetry copies
        assert!((4.0 * r.areas.iter().sum::<f64>() - g.area).abs() < 1e-9);
    }

    #[test]
    fn inertia_about_the_centre_of_gravity_moves_to_r() {
        let mut b =
            Body { mass: 2.0, center_of_gravity: [0.0, 0.0, -1.0], reference: [0.0, 0.0, 0.0], ..Body::default() };
        b.inertia = [1.0, 0.0, 0.0, 1.0, 0.0, 1.0];
        let g = b.center_of_gravity;
        move_inertia(&mut b, g);
        assert_eq!(b.inertia, [3.0, 0.0, 0.0, 3.0, 0.0, 1.0]);
    }

    #[test]
    fn the_wetted_surface_is_what_the_surfaces_leave() {
        assert_eq!(remaining([2, 9], 10, &[[3, 4]]), vec![[2, 2], [5, 9]]);
        assert_eq!(complement(10, &[[3, 4], [9, 12]]), vec![[1, 2], [5, 8]]);
        assert_eq!(complement(3, &[]), vec![[1, 3]]);
        assert!(complement(2, &[[1, 2]]).is_empty());
    }

    #[test]
    fn a_grid_is_rebuilt_from_points() {
        assert_eq!(axis_of(vec![2.0, 0.0, 1.0, 1.0, 0.0]), (0.0, 2.0, 3));
        assert_eq!(axis_of(vec![0.0, 0.5, 2.0]), (0.0, 2.0, 5));
        assert!(evenly_spaced(&[0.0, 1.0, 2.0]) && !evenly_spaced(&[0.0, 1.0, 3.0]));
    }

    #[test]
    fn the_interior_preview_is_the_engine_waterplane() {
        let mut p = cylinder_project();
        p.bodies[0].interior.method = InteriorMethod::Projected;
        let panels = interior_preview(&p.bodies[0], &p.environment).expect("preview");
        assert!(!panels.is_empty());
        assert!(panels.iter().flatten().all(|v| v[2].abs() < 1e-9));
        // the projected surface spans the cylinder in every symmetry copy
        assert!(panels.iter().flatten().any(|v| v[0] < -0.9) && panels.iter().flatten().any(|v| v[0] > 0.9));
    }

    #[test]
    fn a_patch_hull_takes_an_interior_surface_of_its_own_patch_mesh() {
        use crate::analytic::{AnalyticKind, AnalyticSpec};
        let mut hull = AnalyticSpec::of(AnalyticKind::Barge);
        hull.lid = false;
        // the waterplane of the same barge alone, as its own mesh
        let mut m = Model::new();
        let shape = hydrobem_engine::AnalyticShape {
            shape: "barge".into(),
            dimensions: vec![("half_length".into(), 2.0), ("half_beam".into(), 1.0), ("draft".into(), 0.5)],
            parts: vec!["waterplane".into()],
            discretization: vec![[4, 4, 3, 3, 3, 4]],
            reference_length: 1.0,
            symmetry: "xz_and_yz".into(),
            ..Default::default()
        };
        let id = m.analytic_mesh("lid", &shape).expect("waterplane");
        let lid = Arc::new(Mesh::from_object(&m, id).expect("lid"));
        let mut p = Project::default();
        let mut b = Body { mesh: Some(hull.build(None).expect("hull")), ..Body::default() };
        b.interior.method = InteriorMethod::Mesh;
        b.interior.mesh = Some(lid);
        p.bodies.push(b);
        let roles = role_panels(&p).expect("the engine appends the lid to the hull's records");
        // the elements: the hull's patches all wet (element 1 of the lid repeats the hull's
        // element 1, engine D-166) and the lid's patches, each in every copy
        let list = &roles.bodies[&0];
        let count = |role| list.iter().filter(|(r, _)| *r == role).count();
        let hull = p.bodies[0].mesh.as_ref().unwrap();
        assert_eq!(count(PanelRole::InteriorSurface), 4 * p.bodies[0].interior.mesh.as_ref().unwrap().panels.len());
        assert_eq!(count(PanelRole::Wetted), 4 * hull.panels.len());
        // drawn by the engine's tessellation, the lid at the waterline
        let smooth = &roles.smooth[&0];
        let lid: Vec<_> = smooth.iter().filter(|(r, _)| *r == PanelRole::InteriorSurface).collect();
        assert!(lid.len() > count(PanelRole::InteriorSurface), "several quadrilaterals per patch");
        assert!(lid.iter().all(|(_, q)| q.corners.iter().all(|v| v[2].abs() < 1e-9)), "the lid at z = 0");
    }

    #[test]
    fn cad2bems_report_is_saved_with_the_project() {
        use crate::analytic::{AnalyticKind, AnalyticSpec};
        use crate::patch_report::{PatchReport, ReportPatch};
        let mut mesh = (*AnalyticSpec::of(AnalyticKind::Barge).build(None).expect("barge")).clone();
        // read from a geometry file that is gone, with no report beside it
        mesh.source_file =
            std::env::temp_dir().join("hydrobem-gui-gone").join("hull.hbg").to_string_lossy().into_owned();
        let patches: Vec<ReportPatch> = mesh
            .patch_names()
            .into_iter()
            .map(|name| ReportPatch { name, face: 3, state: "warn".into(), ..ReportPatch::default() })
            .collect();
        mesh.report = Some(Arc::new(PatchReport { state: "warn".into(), patches, ..PatchReport::default() }));
        let mut p = Project::default();
        p.add_mesh("hull", Arc::new(mesh));
        let built = build_model(&p, &BuildOptions::default()).expect("built");
        let (q, _) = read_model(&built.model).expect("read");
        assert_eq!(q.meshes[0].mesh.report, p.meshes[0].mesh.report);
    }

    #[test]
    fn ring_points_without_gui_state_open_as_a_list_at_once() {
        // the three cylinders with their body-fitted rings (1424 points), the GUI state removed as a
        // model written by Python has none: the rings are no Cartesian grid; the reader once
        // built one of their smallest spacing (about 2e18 nodes) and never returned
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/three_truncated_cylinders.hbm");
        let mut m = Model::load(&file).expect("load");
        let tool = m.object(&format!("tool/{GUI_TOOL}")).expect("its GUI state");
        m.remove(tool).expect("removed");
        let started = std::time::Instant::now();
        let (p, _) = read_model(&m).expect("read");
        assert!(started.elapsed().as_secs_f64() < 20.0, "{:?}", started.elapsed());
        let list = p.field_points.list.as_ref().expect("a point list");
        assert_eq!(list.len(), 1424);
    }
}
