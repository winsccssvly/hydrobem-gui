//! The project tree (REQUIREMENTS.md, Project tree), HydroBEM's input tree: the mesh library,
//! the bodies with what they own, the lids, the fixed dampers, the environment (with its
//! frequencies and headings), the first and second order (the field points under the first) and
//! the options; then the results as the engine's result tree (its global paths; a body is
//! chosen in Details). The labels are the engine's names in title case.

use eframe::egui::{self, Color32, FontId, Stroke, Ui, vec2};
use gui_core::hydrobem_engine::QuantityId;
use gui_core::project::Project;
use gui_core::results::{FieldKind, SolveResult};

use crate::style;
use crate::widgets::tree_row;

/// A child of a body, added by right-clicking the body: the matrices once each, the dipoles and
/// pressure surfaces any number of times (numbered in their order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BodyPart {
    Damping,
    Stiffness,
    Dipole(usize),
    Damper(usize),
    Tank(usize),
    Pressure(usize),
    /// A free-surface plate (`free_surface_plates`).
    Plate(usize),
    ControlSurface,
    BodyPoints,
    Generalized(usize),
    ModeMatrices,
    Coupling(usize),
}

impl BodyPart {
    pub fn label(self) -> String {
        match self {
            BodyPart::Damping => "Additional Damping".into(),
            BodyPart::Stiffness => "Additional Stiffness".into(),
            BodyPart::Dipole(k) => format!("Dipole {}", k + 1),
            BodyPart::Damper(k) => format!("Dipole Damper {}", k + 1),
            BodyPart::Tank(k) => format!("Tank {}", k + 1),
            BodyPart::Pressure(k) => format!("Pressure Surface {}", k + 1),
            BodyPart::Plate(k) => format!("Free Surface Plate {}", k + 1),
            BodyPart::ControlSurface => "Control Surface".into(),
            BodyPart::BodyPoints => "Body Points".into(),
            BodyPart::Generalized(k) => format!("Generalized Mode {}", k + 1),
            BodyPart::ModeMatrices => "Mode Matrices".into(),
            BodyPart::Coupling(k) => format!("Coupling {}", k + 1),
        }
    }

    /// The part exists in the body.
    pub fn exists(self, body: &gui_core::project::Body) -> bool {
        match self {
            BodyPart::Damping => body.additional_damping.is_some(),
            BodyPart::Stiffness => body.additional_stiffness.is_some(),
            BodyPart::Dipole(k) => k < body.dipoles.len(),
            BodyPart::Damper(k) => k < body.dipole_dampers.len(),
            BodyPart::Tank(k) => k < body.tanks.len(),
            BodyPart::Pressure(k) => k < body.pressure_surfaces.len(),
            BodyPart::Plate(k) => k < body.free_surface_plates.len(),
            BodyPart::ControlSurface => body.control_surface.is_some(),
            BodyPart::BodyPoints => body.body_points.is_some(),
            BodyPart::Generalized(k) => k < body.generalized_modes.len(),
            BodyPart::ModeMatrices => body.mode_matrices.is_some(),
            BodyPart::Coupling(k) => k < body.couplings.len(),
        }
    }

    /// The parts of a body, in tree order.
    pub fn of(body: &gui_core::project::Body) -> Vec<BodyPart> {
        // the engine's order: the body's items, then its lists
        let mut parts = Vec::new();
        if body.additional_stiffness.is_some() {
            parts.push(BodyPart::Stiffness);
        }
        if body.additional_damping.is_some() {
            parts.push(BodyPart::Damping);
        }
        if body.mode_matrices.is_some() {
            parts.push(BodyPart::ModeMatrices);
        }
        if body.body_points.is_some() {
            parts.push(BodyPart::BodyPoints);
        }
        if body.control_surface.is_some() {
            parts.push(BodyPart::ControlSurface);
        }
        parts.extend((0..body.dipoles.len()).map(BodyPart::Dipole));
        parts.extend((0..body.dipole_dampers.len()).map(BodyPart::Damper));
        parts.extend((0..body.tanks.len()).map(BodyPart::Tank));
        parts.extend((0..body.pressure_surfaces.len()).map(BodyPart::Pressure));
        parts.extend((0..body.free_surface_plates.len()).map(BodyPart::Plate));
        parts.extend((0..body.generalized_modes.len()).map(BodyPart::Generalized));
        parts.extend((0..body.couplings.len()).map(BodyPart::Coupling));
        parts
    }
}

/// What a body's context menu adds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewPart {
    Damping,
    Stiffness,
    Dipole,
    Damper,
    Tank,
    Pressure,
    Plate,
    ControlSurface,
    BodyPoints,
    Generalized,
    ModeMatrices,
    Coupling,
}

/// A node of the result tree: the engine's global result paths (a group, or what Details shows
/// of a quantity), the GUI's Total of the field points beside them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResultNode {
    Summary,
    AddedMass,
    Damping,
    PressureModeDamping,
    /// The group `excitation`.
    ExcitationGroup,
    /// `excitation.diffraction`.
    Excitation,
    FroudeKrylov,
    Scattering,
    /// `excitation.haskind`: the whole exciting force by the Haskind relation.
    Haskind,
    Motion,
    /// The group `body_points`.
    BodyPoints,
    BodyPressure,
    /// The fluid velocity on the bodies.
    BodyVelocity,
    /// The group `field_points`.
    FieldPoints,
    Field(FieldKind),
    /// The fluid velocity at the field points.
    FieldVelocity,
    /// The group `lids`.
    Lids,
    LidElevation,
    /// The group `pressure_surfaces` (of the bodies).
    PressureSurfaces,
    SurfaceElevation,
    Kochin,
    Drift,
    /// The quadratic transfer functions (second-order forces of pairs of frequencies).
    Qtf,
    Hydrostatics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Node {
    Environment,
    /// `environment/omegas` or `periods` (by `frequency_input`).
    Frequencies,
    /// `environment/headings`.
    Headings,
    Options,
    FirstOrder,
    /// `first_order/field_points` (and the points of the tanks).
    FieldPoints,
    SecondOrder,
    Bodies,
    Body(usize),
    BodyPart(usize, BodyPart),
    Lids,
    Lid(usize),
    FixedDampers,
    FixedDamper(usize),
    Meshes,
    Mesh(usize),
    Result(ResultNode),
}

impl Node {
    /// The label of its tree row.
    pub fn label(self, project: &Project) -> String {
        match self {
            Node::Environment => "Environment".into(),
            Node::Frequencies => {
                let periods = project.waves.frequencies.input_type == gui_core::project::WaveInputType::Period;
                if periods { "Periods" } else { "Omegas" }.into()
            }
            Node::Headings => "Headings".into(),
            Node::Options => "Options".into(),
            Node::FirstOrder => "First Order".into(),
            Node::FieldPoints => "Field Points".into(),
            Node::SecondOrder => "Second Order".into(),
            Node::Bodies => "Bodies".into(),
            Node::Body(i) => project.bodies.get(i).map(|b| b.name.clone()).unwrap_or_default(),
            Node::BodyPart(_, part) => part.label(),
            Node::Lids => "Lids".into(),
            Node::Lid(i) => project.lids.get(i).map(|l| l.name.clone()).unwrap_or_default(),
            Node::FixedDampers => "Fixed Dampers".into(),
            Node::FixedDamper(k) => format!("Fixed Damper {}", k + 1),
            Node::Meshes => "Meshes".into(),
            Node::Mesh(i) => project.meshes.get(i).map(|e| e.name.clone()).unwrap_or_default(),
            Node::Result(r) => result_label(r).into(),
        }
    }

    /// Its title (Details, the output pane): the label, a result with its group's.
    pub fn title(self, project: &Project) -> String {
        match self {
            Node::Result(r) => result_title(r),
            Node::FieldPoints => "First Order · Field Points".into(),
            n => n.label(project),
        }
    }

    /// The node still names something of the project (after a body or lid was deleted).
    pub fn valid(self, project: &Project, result: Option<&SolveResult>) -> bool {
        match self {
            Node::Body(i) => i < project.bodies.len(),
            Node::BodyPart(i, part) => project.bodies.get(i).is_some_and(|b| part.exists(b)),
            Node::Lid(i) => i < project.lids.len(),
            Node::Mesh(i) => i < project.meshes.len(),
            Node::FixedDamper(k) => k < project.fixed_dampers.len(),
            // the Results node stays without results: it says to Run
            Node::Result(ResultNode::Summary) => true,
            Node::Result(r) => result.is_some_and(|res| result_shown(r, res)),
            _ => true,
        }
    }
}

/// The label of a result node: the engine's name of its path (`excitation.froude_krylov` is
/// "Froude Krylov" under "Excitation").
pub fn result_label(r: ResultNode) -> &'static str {
    match r {
        ResultNode::Summary => "Results",
        ResultNode::AddedMass => "Added Mass",
        ResultNode::Damping => "Radiation Damping",
        ResultNode::PressureModeDamping => "Pressure Mode Damping",
        ResultNode::ExcitationGroup => "Excitation",
        ResultNode::Excitation => "Diffraction",
        ResultNode::FroudeKrylov => "Froude Krylov",
        ResultNode::Scattering => "Scattering",
        ResultNode::Haskind => "Haskind",
        ResultNode::Motion => "Motion",
        ResultNode::BodyPoints => "Body Points",
        ResultNode::BodyPressure => "Pressure",
        ResultNode::BodyVelocity => "Velocity",
        ResultNode::FieldPoints => "Field Points",
        ResultNode::Field(FieldKind::Radiation) => "Pressure Radiation",
        ResultNode::Field(FieldKind::Incident) => "Pressure Incident",
        ResultNode::Field(FieldKind::Scattering) => "Pressure Scattering",
        ResultNode::Field(FieldKind::Excitation) => "Pressure Diffraction",
        ResultNode::Field(FieldKind::Total) => "Pressure Total",
        ResultNode::FieldVelocity => "Velocity",
        ResultNode::Lids => "Lids",
        ResultNode::LidElevation => "Elevation",
        ResultNode::PressureSurfaces => "Pressure Surfaces",
        ResultNode::SurfaceElevation => "Elevation",
        ResultNode::Kochin => "Kochin",
        ResultNode::Drift => "Drift",
        ResultNode::Qtf => "QTF",
        ResultNode::Hydrostatics => "Hydrostatics",
    }
}

/// The group a result node is in (None: at the top of the results).
pub fn result_group(r: ResultNode) -> Option<ResultNode> {
    Some(match r {
        ResultNode::Excitation | ResultNode::FroudeKrylov | ResultNode::Scattering | ResultNode::Haskind => {
            ResultNode::ExcitationGroup
        }
        ResultNode::BodyPressure | ResultNode::BodyVelocity => ResultNode::BodyPoints,
        ResultNode::Field(_) | ResultNode::FieldVelocity => ResultNode::FieldPoints,
        ResultNode::LidElevation => ResultNode::Lids,
        // the engine's top-level pressure_mode_damping beside the surfaces whose modes it damps
        ResultNode::SurfaceElevation | ResultNode::PressureModeDamping => ResultNode::PressureSurfaces,
        _ => return None,
    })
}

/// The title of a result node: its label after its group's ("Lids · Elevation").
pub fn result_title(r: ResultNode) -> String {
    match result_group(r) {
        Some(g) => format!("{} · {}", result_label(g), result_label(r)),
        None => result_label(r).to_string(),
    }
}

/// Whether the result tree shows a node: motions only when they were solved, the free surface
/// only with field results.
fn result_shown(r: ResultNode, result: &SolveResult) -> bool {
    match r {
        ResultNode::Motion => result.has(gui_core::hydrobem_engine::QuantityId::Motion),
        ResultNode::FieldPoints | ResultNode::Field(_) => result.has_field_results(),
        ResultNode::BodyPoints => {
            [ResultNode::BodyPressure, ResultNode::BodyVelocity].iter().any(|n| result_shown(*n, result))
        }
        ResultNode::Lids => result_shown(ResultNode::LidElevation, result),
        ResultNode::PressureSurfaces => {
            result_shown(ResultNode::SurfaceElevation, result) || result_shown(ResultNode::PressureModeDamping, result)
        }
        ResultNode::Drift => [
            QuantityId::DriftPressureIntegration,
            QuantityId::DriftControlSurface,
            QuantityId::DriftMomentumConservation,
        ]
        .iter()
        .any(|q| result.has(*q)),
        ResultNode::Qtf => QuantityId::ALL.iter().any(|q| q.name().starts_with("qtf_") && result.has(*q)),
        ResultNode::Kochin => result.has(QuantityId::KochinDiffraction) || result.has(QuantityId::KochinRadiation),
        ResultNode::PressureModeDamping => result.has(QuantityId::PressureModeDamping),
        ResultNode::Haskind => result.has(QuantityId::ExcitationHaskind),
        ResultNode::LidElevation => {
            result.has(QuantityId::LidsElevationDiffraction) || result.has(QuantityId::LidsElevationRadiation)
        }
        ResultNode::SurfaceElevation => {
            result.has(QuantityId::PressureSurfacesElevationDiffraction)
                || result.has(QuantityId::PressureSurfacesElevationRadiation)
        }
        ResultNode::BodyPressure => {
            result.has(QuantityId::BodyPointsPressureDiffraction) || result.has(QuantityId::BodyPointsPressureRadiation)
        }
        ResultNode::BodyVelocity => {
            result.has(QuantityId::BodyPointsVelocityDiffraction) || result.has(QuantityId::BodyPointsVelocityRadiation)
        }
        ResultNode::FieldVelocity => {
            result.has(QuantityId::FieldPointsVelocityIncident) || result.has(QuantityId::FieldPointsVelocityRadiation)
        }
        _ => true,
    }
}

/// The expanded state of the nodes that have children.
#[derive(Clone, Debug)]
pub struct Expanded {
    pub environment: bool,
    pub first_order: bool,
    pub bodies_node: bool,
    pub lids: bool,
    pub fixed_dampers: bool,
    pub meshes: bool,
    pub results: bool,
    pub excitation: bool,
    pub body_points: bool,
    pub field_points: bool,
    pub lid_results: bool,
    pub pressure_surfaces: bool,
    /// Per body, in tree order; a new body is collapsed.
    pub bodies: Vec<bool>,
}

impl Default for Expanded {
    fn default() -> Self {
        Self {
            environment: true,
            first_order: true,
            bodies_node: true,
            lids: true,
            fixed_dampers: true,
            meshes: true,
            results: true,
            excitation: true,
            body_points: true,
            field_points: true,
            lid_results: true,
            pressure_surfaces: true,
            bodies: Vec::new(),
        }
    }
}

pub enum TreeAction {
    Select(Node),
    ImportMesh,
    /// Mesh files (GDF, HBG) dropped onto Meshes: each is imported.
    ImportMeshFiles(Vec<std::path::PathBuf>),
    NewAnalyticMesh,
    DeleteMesh(usize),
    /// Writes a mesh as a geometry file (`.hbg`).
    ExportMesh(usize),
    AddPart(usize, NewPart),
    DeletePart(usize, BodyPart),
    NewFixedDamper,
    DeleteFixedDamper(usize),
    NewBody,
    NewLid,
    DeleteBody(usize),
    DeleteLid(usize),
}

pub fn show(
    ui: &mut Ui,
    project: &Project,
    result: Option<&SolveResult>,
    selected: Node,
    expanded: &mut Expanded,
    actions: &mut Vec<TreeAction>,
) {
    expanded.bodies.resize(project.bodies.len(), false);
    let node = |ui: &mut Ui, n: Node, depth: usize, open: Option<&mut bool>, actions: &mut Vec<TreeAction>| {
        let resp = tree_row(ui, depth, &n.label(project), open, selected == n);
        if resp.clicked() || resp.secondary_clicked() {
            actions.push(TreeAction::Select(n));
        }
        resp
    };
    ui.add_space(4.0);

    // the model's objects first (meshes, bodies, lids, fixed dampers), then the environment,
    // the first and second order and the options
    let meshes = node(ui, Node::Meshes, 0, Some(&mut expanded.meshes), actions);
    let mut drop_rect = meshes.rect;
    meshes.context_menu(|ui| {
        if ui.button("Import Mesh…").clicked() {
            actions.push(TreeAction::ImportMesh);
            ui.close();
        }
        if ui.button("New Analytic Mesh").clicked() {
            actions.push(TreeAction::NewAnalyticMesh);
            ui.close();
        }
    });
    if expanded.meshes {
        for i in 0..project.meshes.len() {
            let row = node(ui, Node::Mesh(i), 1, None, actions);
            drop_rect = drop_rect.union(row.rect);
            row.context_menu(|ui| {
                if ui.button("Export Mesh (.hbg)…").clicked() {
                    actions.push(TreeAction::ExportMesh(i));
                    ui.close();
                }
                let used = !project.mesh_users(i).is_empty();
                if ui
                    .add_enabled(!used, egui::Button::new("Delete Mesh"))
                    .on_disabled_hover_text("The mesh is in use")
                    .clicked()
                {
                    actions.push(TreeAction::DeleteMesh(i));
                    ui.close();
                }
            });
        }
    }
    // mesh files dropped onto Meshes or its meshes are imported
    let extensions = gui_core::mesh::MESH_EXTENSIONS;
    if crate::widgets::dragged_over(ui, drop_rect, &extensions) {
        let painter = ui.painter();
        painter.rect_filled(drop_rect, 3.0, Color32::from_rgba_unmultiplied(0, 120, 215, 28));
        painter.rect_stroke(drop_rect, 3.0, Stroke::new(2.0, style::SELECT), egui::StrokeKind::Inside);
        // on a white label: readable on a selected (blue) row too
        let galley = painter.layout_no_wrap("Drop to import".into(), FontId::proportional(12.0), style::SELECT);
        let at = drop_rect.right_top() + vec2(-8.0 - galley.size().x, 11.0 - galley.size().y / 2.0);
        let label = egui::Rect::from_min_size(at, galley.size()).expand2(vec2(4.0, 1.0));
        painter.rect(label, 3.0, Color32::WHITE, Stroke::new(1.0, style::SELECT), egui::StrokeKind::Inside);
        painter.galley(at, galley, style::SELECT);
    }
    let dropped = crate::widgets::dropped_on(ui, drop_rect, &extensions);
    if !dropped.is_empty() {
        actions.push(TreeAction::ImportMeshFiles(dropped));
    }

    node(ui, Node::Bodies, 0, Some(&mut expanded.bodies_node), actions).context_menu(|ui| {
        if ui.button("New Body").clicked() {
            actions.push(TreeAction::NewBody);
            ui.close();
        }
    });
    if expanded.bodies_node {
        for i in 0..project.bodies.len() {
            let mut open = expanded.bodies[i];
            let resp = node(ui, Node::Body(i), 1, Some(&mut open), actions);
            expanded.bodies[i] = open;
            let body = &project.bodies[i];
            resp.context_menu(|ui| {
                let mut add = |ui: &mut Ui, label: &str, enabled: bool, part: NewPart| {
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                        actions.push(TreeAction::AddPart(i, part));
                        ui.close();
                    }
                };
                add(ui, "Add Additional Damping", body.additional_damping.is_none(), NewPart::Damping);
                add(ui, "Add Additional Stiffness", body.additional_stiffness.is_none(), NewPart::Stiffness);
                add(ui, "Add Dipole", true, NewPart::Dipole);
                add(ui, "Add Dipole Damper", true, NewPart::Damper);
                add(ui, "Add Tank", true, NewPart::Tank);
                add(ui, "Add Pressure Surface", true, NewPart::Pressure);
                add(ui, "Add Free Surface Plate", true, NewPart::Plate);
                add(ui, "Add Control Surface", body.control_surface.is_none(), NewPart::ControlSurface);
                add(ui, "Add Body Points", body.body_points.is_none(), NewPart::BodyPoints);
                add(ui, "Add Generalized Mode", true, NewPart::Generalized);
                add(ui, "Add Mode Matrices", body.mode_matrices.is_none(), NewPart::ModeMatrices);
                // one coupling per other body
                let free = project
                    .bodies
                    .iter()
                    .any(|o| o.name != body.name && body.couplings.iter().all(|c| c.body != o.name));
                add(ui, "Add Coupling", free, NewPart::Coupling);
                ui.separator();
                if ui.button("Delete Body").clicked() {
                    actions.push(TreeAction::DeleteBody(i));
                    ui.close();
                }
            });
            if open {
                for part in BodyPart::of(body) {
                    node(ui, Node::BodyPart(i, part), 2, None, actions).context_menu(|ui| {
                        if ui.button(format!("Delete {}", part.label())).clicked() {
                            actions.push(TreeAction::DeletePart(i, part));
                            ui.close();
                        }
                    });
                }
            }
        }
    }

    node(ui, Node::Lids, 0, Some(&mut expanded.lids), actions).context_menu(|ui| {
        if ui.button("New Lid").clicked() {
            actions.push(TreeAction::NewLid);
            ui.close();
        }
    });
    if expanded.lids {
        for i in 0..project.lids.len() {
            node(ui, Node::Lid(i), 1, None, actions).context_menu(|ui| {
                if ui.button("Delete Lid").clicked() {
                    actions.push(TreeAction::DeleteLid(i));
                    ui.close();
                }
            });
        }
    }
    node(ui, Node::FixedDampers, 0, Some(&mut expanded.fixed_dampers), actions).context_menu(|ui| {
        if ui.button("New Fixed Damper").clicked() {
            actions.push(TreeAction::NewFixedDamper);
            ui.close();
        }
    });
    if expanded.fixed_dampers {
        for k in 0..project.fixed_dampers.len() {
            node(ui, Node::FixedDamper(k), 1, None, actions).context_menu(|ui| {
                if ui.button(format!("Delete Fixed Damper {}", k + 1)).clicked() {
                    actions.push(TreeAction::DeleteFixedDamper(k));
                    ui.close();
                }
            });
        }
    }

    node(ui, Node::Environment, 0, Some(&mut expanded.environment), actions);
    if expanded.environment {
        node(ui, Node::Frequencies, 1, None, actions);
        node(ui, Node::Headings, 1, None, actions);
    }
    node(ui, Node::FirstOrder, 0, Some(&mut expanded.first_order), actions);
    if expanded.first_order {
        node(ui, Node::FieldPoints, 1, None, actions);
    }
    node(ui, Node::SecondOrder, 0, None, actions);
    node(ui, Node::Options, 0, None, actions);

    // the results as the engine's global result tree
    let Some(result) = result else {
        node(ui, Node::Result(ResultNode::Summary), 0, None, actions);
        return;
    };
    node(ui, Node::Result(ResultNode::Summary), 0, Some(&mut expanded.results), actions);
    if !expanded.results {
        return;
    }
    let leaf = |ui: &mut Ui, r: ResultNode, depth: usize, actions: &mut Vec<TreeAction>| {
        if result_shown(r, result) {
            node(ui, Node::Result(r), depth, None, actions);
        }
    };
    // the hydrostatics first (the GUI's order; the engine lists them last)
    for r in [ResultNode::Hydrostatics, ResultNode::AddedMass, ResultNode::Damping] {
        leaf(ui, r, 1, actions);
    }
    let groups: [(ResultNode, &mut bool, Vec<ResultNode>); 5] = [
        (
            ResultNode::ExcitationGroup,
            &mut expanded.excitation,
            vec![ResultNode::Excitation, ResultNode::FroudeKrylov, ResultNode::Scattering, ResultNode::Haskind],
        ),
        (ResultNode::Motion, &mut false, Vec::new()),
        (ResultNode::BodyPoints, &mut expanded.body_points, vec![ResultNode::BodyPressure, ResultNode::BodyVelocity]),
        (
            ResultNode::FieldPoints,
            &mut expanded.field_points,
            vec![
                ResultNode::Field(FieldKind::Radiation),
                ResultNode::Field(FieldKind::Incident),
                ResultNode::Field(FieldKind::Scattering),
                ResultNode::Field(FieldKind::Excitation),
                ResultNode::Field(FieldKind::Total),
                ResultNode::FieldVelocity,
            ],
        ),
        (ResultNode::Lids, &mut expanded.lid_results, vec![ResultNode::LidElevation]),
    ];
    for (group, open, children) in groups {
        if !result_shown(group, result) {
            continue;
        }
        if children.is_empty() {
            node(ui, Node::Result(group), 1, None, actions);
            continue;
        }
        node(ui, Node::Result(group), 1, Some(&mut *open), actions);
        if *open {
            for r in children {
                leaf(ui, r, 2, actions);
            }
        }
    }
    if result_shown(ResultNode::PressureSurfaces, result) {
        node(ui, Node::Result(ResultNode::PressureSurfaces), 1, Some(&mut expanded.pressure_surfaces), actions);
        if expanded.pressure_surfaces {
            leaf(ui, ResultNode::SurfaceElevation, 2, actions);
            leaf(ui, ResultNode::PressureModeDamping, 2, actions);
        }
    }
    for r in [ResultNode::Kochin, ResultNode::Drift, ResultNode::Qtf] {
        leaf(ui, r, 1, actions);
    }
}
