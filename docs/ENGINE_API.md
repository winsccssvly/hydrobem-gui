# The engine and how the GUI uses it

The engine is HydroBEM's `hydrobem.dll`, the C ABI of its engine (ABI version 8, HydroBEM 0.3.0, header
`engine/hydrobem.h`, HydroBEM `docs/reference/c-api.md`). HydroBEM builds the DLL (`cargo build
--release -p hydrobem-c` in HydroBEM); the GUI imports it, loads it at run time and calls it
through `crates/hydrobem-engine` only, a safe binding whose names follow the engine's Rust API
(`Model`, `Value`, `Results`, `Panels`, ...). Every answer is read from the DLL's tables (named
columns with their shapes) and its result views; the binding never infers a dimension from a
length.

## Import

```powershell
./scripts/import-engine.ps1 -Source ..\HydroBEM
```

copies, read-only, `target/release/hydrobem.dll`, `crates/hydrobem-c/include/hydrobem.h` and the
licence files into `engine/`, and writes `engine/IMPORT.json`: the source commit and `git describe`,
the dirty flag of the engine sources, the ABI version of the header, the times of the DLL and of
the source commit, and the SHA-256 of every file. It warns when the DLL is older than the source
commit (built from an earlier one). HydroBEM is never built from here. `engine/hydrobem.dll` is
not committed (`.gitignore`); the header and `IMPORT.json` are. The build script of
`crates/hydrobem-engine` copies the DLL beside the programs of the build (`target/<profile>/`), and
`cargo xtask dist` beside `dist/HydroBEM.exe`.

At start-up HydroBEM.exe loads `hydrobem.dll` (`HYDROBEM_DLL` when set, else beside the program,
else one folder up for cargo's test programs) and checks its ABI (`hb_abi_version`) and its result
quantities (`hb_quantities`, by name and order) against the binding; else it says why in a message
box and exits. The commit shown (status bar, About, Summary) is the one the DLL reports of its own
build (`hb_build_info`), not that of the import.

The engine requires AVX2 and FMA. `.cargo/config.toml` sets `+avx2,+fma` (HydroBEM's own flags);
HydroBEM.exe refuses to start without them. The C runtime is linked as a DLL, as HydroBEM's (since
HydroBEM `ee6c71b`): `hydrobem.dll` and HydroBEM.exe import `vcruntime140.dll` and the Universal C
Runtime (`api-ms-win-crt-*`), which Python and Windows provide (`REQUIREMENTS.md`).

## Threads and progress

`hb_solve` runs on the solve thread with the model's lock released; the UI thread reads the
progress with `hb_model_progress` every frame (`CancelToken::progress`, without waiting) and
cancels with `hb_model_cancel`. The progress is the engine's: the preparation (the influence
data, 0 to 1), each limit by kind (ω = 0, ω = ∞) and each ordinary frequency in the order of the
results, with its state (waiting, running, done, failed, cancelled), stage, the share of its
work done (the stages weighted by their cost; never decreasing, 1 once done) and the worker that
runs it; the phase of the solve (`Phase`: preparation, limits, first order, second order,
finishing, then done, cancelled or failed); each frequency worker (`WorkerProgress`,
frequency_threads of them, fixed numbers): running, idle or done, and what it is on (a limit, a
frequency, an item of a second-order step, or the work all workers share); and each step of the
second order (`SecondOrderProgress`: the quadratic QTF rows, the potential QTF's sub-problems,
fits, outer integral and pairs) with its state, share and items. The callback (`Progress`) carries
the reporting worker, the phase and the second-order step and item as well (since C ABI 7).

## The project as an engine model

`gui_core::engine_model::build_model` is the one path from the GUI's inputs to the engine, for
solving, for the queries and for saving; `read_model` is the inverse. Items are set through
`Model::set`, which checks each value and names the object path when it rejects one.

| GUI | Engine object / item |
|---|---|
| Depth Type, Depth, Density, Gravity | `environment/depth` (inf for infinite), `gravity`, `density` |
| Domain, Channel Width | `environment/domain` (`open_sea`, `x_wall`, `y_wall`, `orthogonal_walls`, `channel`), `channel_width` |
| Diffraction Formulation (First Order) | `first_order/diffraction_formulation` (`scattered`, `total`, `auto`) |
| Logarithmic Integration, tolerance | `options/logarithmic_integration`, `log_tolerance` |
| Frequency / Inner Threads | `options/frequency_threads`, `inner_threads` (1..1024) |
| Radiation, Diffraction, Motion | `first_order/radiation`, `diffraction`, `motion` (`Project::first_order`, the same names) |
| Field points (submitted) | the points in the sea `first_order/field_points` [N,3] (global); the points of a tank its `field_points` [N,3] in its body's axes; `first_order/field_pressure = true` |
| Omegas, Periods (Frequency Input) | `environment/omegas` (Omegas), or `frequency_input = "periods"` and `periods` (Periods) |
| Wave Headings | `environment/headings`, only with diffraction |
| Body | `bodies/<name>`: `position`, `yaw`, `rotation_center` (R), `modes`, `center_of_gravity` |
| Mass | Displaced: `mass = "displaced"`; Given: `mass` = the value |
| Ixx … Izz | `inertia_input = "components"`, the six items and `inertia_origin = "rotation_center"`, only when one is non-zero (else the engine's default: radii of gyration of zero) |
| Additional Damping / Stiffness | `additional_damping`, `additional_stiffness` [6,6] about R, only when non-zero |
| Mesh library | every mesh of the library is a `meshes/<name>` object (kind `panels`, `vertices` [P,4,3], `symmetry` `none/yz/xz/xz_and_yz` (the GUI's X = 0, Y = 0, both planes), `reference_length`, `label`, `source_file`), used or not, in tree order |
| Body mesh, Panel Range | the body's `mesh` names its library mesh and `ranges` [n,2] the records of its range less those its surfaces take (unset: every record) |
| Interior Surface None / Detect / Projected / Waterline / Graded | the body's switch `interior_surface` (`none`, `detect`, `projected`, `waterline`, `graded`); None is written as `none` (the engine's default is `detect`) |
| IRR Panel Range | `interior_surface = "mesh"`, `interior_surface_mesh` = the body mesh, `interior_surface_ranges` its records |
| Dipole K | child `dipoles/K` with `mesh` and `ranges` (records of the body mesh), or `mesh` alone (a library mesh) |
| Free Surface Plate K | child `free_surface_plates/K` with `mesh` and `ranges` (records of the body mesh in z = 0), or `mesh` alone (a library mesh in body axes), or Generated: `inside` (x, y) with `body` (set only for another body than its own) and no mesh, the engine's plate inside a trimmed hull's waterline loop; `enabled`. A body without a mesh takes its first plate's (a body of plates alone) |
| Pressure Surface K | child `pressure_surfaces/K` with `mesh` and `ranges` (or a library mesh), `chamber_coupling = true`, one mode `pressure_modes/Uniform_K` (`shape = "uniform"`) |
| Lid | `lids/<name>` with `mesh` (global coordinates) and `damping_factor` |
| Fixed Dipole Damper K | `fixed_dampers/K` with `mesh` (a library mesh, global) and `damping_factor` |
| Body Dipole Damper K | child `dipole_dampers/K` with `mesh` and `ranges` (records of the body mesh), `damping_factor` |
| Tank K | child `tanks/<name>` (the tank's name, else `tank_K`) with `mesh` and `ranges` (records of the body mesh), `density_ratio`, `surface_level` (set only when Given) |
| Control Surface | the body's switch `control_surface` (`cylinder`, `ellipsoid`, `automatic`, `mesh`) with the items `control_surface_radius`, `_depth`, `_inner_radius`, `_semi_axes`, `_inner_semi_axes`, `_panel_size`, or `control_surface_mesh` (a library mesh); `control_surface_waterline_term = "body"` for the waterline term on the body. The automatic surface always has its free-surface part (the engine removed the switch) |
| Body Points | `body_points` [N,3], body-local |
| Trim | `trim` = [elevation, pitch, roll] (m, deg) |
| Excitation Method | `excitation_method = "haskind"` for Haskind |
| Channel Reflect | `environment/channel_reflect` (0..1) |
| Mean drift | `second_order/drift_pressure_integration`, `drift_control_surface`, `drift_momentum_conservation`, `drift_bidirectional` (the heading pairs of every drift), `drift_momentum_conservation_angles` (auto when unset): `Project::second_order`, the same names |
| Point outputs | `first_order/field_velocity`, `body_pressure`, `body_velocity` |
| QTF | `second_order/qtf_quadratic_pressure_integration`, `qtf_quadratic_control_surface`, `qtf_potential_direct`, `qtf_potential_indirect`, `qtf_newman`, `qtf_newman_average`, `qtf_bidirectional`, `qtf_difference`, `qtf_sum` (`Project::second_order`, the same names) |
| Free Surface (Potential QTF) | `second_order/free_surface_method` (`panels`, `cells`, `mesh`), `free_surface_panel_size`, `free_surface_inner_radius` (unset: `"auto"`), `free_surface_mesh` (a library mesh, every record; kept while another method is chosen), `free_surface_quadrature_points`, `quadrature_annuli`, `quadrature_radius_step`, `quadrature_radial_nodes`, `quadrature_azimuthal_nodes`, `outer_circle_segments`, `expansion_order` (unset: `"auto"`); a model with `free_surface_ranges` is refused |
| Added Mass Limits | `environment/omegas` with 0 and inf appended (or `periods` with inf and 0); results `added_mass_zero_frequency`, `added_mass_infinite_frequency` [D][K] |
| Pressure Surface K, modes | `chamber_coupling`, `response_convention` (`variational`, `numeric_compatibility`); child `pressure_modes/<name>` per mode with `shape` (`uniform`, `sign_x`, `sign_y`), `additional_stiffness`, `additional_damping` |
| Generalized Mode K | child `generalized_modes/<name>` with `parity` (a rigid mode) and `pieces`: `u`, `v`, `w` [N,4] terms (c, px, py, pz) and `ranges` (records of the body mesh, unset for every wetted record) |
| Mode Matrices | `mode_mass`, `mode_damping`, `mode_stiffness` [(6+G),(6+G)], only the non-zero ones |
| Additional matrices, About | `stiffness_origin`, `damping_origin`: `center_of_gravity` or a point; unset for the rotation centre |
| IRR Mesh | `interior_surface = "mesh"` with `interior_surface_mesh` = a library mesh; for a patch-mesh body a patch mesh, which the engine appends to the body's records before the rules (D-165). Drawing: by `body_tessellation` like every element of the body (Viewport) |
| Tank sheets | the tank's children `dipoles` and `dipole_dampers` (`damping_factor`), each `mesh` and `ranges` of the body mesh |
| Tank free-surface plates | the tank's children `free_surface_plates`, each `mesh` and `ranges` of the body mesh or `mesh` alone (a library mesh at the tank's surface level); `enabled`. The engine's queries give them role `free_surface_plate` with the tank's key |
| Control polygon, partitions | `control_surface_outer = "polygon"` with `control_surface_boundary` [N,2] instead of `control_surface_radius`; `control_surface_partitions` [N,2], each polyline ended by a row of NaN |
| Lid Placed With | `placed_with` = the body; `mesh` = that body's mesh and `ranges` its records, which leave its wetted surface |
| Body Points neighbours | `body_point_neighbors` (written when not 4) |
| Field point depths and tank | every depth of the grid in the points; a point of a tank (the grid's tank, or a listed point's) goes to that tank's `field_points` in its body's axes (`Body::to_local`), judged by `classify_points` with the tank |
| Several record ranges | a surface's Start/End Panel and its Further Ranges are its `ranges`, every range; the body's Unused Records leave its `ranges` as the surfaces' records do |
| Analytic mesh made in the GUI | `Model::analytic_mesh` with `AnalyticShape` by name (`circular_cylinder`, `elliptic_cylinder`, `sphere`, `ellipsoid`, `barge`, `moonpool_cylinder`, `cylinder_with_dipole_discs`), its dimensions by name, `parts` = every part with the interior free surface or every part but it (none for the discs, whose patches follow from `discs`), `mapping` `uniform` / `nonuniform`, one discretization row or one per patch, symmetry `xz_and_yz`); the mesh object it makes is written like any patch mesh |
| Higher-order mesh | the mesh object as the engine read it (kind `flat_patches`, `spline_patches` or `analytic_patches`, its `patches` and `discretization`), copied item by item; `options/higher_order = true` when an active body uses a patch mesh; drawn by `mesh_tessellation` / `body_tessellation` (n x n per patch, n = 4 ... 24 for about 20 000 quadrilaterals) with its corner normals, outlined where a quadrilateral's side belongs to no other of its element (its seven keys) |
| Point List | as listed: the points in the sea in `first_order/field_points`, each tank's in its `field_points` |
| Generalized mode display | `Model::mode_displacements` at the corners of the body's wetted panels (the body alone), column 6 + k |
| Body output faces | `Model::panels(None, true)` of the solved model, matched to `body_points_ids` (body, record, reflection) and the centroid |
| Suppressed, Enabled | `enabled = false` on the body, lid, dipole, dipole damper (of a body or a tank), fixed damper, tank, pressure surface or coupling; the engine solves the model as if it were absent. A suppressed body the engine cannot hold (an item it refuses) stays in the GUI state alone |
| Coupling K | child `couplings/K` of the body with `body` = the other body's name, `stiffness` and `damping` [6,6] (the non-zero ones): this body's forces from the other body's motions, about each body's `stiffness_origin` / `damping_origin`; written after every body (a reference needs its body) |
| Field Pressure, Field Velocity | `first_order/field_pressure`, `first_order/field_velocity`, set after the field points are judged and written (the engine builds no model asking for field outputs without field points); without field points the GUI keeps Free-Surface Amplitude on |
| Viewport | On a model of the meshed bodies with a placeholder frequency (a body's queries check that body alone, since `be7b89f`), per body: `Model::body_panels(id, role, true)` gives its elements (their counts by role in Details) and `Model::body_tessellation(id, role, true, n, n)` what is drawn, every role of the engine (wetted, interior surface, dipole, dipole damper, pressure surface, tank, plate) and then the control surface on its own (`role` "control_surface", asked for with `second_order/drift_control_surface`), each with its own n (4 ... 24, about 20 000 quadrilaterals): a low-order panel is one quadrilateral, a patch n x n on its exact surface, also a generated control surface or interior surface of a low-order model. Each quadrilateral has its role and the seven keys of its element (`PanelKey`: body, role, tank, owner, element, part, reflection); it is outlined where it bounds its element (a low-order panel only up to 25 000). The lids are drawn from the GUI's meshes |
| GUI state | `tool/hydrobem_gui`, `text` = JSON (`FILES.md`) |

A record has one role in a body: when the IRR, the dipole panels or the pressure surface take a
record range of the body's own mesh, the body's `ranges` are the records they leave
(`Model::set_surface(id, "mesh", "ranges", name, ranges)`), as the engine requires.

Meshes shared by several users (one GDF file read once) are one mesh object, named after their
first user. Lids without a mesh are kept in the GUI state alone.

The body's `ranges` and each surface's `ranges` are records of the same mesh, not a
partition that switching changes: a switched-off dipole, damper, pressure surface, tank or placed
lid keeps its records out of the body's reference, so they have no role while it is off (the
engine's rule; the hull has a gap there), as in a model written by Python.

The results number the enabled bodies (`Results::solved_model`): `SolveResult.bodies` are the
active ones. The locations of errors and warnings count every body of the model
(`SolveResult.model_bodies`).

Reading refuses, with the object path, what the GUI cannot represent:
a wetted surface that is not one range less its surfaces, surfaces of several record ranges, field points in different tanks or with different
grids at different depths, and negative frequencies or periods. Radii of gyration open as Ixx ...
Izz (I = m r |r|, with a warning) when `inertia_input` is `radii_of_gyration` and they are not zero. The limits ω = 0 and ∞ (omegas 0 and inf,
periods inf and 0) open as Added Mass Limits. `enabled = false` opens as Suppressed or a cleared
Enabled. A model without
a mass opens as Displaced with a warning. Inertia given about G or a point is moved to R by the
parallel-axis theorem.

## Queries

| GUI | Engine |
|---|---|
| GDF import | `Model::load_mesh` on a scratch model, read back as the GUI's mesh (`gui_core::mesh`); a trailing record of zeros is accepted |
| Geometry file (.hbg) | import: a file of version 2 (cad2bem's) is read as version 3 (`gui_core::hbg_upgrade`: the version, and the symmetry by its planes, `x` → `yz`, `y` → `xz`, `xy` → `xz_and_yz`) from a scratch copy, the mesh keeping the file's path; then `Model::load_mesh` of the `.hbg` path, with the file's discretization, else the rows of HOPatch's protocol 1 `NAME.report.json` (format `hopatch-report` 1, by patch name; GUI-side, not the engine's), else 4 x 4 spans of order 3 (quadratic) per patch; the patch names are the names of the mesh's `patches` children. Export: `Model::write_mesh(id, path.hbg, None, overwrite)` of the mesh written to a scratch model |

| Displaced volume, centre of buoyancy, waterplane area | `Model::body_hydrostatics` of a scratch model with the body alone (its wetted records) |
| Mesh Details: geometry and quality of a mesh alone | `Model::mesh_geometry`, `Model::mesh_geometry_quality` |
| Mesh tables: per stored record its centroid, normal into the fluid and area | `Model::mesh_elements` (one row per stored element, in the element numbering of the mesh) |
| Higher-order body outputs coloured on the hull | `Model::tessellation(Some("wetted,pressure_surface"), true, n, n)` of the solve's model; each quadrilateral to the nearest `BodyPoints` of its body and reflection (`BodyPointsIds`) |
| Interior surface preview (viewport) | `Model::body_tessellation(body, Some("interior_surface"), true, n, n)`: global coordinates, symmetry expanded |
| Field-point exclusion | `Model::classify_points` (`PointStatus`) on the model of the meshed active bodies and lids: the points in the sea at once (global), each tank's with the tank (its body's axes) |
| Wave number | `hb_wavenumber` (`hydrobem_engine::wavenumber`) |
| Memory estimate | `Model::memory_estimate` |

## Solve

`gui_core::solve::make_solve_request` builds a fresh model of the project on the UI thread, so the
editing project is never blocked. The model moves to a worker thread and solves there
(`Model::solve`); its stage reports (`Progress`) come back over a channel and wake the UI, the
progress shown is the engine's snapshot read every frame (`hb_model_progress`, `CancelToken::progress`),
and Stop cancels through the model's `CancelToken`. A failure keeps the engine's `Location` (body,
record, field point, lid, frequency), which the GUI shows and uses to select the body.

## Results

`gui_core::results::SolveResult` holds the engine's `Results` (with the model solved, for saving),
the bodies on the D axis from `QuantityId::Coordinates` (rows of body, kind 0 rigid / 1 generalized
/ 2 pressure, index), the per-body hydrostatics of the solve, the warnings and the run's meta data.
Shapes come from `Results::shape`; the views are in `RESULTS.md`.

| View | Quantity | Shape |
|---|---|---|
| Added Mass, Radiation Damping | `AddedMass`, `RadiationDamping` (f32) | [F][D][K] |
| Froude-Krylov, Scattering, Excitation, Motions | `ExcitationFroudeKrylov`, `ExcitationScattering`, `ExcitationDiffraction`, `Motion` (complex f32) | [F][H][D] |
| Free surface | `FieldPointsPressureRadiation` [F][P][K], `FieldPointsPressureIncident` / `Scattering` / `Diffraction` [F][P][H] (complex f32; elevation at z = 0) for the points in the sea; the points of a tank by `Results::select_complex32("tanks.pressure_radiation", body, tank)` [F][P_t][K] (radiation only: the waves do not reach a tank's fluid, its other parts are zero); laid out in grid order by `SolveResult::field_sources` | |
| QTF | `QtfQuadratic*`, `QtfPotential*` (`Difference`, `Sum`) [F][F][P][D] complex, P `QtfHeadingPairs`; `QtfNewman*Difference` [F][F][H][D] real (`MomentumConservation` [F][F][H][3]): `SolveResult::qtf_matrix` | |
| Mean drift | `DriftPressureIntegration`, `DriftControlSurface` [F][P][D] on the coordinates (the body's rigid rows), `DriftMomentumConservation` [F][P][3], `Drift*BodyAxes` [F][P][3B] (complex), P the heading pairs `DriftHeadingPairs` [P][2]: the GUI shows the pair of a heading with itself (real), and Heading Pairs the matrix of every pair (the lower triangle the conjugates) | |
| Axes | `Omegas` [F], `Headings` [H], `RadiationModeIds` [K] (one-based coordinate ids) |
| NumPy export | every quantity of `Results::available` as `Results::bytes` (the elements of any numeric type, the engine's layout) with `Results::shape` and `QuantityId::dims` / `unit` / `description`; of `Results::solved_model` (`hb_results_solved_model`) `Model::motion_equation` (`hb_model_motion_equation`, None when not ready) and each body's `position`, `yaw`, `rotation_center`, `trim` (`FILES.md`) | |
