//! The UI-free logic of the HydroBEM GUI: the project, its engine model, files, solves and the
//! views of the results. Nothing here depends on egui or wgpu (AGENTS.md).

pub mod analytic;
pub mod animation;
pub mod engine_info;
pub mod engine_model;
pub mod field_layout;
pub mod field_points;
pub mod files;
pub mod format;
pub mod hbg_upgrade;
pub mod mesh;
pub mod npz;
pub mod patch_report;
pub mod project;
pub mod results;
pub mod scene;
pub mod solve;
pub mod workers;
pub mod zip;

pub use hydrobem_engine;
