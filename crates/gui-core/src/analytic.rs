//! Analytic patch meshes made in the GUI: one of the engine's named analytic shapes (`Model::
//! analytic_mesh`) from its dimensions. The shapes
//! are quarters about x = 0 and y = 0 (symmetry xz_and_yz). The spec stays with the mesh, so that its
//! Details edit the dimensions and the GUI state reopens it editable.

use std::sync::Arc;

use hydrobem_engine::{AnalyticShape, Model};
use serde::{Deserialize, Serialize};

use crate::mesh::{DEFAULT_DISCRETIZATION, Mesh};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalyticKind {
    #[default]
    Cylinder,
    EllipticCylinder,
    Sphere,
    Ellipsoid,
    Barge,
    MoonpoolCylinder,
    CylinderWithDiscs,
}

impl AnalyticKind {
    pub const ALL: [AnalyticKind; 7] = [
        AnalyticKind::Cylinder,
        AnalyticKind::EllipticCylinder,
        AnalyticKind::Sphere,
        AnalyticKind::Ellipsoid,
        AnalyticKind::Barge,
        AnalyticKind::MoonpoolCylinder,
        AnalyticKind::CylinderWithDiscs,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AnalyticKind::Cylinder => "Circular Cylinder",
            AnalyticKind::EllipticCylinder => "Elliptic Cylinder",
            AnalyticKind::Sphere => "Sphere",
            AnalyticKind::Ellipsoid => "Ellipsoid",
            AnalyticKind::Barge => "Barge",
            AnalyticKind::MoonpoolCylinder => "Moonpool Cylinder",
            AnalyticKind::CylinderWithDiscs => "Cylinder with Dipole Discs",
        }
    }

    /// The engine's name of the shape.
    pub fn shape(self) -> &'static str {
        match self {
            AnalyticKind::Cylinder => "circular_cylinder",
            AnalyticKind::EllipticCylinder => "elliptic_cylinder",
            AnalyticKind::Sphere => "sphere",
            AnalyticKind::Ellipsoid => "ellipsoid",
            AnalyticKind::Barge => "barge",
            AnalyticKind::MoonpoolCylinder => "moonpool_cylinder",
            AnalyticKind::CylinderWithDiscs => "cylinder_with_dipole_discs",
        }
    }

    /// The engine's names of the dimensions, in the order of `dimensions`.
    fn dimension_names(self) -> &'static [&'static str] {
        match self {
            AnalyticKind::Cylinder | AnalyticKind::CylinderWithDiscs => &["radius", "draft"],
            AnalyticKind::EllipticCylinder => &["a", "b", "draft"],
            AnalyticKind::Sphere => &["radius"],
            AnalyticKind::Ellipsoid => &["a", "b", "c"],
            AnalyticKind::Barge => &["half_length", "half_beam", "draft"],
            AnalyticKind::MoonpoolCylinder => &["radius", "draft", "moonpool_radius"],
        }
    }

    /// The engine's names of the parts (patches) in their order, the interior free surface last;
    /// none for the cylinder with dipole discs (its patches follow from the discs).
    fn parts(self) -> &'static [&'static str] {
        match self {
            AnalyticKind::Cylinder | AnalyticKind::EllipticCylinder => &["side", "bottom", "waterplane"],
            AnalyticKind::Sphere | AnalyticKind::Ellipsoid => &["hull", "waterplane"],
            AnalyticKind::Barge => &["end", "side", "bottom", "waterplane"],
            AnalyticKind::MoonpoolCylinder => &["side", "bottom", "moonpool_wall", "moonpool_surface"],
            AnalyticKind::CylinderWithDiscs => &[],
        }
    }

    /// The dimensions, m, in the engine's order, and their defaults.
    pub fn dimensions(self) -> &'static [(&'static str, f64)] {
        match self {
            AnalyticKind::Cylinder | AnalyticKind::CylinderWithDiscs => &[("Radius", 1.0), ("Draft", 1.0)],
            AnalyticKind::EllipticCylinder => &[("Semi-Axis a (x)", 2.0), ("Semi-Axis b (y)", 1.0), ("Draft", 1.0)],
            AnalyticKind::Sphere => &[("Radius", 1.0)],
            AnalyticKind::Ellipsoid => &[("Semi-Axis a (x)", 2.0), ("Semi-Axis b (y)", 1.0), ("Semi-Axis c (z)", 0.5)],
            AnalyticKind::Barge => &[("Half Length", 2.0), ("Half Beam", 1.0), ("Draft", 0.5)],
            AnalyticKind::MoonpoolCylinder => &[("Radius", 1.0), ("Draft", 1.0), ("Moonpool Radius", 0.5)],
        }
    }

    /// The patches of the hull (without the interior free surface).
    fn hull_patches(self) -> i32 {
        match self {
            AnalyticKind::Cylinder | AnalyticKind::EllipticCylinder | AnalyticKind::CylinderWithDiscs => 2,
            AnalyticKind::Sphere | AnalyticKind::Ellipsoid => 1,
            AnalyticKind::Barge | AnalyticKind::MoonpoolCylinder => 3,
        }
    }

    /// Whether the engine adds the interior free surface (the lid against irregular frequencies)
    /// as the last patch.
    pub fn has_lid(self) -> bool {
        self != AnalyticKind::CylinderWithDiscs
    }

    /// Whether the patches may be mapped non-uniformly (finer at the waterline and the corners).
    pub fn has_nonuniform(self) -> bool {
        matches!(
            self,
            AnalyticKind::Cylinder
                | AnalyticKind::EllipticCylinder
                | AnalyticKind::Sphere
                | AnalyticKind::MoonpoolCylinder
        )
    }
}

/// An analytic mesh as the GUI makes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnalyticSpec {
    pub kind: AnalyticKind,
    /// The dimensions of `kind`, in its order (m).
    pub dimensions: Vec<f64>,
    /// The interior free surface as the last patch (the irregular-frequency lid).
    pub lid: bool,
    /// The non-uniform mapping (INONUMAP = 1).
    pub nonuniform: bool,
    /// Cylinder with Dipole Discs: per disc its outer and inner radius and its depth (m).
    pub discs: Vec<[f64; 3]>,
}

impl Default for AnalyticSpec {
    fn default() -> Self {
        AnalyticSpec::of(AnalyticKind::Cylinder)
    }
}

impl AnalyticSpec {
    /// The shape with its default dimensions and the interior free surface.
    pub fn of(kind: AnalyticKind) -> AnalyticSpec {
        AnalyticSpec {
            kind,
            dimensions: kind.dimensions().iter().map(|d| d.1).collect(),
            lid: kind.has_lid(),
            nonuniform: false,
            discs: if kind == AnalyticKind::CylinderWithDiscs { vec![[2.0, 1.0, 0.5]] } else { Vec::new() },
        }
    }

    /// Another kind, keeping the dimensions of the same name.
    pub fn with_kind(&self, kind: AnalyticKind) -> AnalyticSpec {
        let mut next = AnalyticSpec::of(kind);
        for (k, (name, _)) in kind.dimensions().iter().enumerate() {
            if let Some(i) = self.kind.dimensions().iter().position(|d| d.0 == *name)
                && let Some(&v) = self.dimensions.get(i)
            {
                next.dimensions[k] = v;
            }
        }
        next.nonuniform = self.nonuniform && kind.has_nonuniform();
        next
    }

    pub fn patch_count(&self) -> usize {
        let lid = self.lid && self.kind.has_lid();
        let discs = if self.kind == AnalyticKind::CylinderWithDiscs { self.discs.len() } else { 0 };
        self.kind.hull_patches() as usize + usize::from(lid) + discs
    }

    /// The one-based patch of the interior free surface, when there is one (the last).
    pub fn lid_patch(&self) -> Option<i64> {
        (self.lid && self.kind.has_lid()).then_some(self.patch_count() as i64)
    }

    /// The engine's parts of the mesh: the hull, with the interior free surface when kept.
    fn parts(&self) -> Vec<String> {
        let all = self.kind.parts();
        let n = if self.lid_patch().is_some() { all.len() } else { all.len().saturating_sub(1) };
        all[..n].iter().map(|p| p.to_string()).collect()
    }

    /// The mesh the engine makes of it, with the discretization `rows` (one per patch, else the
    /// default for every patch); the error is the engine's.
    pub fn build(&self, rows: Option<Vec<[i32; 6]>>) -> Result<Arc<Mesh>, String> {
        let discretization = rows.filter(|r| r.len() == self.patch_count()).unwrap_or(vec![DEFAULT_DISCRETIZATION]);
        let names = self.kind.dimension_names();
        let shape = AnalyticShape {
            shape: self.kind.shape().into(),
            dimensions: names.iter().zip(&self.dimensions).map(|(n, v)| (n.to_string(), *v)).collect(),
            discs: if self.kind == AnalyticKind::CylinderWithDiscs { self.discs.clone() } else { Vec::new() },
            parts: self.parts(),
            mapping: if self.nonuniform && self.kind.has_nonuniform() { "nonuniform" } else { "uniform" }.into(),
            discretization,
            reference_length: 1.0,
            symmetry: "xz_and_yz".into(),
            label: format!("{} (analytic)", self.kind.label()),
        };
        let mut m = Model::new();
        let id = m.analytic_mesh("mesh", &shape).map_err(|e| e.message().to_string())?;
        let mut mesh = Mesh::from_object(&m, id)?;
        mesh.analytic = Some(self.clone());
        mesh.rows_from = crate::mesh::RowsFrom::Analytic;
        Ok(Arc::new(mesh))
    }

    /// The same shape again with these changes, keeping the discretization of each patch when the
    /// patches stay the same.
    pub fn rebuild(&self, from: &Mesh) -> Result<Arc<Mesh>, String> {
        let rows: Vec<[i32; 6]> = from.discretization().iter().map(|r| r.map(|v| v as i32)).collect();
        self.build(Some(rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shape_builds_with_its_defaults() {
        for kind in AnalyticKind::ALL {
            let spec = AnalyticSpec::of(kind);
            let mesh = spec.build(None).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            assert!(mesh.is_patch());
            assert_eq!(mesh.panels.len(), spec.patch_count(), "{kind:?}");
            assert_eq!(mesh.analytic.as_ref(), Some(&spec));
            assert_eq!(spec.lid_patch().is_some(), kind.has_lid());
        }
    }

    #[test]
    fn the_engine_refuses_impossible_dimensions() {
        let mut spec = AnalyticSpec::of(AnalyticKind::MoonpoolCylinder);
        spec.dimensions[2] = 2.0; // the moonpool wider than the cylinder
        assert!(spec.build(None).is_err());
        let mut spec = AnalyticSpec::of(AnalyticKind::Cylinder);
        spec.dimensions[1] = -1.0;
        assert!(spec.build(None).is_err());
    }

    #[test]
    fn a_new_kind_keeps_the_dimensions_of_the_same_name() {
        let mut c = AnalyticSpec::of(AnalyticKind::Cylinder);
        c.dimensions = vec![3.0, 2.0];
        let m = c.with_kind(AnalyticKind::MoonpoolCylinder);
        assert_eq!(m.dimensions, vec![3.0, 2.0, 0.5]);
    }
}
