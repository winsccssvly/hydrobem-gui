//! The codes of the engine: result quantities, stages, states, warnings, roles, statuses.

macro_rules! quantities {
    ($($id:ident = $name:literal,)*) => {
        /// A result quantity, in the order of the engine's quantity table (`hb_quantities`).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum QuantityId {
            $($id,)*
        }

        impl QuantityId {
            /// Every quantity in the order of the table.
            pub const ALL: &[QuantityId] = &[$(QuantityId::$id,)*];

            /// Its name in the engine, e.g. "added_mass".
            pub fn name(self) -> &'static str {
                match self {
                    $(QuantityId::$id => $name,)*
                }
            }

            pub fn from_name(name: &str) -> Option<QuantityId> {
                QuantityId::ALL.iter().copied().find(|q| q.name() == name)
            }
        }
    };
}

/// The axes, unit and description of each quantity, from the DLL's quantity table (empty texts
/// when it is not loaded).
fn quantity_texts() -> &'static [[String; 3]] {
    static TEXTS: std::sync::OnceLock<Vec<[String; 3]>> = std::sync::OnceLock::new();
    TEXTS.get_or_init(|| {
        #[allow(unsafe_code)]
        // SAFETY: a writable pointer
        let read = || -> crate::Result<Vec<[String; 3]>> {
            let t = crate::table::table(|a, out| unsafe { (a.hb_quantities)(out) })?;
            let (dims, unit, description) = (t.texts("dims")?, t.texts("unit")?, t.texts("description")?);
            Ok((0..dims.len()).map(|k| [dims[k].clone(), unit[k].clone(), description[k].clone()]).collect())
        };
        read().unwrap_or_default()
    })
}

impl QuantityId {
    fn texts(self) -> Option<&'static [String; 3]> {
        quantity_texts().get(self as usize)
    }

    /// Its axes, e.g. "frequency,radiation_mode,coordinate".
    pub fn dims(self) -> &'static str {
        self.texts().map_or("", |t| &t[0])
    }

    pub fn unit(self) -> &'static str {
        self.texts().map_or("", |t| &t[1])
    }

    pub fn description(self) -> &'static str {
        self.texts().map_or("", |t| &t[2])
    }
}

quantities! {
    Omegas = "omegas",
    Headings = "headings",
    RadiationModeIds = "radiation_mode_ids",
    Coordinates = "coordinates",
    AddedMass = "added_mass",
    RadiationDamping = "radiation_damping",
    AddedMassZeroFrequency = "added_mass_zero_frequency",
    AddedMassInfiniteFrequency = "added_mass_infinite_frequency",
    ExcitationDiffraction = "excitation_diffraction",
    ExcitationFroudeKrylov = "excitation_froude_krylov",
    ExcitationScattering = "excitation_scattering",
    ExcitationHaskind = "excitation_haskind",
    Motion = "motion",
    ModeRestoring = "mode_restoring",
    PressureModeDamping = "pressure_mode_damping",
    BodyPoints = "body_points",
    BodyPointsIds = "body_points_ids",
    BodyPointsNormals = "body_points_normals",
    BodyPointsAreas = "body_points_areas",
    BodyPointsPressureRadiation = "body_points_pressure_radiation",
    BodyPointsPressureDiffraction = "body_points_pressure_diffraction",
    BodyPointsVelocityRadiation = "body_points_velocity_radiation",
    BodyPointsVelocityDiffraction = "body_points_velocity_diffraction",
    FieldPoints = "field_points",
    FieldPointsPressureRadiation = "field_points_pressure_radiation",
    FieldPointsPressureIncident = "field_points_pressure_incident",
    FieldPointsPressureScattering = "field_points_pressure_scattering",
    FieldPointsPressureDiffraction = "field_points_pressure_diffraction",
    FieldPointsVelocityRadiation = "field_points_velocity_radiation",
    FieldPointsVelocityIncident = "field_points_velocity_incident",
    FieldPointsVelocityScattering = "field_points_velocity_scattering",
    FieldPointsVelocityDiffraction = "field_points_velocity_diffraction",
    Lids = "lids",
    LidsIds = "lids_ids",
    LidsElevationRadiation = "lids_elevation_radiation",
    LidsElevationDiffraction = "lids_elevation_diffraction",
    PressureSurfaces = "pressure_surfaces",
    PressureSurfacesIds = "pressure_surfaces_ids",
    PressureSurfacesElevationRadiation = "pressure_surfaces_elevation_radiation",
    PressureSurfacesElevationDiffraction = "pressure_surfaces_elevation_diffraction",
    Tanks = "tanks",
    TanksIds = "tanks_ids",
    TanksPressureRadiation = "tanks_pressure_radiation",
    TanksVelocityRadiation = "tanks_velocity_radiation",
    KochinAngles = "kochin_angles",
    KochinRadiation = "kochin_radiation",
    KochinRadiationDerivative = "kochin_radiation_derivative",
    KochinDiffraction = "kochin_diffraction",
    KochinDiffractionDerivative = "kochin_diffraction_derivative",
    DriftPressureIntegration = "drift_pressure_integration",
    DriftPressureIntegrationBodyAxes = "drift_pressure_integration_body_axes",
    DriftControlSurface = "drift_control_surface",
    DriftControlSurfaceBodyAxes = "drift_control_surface_body_axes",
    DriftMomentumConservation = "drift_momentum_conservation",
    DriftHeadingPairs = "drift_heading_pairs",
    QtfQuadraticPressureIntegrationDifference = "qtf_quadratic_pressure_integration_difference",
    QtfQuadraticPressureIntegrationSum = "qtf_quadratic_pressure_integration_sum",
    QtfQuadraticControlSurfaceDifference = "qtf_quadratic_control_surface_difference",
    QtfQuadraticControlSurfaceSum = "qtf_quadratic_control_surface_sum",
    QtfPotentialDirectDifference = "qtf_potential_direct_difference",
    QtfPotentialDirectSum = "qtf_potential_direct_sum",
    QtfPotentialIndirectDifference = "qtf_potential_indirect_difference",
    QtfPotentialIndirectSum = "qtf_potential_indirect_sum",
    QtfNewmanPressureIntegrationDifference = "qtf_newman_pressure_integration_difference",
    QtfNewmanControlSurfaceDifference = "qtf_newman_control_surface_difference",
    QtfNewmanMomentumConservationDifference = "qtf_newman_momentum_conservation_difference",
    QtfHeadingPairs = "qtf_heading_pairs",
    HydrostaticsVolumes = "hydrostatics_volumes",
    HydrostaticsVolume = "hydrostatics_volume",
    HydrostaticsDisplacedMass = "hydrostatics_displaced_mass",
    HydrostaticsCenterOfBuoyancy = "hydrostatics_center_of_buoyancy",
    HydrostaticsWaterplaneArea = "hydrostatics_waterplane_area",
    HydrostaticsWaterplaneCentroid = "hydrostatics_waterplane_centroid",
    HydrostaticsStiffnessRhoG = "hydrostatics_stiffness_rho_g",
    HydrostaticsMass = "hydrostatics_mass",
    HydrostaticsCenterOfGravity = "hydrostatics_center_of_gravity",
    HydrostaticsStiffness = "hydrostatics_stiffness",
    HydrostaticsMetacentricHeight = "hydrostatics_metacentric_height",
}

/// The element type of a result quantity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ElementType {
    Float64,
    Float32,
    /// complex float32 (real, imaginary)
    Complex64,
    Int32,
    /// complex float64 (real, imaginary)
    Complex128,
    Int64,
    Text,
}

impl ElementType {
    /// The bytes of one element (0 for text).
    pub fn size(self) -> usize {
        match self {
            ElementType::Float32 | ElementType::Int32 => 4,
            ElementType::Float64 | ElementType::Int64 | ElementType::Complex64 => 8,
            ElementType::Complex128 => 16,
            ElementType::Text => 0,
        }
    }

    pub(crate) fn from_dtype(dtype: i32) -> Option<ElementType> {
        use crate::ffi::*;
        Some(match dtype {
            HB_F64 => ElementType::Float64,
            HB_F32 => ElementType::Float32,
            HB_I32 => ElementType::Int32,
            HB_I64 => ElementType::Int64,
            HB_C64 => ElementType::Complex64,
            HB_C128 => ElementType::Complex128,
            HB_TEXT => ElementType::Text,
            _ => return None,
        })
    }
}

/// The stage of a solve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
    Statics,
    Assembly,
    Lu,
    Outputs,
    /// one more frequency (or limit) finished
    FrequencyDone,
    /// the parts of an item of a second-order step
    SecondOrder,
    /// the phase of the solve changed
    Phase,
    /// a frequency worker has no item left in its phase
    WorkerIdle,
}

impl Stage {
    pub(crate) fn from_code(code: i32) -> Stage {
        match code {
            0 => Stage::Statics,
            1 => Stage::Assembly,
            2 => Stage::Lu,
            3 => Stage::Outputs,
            5 => Stage::SecondOrder,
            6 => Stage::Phase,
            7 => Stage::WorkerIdle,
            _ => Stage::FrequencyDone,
        }
    }

    /// The stage of a name of the progress table ("assembly", "lu", "outputs", "second_order"; ""
    /// none).
    pub(crate) fn from_name(name: &str) -> Option<Stage> {
        Some(match name {
            "statics" => Stage::Statics,
            "assembly" => Stage::Assembly,
            "lu" => Stage::Lu,
            "outputs" => Stage::Outputs,
            "frequency_done" => Stage::FrequencyDone,
            "second_order" => Stage::SecondOrder,
            "phase" => Stage::Phase,
            "worker_idle" => Stage::WorkerIdle,
            _ => return None,
        })
    }
}

/// The phase of a solve (`HB_PHASE_`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Phase {
    /// the frequency-independent influence data
    #[default]
    Preparation,
    /// the limits of the radiation problem
    Limits,
    /// the ordinary frequencies
    FirstOrder,
    /// the second-order loads, after every frequency
    SecondOrder,
    /// the results written
    Finishing,
    Done,
    Cancelled,
    Failed,
}

impl Phase {
    const ALL: [Phase; 8] = [
        Phase::Preparation,
        Phase::Limits,
        Phase::FirstOrder,
        Phase::SecondOrder,
        Phase::Finishing,
        Phase::Done,
        Phase::Cancelled,
        Phase::Failed,
    ];
    const NAMES: [&str; 8] =
        ["preparation", "limits", "first_order", "second_order", "finishing", "done", "cancelled", "failed"];

    pub(crate) fn from_code(code: i32) -> Phase {
        usize::try_from(code).ok().and_then(|k| Phase::ALL.get(k).copied()).unwrap_or_default()
    }

    pub(crate) fn from_name(name: &str) -> Option<Phase> {
        Phase::NAMES.iter().position(|n| *n == name).map(|k| Phase::ALL[k])
    }
}

/// A step of the second order of a solve (`HB_STEP_`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SecondOrderStep {
    /// the quadratic QTF by pressure integration (items the rows ω_a)
    QuadraticPressureIntegration,
    /// the quadratic QTF over the control surfaces (items the rows ω_a)
    QuadraticControlSurface,
    /// the first-order sub-problem of the potential QTF (items its frequencies)
    PotentialFirstOrder,
    /// the radiation at the pair frequencies of the potential QTF (items its frequencies)
    PotentialRadiation,
    /// the far-field fits of the potential QTF (one item)
    PotentialFits,
    /// the outer integral of the potential QTF (items its batches of radii)
    PotentialOuter,
    /// the inner free surface and the body of every pair (items the pair frequencies)
    PotentialPairs,
    /// the direct method's solve of the potential QTF (items its frequencies)
    PotentialDirect,
}

impl SecondOrderStep {
    const ALL: [SecondOrderStep; 8] = [
        SecondOrderStep::QuadraticPressureIntegration,
        SecondOrderStep::QuadraticControlSurface,
        SecondOrderStep::PotentialFirstOrder,
        SecondOrderStep::PotentialRadiation,
        SecondOrderStep::PotentialFits,
        SecondOrderStep::PotentialOuter,
        SecondOrderStep::PotentialPairs,
        SecondOrderStep::PotentialDirect,
    ];
    const NAMES: [&str; 8] = [
        "quadratic_pressure_integration",
        "quadratic_control_surface",
        "potential_first_order",
        "potential_radiation",
        "potential_fits",
        "potential_outer",
        "potential_pairs",
        "potential_direct",
    ];

    pub(crate) fn from_code(code: i32) -> Option<SecondOrderStep> {
        usize::try_from(code).ok().and_then(|k| SecondOrderStep::ALL.get(k).copied())
    }

    pub(crate) fn from_name(name: &str) -> Option<SecondOrderStep> {
        SecondOrderStep::NAMES.iter().position(|n| *n == name).map(|k| SecondOrderStep::ALL[k])
    }

    /// The engine's name of the step.
    pub fn name(self) -> &'static str {
        SecondOrderStep::NAMES[SecondOrderStep::ALL.iter().position(|s| *s == self).unwrap_or(0)]
    }
}

/// The state of a model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelState {
    /// no results of its present inputs
    Reset,
    /// a solve runs
    Calculating,
    /// it holds the results of its present inputs
    Calculated,
}

/// The severity of a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    Warning,
    Error,
}

/// The warnings of the model rules and of a solve (the `code` of their diagnostics).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Warning {
    PressureDriftPressureSurfaces = 1,
    CoarseMesh = 2,
    MeshOrientation = 3,
    MeshOpen = 4,
    MeshVolume = 5,
    FieldPointNearBody = 6,
    TankZeroFrequency = 7,
    /// a disabled object would fail the rules were it enabled
    DisabledObject = 8,
    /// a hook of the environment that changes the numbers is set (internal studies only)
    EnvironmentHook = 9,
}

impl Warning {
    const ALL: [Warning; 9] = [
        Warning::PressureDriftPressureSurfaces,
        Warning::CoarseMesh,
        Warning::MeshOrientation,
        Warning::MeshOpen,
        Warning::MeshVolume,
        Warning::FieldPointNearBody,
        Warning::TankZeroFrequency,
        Warning::DisabledObject,
        Warning::EnvironmentHook,
    ];

    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn from_code(code: i32) -> Option<Warning> {
        Warning::ALL.into_iter().find(|w| w.code() == code)
    }
}

/// The role of an element as the engine uses it (`Panels::role`; the codes and names of
/// `hb_panel_roles`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelRole {
    /// the wetted surface of a body
    Wetted = 0,
    /// the interior surface (irregular frequencies), generated or given
    InteriorSurface = 1,
    /// a zero-thickness rigid sheet (`dipoles`)
    Dipole = 2,
    /// a dipole damper
    DipoleDamper = 3,
    PressureSurface = 4,
    Lid = 5,
    /// a tank wall
    Tank = 6,
    /// a fixed damper (`fixed_dampers`)
    FixedDamper = 7,
    /// the control surface of a body (on request)
    ControlSurface = 8,
    /// the panelled zone of the potential QTF's free surface (on request)
    FreeSurface = 9,
    /// a rigid plate in the free surface (`free_surface_plates`)
    FreeSurfacePlate = 10,
}

impl PanelRole {
    const ALL: [PanelRole; 11] = [
        PanelRole::Wetted,
        PanelRole::InteriorSurface,
        PanelRole::Dipole,
        PanelRole::DipoleDamper,
        PanelRole::PressureSurface,
        PanelRole::Lid,
        PanelRole::Tank,
        PanelRole::FixedDamper,
        PanelRole::ControlSurface,
        PanelRole::FreeSurface,
        PanelRole::FreeSurfacePlate,
    ];
    const NAMES: [&str; 11] = [
        "wetted",
        "interior_surface",
        "dipole",
        "dipole_damper",
        "pressure_surface",
        "lid",
        "tank",
        "fixed_damper",
        "control_surface",
        "free_surface",
        "free_surface_plate",
    ];

    /// The role of its value (the `role` column of the panels).
    pub fn from_value(v: i32) -> Option<PanelRole> {
        PanelRole::ALL.get(usize::try_from(v).ok()?).copied()
    }

    pub fn value(self) -> i32 {
        self as i32
    }

    /// Its name in the engine ("wetted", "interior_surface", ...), the `role` argument of the
    /// queries.
    pub fn name(self) -> &'static str {
        PanelRole::NAMES[self as usize]
    }
}

/// The findings of the mesh quality of a record: a set of bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct QualityFlags(pub u32);

impl QualityFlags {
    pub const ORIENTATION: QualityFlags = QualityFlags(1);
    pub const FREE_EDGE: QualityFlags = QualityFlags(2);
    pub const NON_MANIFOLD: QualityFlags = QualityFlags(4);
    pub const COARSE: QualityFlags = QualityFlags(8);

    pub fn contains(self, f: QualityFlags) -> bool {
        self.0 & f.0 == f.0
    }

    /// Whether it has one of the flags of f.
    pub fn intersects(self, f: QualityFlags) -> bool {
        self.0 & f.0 != 0
    }
}

impl std::ops::BitOr for QualityFlags {
    type Output = QualityFlags;
    fn bitor(self, o: QualityFlags) -> QualityFlags {
        QualityFlags(self.0 | o.0)
    }
}

/// The status of a point under the field-point rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointStatus {
    Fluid,
    Tank,
    NoTank,
    EmptyTank,
    AboveTankSurface,
    AboveFreeSurface,
    BelowSeaBed,
    OutsideDomain,
    OnBody,
    InsideWaterplane,
    OnLid,
    NotFinite,
    InsideBody,
    NearBody,
}

impl PointStatus {
    pub(crate) fn from_name(name: &str) -> Option<PointStatus> {
        Some(match name {
            "fluid" => PointStatus::Fluid,
            "tank" => PointStatus::Tank,
            "no_tank" => PointStatus::NoTank,
            "empty_tank" => PointStatus::EmptyTank,
            "above_tank_surface" => PointStatus::AboveTankSurface,
            "above_free_surface" => PointStatus::AboveFreeSurface,
            "below_sea_bed" => PointStatus::BelowSeaBed,
            "outside_domain" => PointStatus::OutsideDomain,
            "on_body" => PointStatus::OnBody,
            "inside_waterplane" => PointStatus::InsideWaterplane,
            "on_lid" => PointStatus::OnLid,
            "not_finite" => PointStatus::NotFinite,
            "inside_body" => PointStatus::InsideBody,
            "near_body" => PointStatus::NearBody,
            _ => return None,
        })
    }
}
