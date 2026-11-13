# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

The first release, 0.3.0: nothing has been released before it, so this lists what the product is
rather than how it got there. The public history starts at this release (one commit); the history
of the work before it is not published.

### Added
- **The product**: one Python wheel, `hydrobem-gui`: the Python API `hydrobem_gui` (HydroBEM's
  Python package at `ee6c71b`, its PyO3 core replaced by `_core.py`, ctypes over the DLL), the GUI
  `HydroBEM.exe` (Rust, egui/eframe and wgpu) and HydroBEM's engine `hydrobem.dll` (HydroBEM
  0.3.0 at `ee6c71b`, C ABI 8, Rust API 56, model record 13, results format 5) in its `bin`;
  `hydrobem_gui.show(model)` and the `hydrobem-gui` command open the GUI.
- **The engine's trees**: the project tree in the engine's order (Meshes, Bodies, Lids, Fixed
  Dampers, Environment with Omegas or Periods and Headings, First Order with Field Points, Second
  Order, Options) and the results by the engine's global paths; Details labels are the engine's
  names in title case; project fields are named as the engine's items (`ENGINE_API.md`).
- **Inputs**: bodies with their meshes (GDF low and higher order, `.hbg` from cad2bem, analytic
  meshes made in the GUI), position, mass and inertia, interior surfaces, dipoles, dipole dampers,
  pressure surfaces, tanks, free-surface plates, control surfaces, generalized modes, couplings;
  lids, fixed dampers; depth, walls, channels; frequencies and headings; the first-order outputs
  and field points (Cartesian, polar, body-fitted grids, point lists, tanks); the drifts, the QTFs
  and the potential QTF's free surface; options and the memory estimate (`REQUIREMENTS.md`).
- **Solves** on a worker thread with the engine's progress: its phase, its frequency workers
  (what each is on, Idle when it has nothing left) and the steps of the second order; Stop,
  engine warnings and errors located in the tree.
- **Results**: tables and graphs of every quantity by its layout (`Results::shape`, F/H/D/K/P),
  the motions, field points, lids, pressure surfaces, Kochin, drifts, the QTF as a table, heatmap
  or row, hydrostatics; the 3D viewport with the free surface, animation and the wave amplitude
  (`RESULTS.md`).
- **Files**: projects are HydroBEM model files (`.hbm`) with the GUI state inside, results its
  results files (`.hbr`); Save, Save As, Open Results, Export WAMIT (s / rad/s) and NumPy (.npz), a table as CSV (its menu); results
  marked out of date once the inputs change (`FILES.md`).
- **`hb.save_npz`** (2026-11-14): the Python package writes the GUI's NumPy archive
  (`hydrobem_gui.npz`, this package's own module beside `gui.py`, not HydroBEM's): the same members
  and `meta` texts as File → Export → NumPy; a Rust test keeps the texts of the two in step.
- **Interior surface Detect** (2026-11-14): a new body's interior surface is Detect, the engine's
  default (the wetted records in z = 0, an analytic mesh's lid among them), and a body with a patch
  mesh offers it too (and Waterline when trimmed), as the engine takes them in the higher order; the
  viewport draws the records Detect takes while the engine cannot build the problem.
- **Viewport** (2026-11-14): a right drag pans (also a middle drag or Shift + left drag); a right
  click without a drag opens the viewport's menu.
- **Solve anyway** (2026-11-13): Solve in the memory dialog (the estimate above the free memory)
  now solves: the engine's own memory check is turned off in that solve's model
  (`gui_core::solve::without_memory_check`; the project keeps it on). It stayed on before, so the
  engine refused the solve for the same reason.
- **Code layout** (2026-11-13): the four largest modules split into folders without a change of
  behaviour: `gui_core::engine_model` (writing, reading, queries), `gui_core::scene` (bodies, water,
  overlays, the builder), the GUI's `details` (body, mesh, analysis, tables) and `app` (menus,
  files, solve, edits, modals); `docs/ARCHITECTURE.md` maps them.
- **Documentation for users** (2026-11-13): the GUI's manual (`docs/manual/`) and the Python API's
  guide and reference (`docs/python/`), HydroBEM's user documentation copied with the package by
  `scripts/copy_hydrobem_docs.py` (the theory and validation stay in HydroBEM's documentation).
- **WAMIT demonstration models** (2026-11-13): WAMIT's demonstration meshes are not distributed;
  `cargo xtask wamit <folder>` writes HydroBEM models of WAMIT's demonstration runs into a folder
  that holds those meshes.
- **System fonts** (2026-11-13): egui is built without its bundled fonts; the window reads Segoe
  UI, Consolas, Segoe UI Symbol, Segoe UI Emoji and Malgun Gothic from Windows, and HydroBEM.exe
  carries no font of its own.
- **Exports** (2026-11-13): an export keeps the file name the user chose (an extension is added only
  when the name lacks it); a CSV export that cannot replace its file (open in another program)
  leaves no temporary file behind; the NumPy export refuses a body whose position, yaw or rotation
  centre the solved model does not give, instead of writing zeros.
- **The C runtime as a DLL** (2026-11-13): as HydroBEM's since `ee6c71b`, the programs are built
  without the static C runtime (`.cargo/config.toml`: `+avx2,+fma`); HydroBEM.exe and the engine use
  the Visual C++ runtime Python installs and the Universal C Runtime of Windows, and the wheel
  carries no copy of the Microsoft runtime. `hydrobem-gui` and `show()` put Python's runtime folder
  first on the GUI's PATH; HydroBEM.exe started directly needs the Visual C++ runtime installed.
- **Notices** (2026-11-13): AI assistance acknowledged in `NOTICE`; the WAMIT trademark notice
  (README, `NOTICE`, `FILES.md`); Kim and Yue (1990) beside Hulme (1982) as the published values of
  the Python tests' data.
- **NumPy export** (2026-11-13): File → Export → NumPy (.npz) writes every quantity of the results
  under its engine name, in the engine's type and dimensions, with the bodies' names, the engine's
  motion equation (rigid inertia, stiffness, damping, 6B x 6B) and each body's position, yaw,
  rotation centre and trim of the model as solved, and a `meta` JSON (run, warnings, each key's
  axes, unit and description); `numpy.load` reads it without pickling, and a time-domain
  calculation needs nothing else. The GUI writes it (`gui_core::npz`, `gui_core::zip`); `Results::bytes`,
  `Results::solved_model` and `Model::motion_equation` in `hydrobem-engine`. File → Export → Excel is removed: it wrote only the table shown, which the
  table's Export CSV does; File → Export now holds the results as files only.
- **Excitation → Haskind** (2026-11-11): the engine's `excitation.haskind`, the whole exciting force
  by the Haskind relation, read through the radiation columns (`SolveResult::haskind_series`).
  Hydrostatics is the first node of Results; Pressure Mode Damping (the engine's top-level
  `pressure_mode_damping`, the additional damping of the pressure modes) is under Pressure
  Surfaces, after Elevation. The added mass limits are part of Added Mass (rows A(0) and A(∞),
  two points on the graph, the 6 × 6 blocks as tabs); the node Added Mass Zero / Infinite
  Frequency is removed.
- **QTF Total** (2026-11-11): Results → QTF → Method "Total, <quadratic> + <potential>" for every
  quadratic and potential part solved, the GUI's sum of the two (`SolveResult::qtf_sum_of`; the
  engine gives the parts only), as table, heatmap or row.
- **HydroBEM `e128a8e`** (2026-11-12, C ABI 7, API 52): the progress table by the engine's own
  frequency workers (what each is on, Idle when it has nothing left, Done) instead of rows the GUI
  assigned; the second-order steps (quadratic QTF rows, the potential QTF's sub-problems, fits,
  outer integral, pairs) with their shares, and the status bar by phase (Second order: the
  running step; Writing the results), so the QTF phase is no longer a silent 100%. The
  second-order loads of a body (drifts, QTFs) have their moments about its rotation centre R, as
  their Axes rows now say; a body may be massless (mass 0). The Python package and tests copied
  at `e128a8e` (`WorkerProgress`, `SecondOrderProgress`, `test_multiple_bodies.py`,
  `test_second_order_rotation_center.py`).
- **HydroBEM `51e7cb6`** (2026-11-12, C ABI 8, API 53, model record 13): the viewport draws every
  element from the engine's tessellation (`hb_body_tessellation` with roles), each quadrilateral
  with its role and the seven keys of its element, instead of panel corners and the GUI's
  matching of rows by order and count: a generated control surface (cylinder, ellipsoid,
  automatic) and a generated interior surface are drawn on their exact curved surfaces, also in a
  low-order model (the control surface was a box of its patch corners). The engine's names in the
  binding (`element` for record, `MeshElements`, `PanelKey`, `PanelRole::Wetted`, ...) and the
  renamed C functions; the test models written again (record 13). Free-surface plates also work
  at ω = ∞ and with the pressure drift and quadratic QTFs on walls (engine).
- **Generated and tank free-surface plates** (2026-11-12): a body's Free Surface Plate takes
  Source "Generated in a Waterline" (the engine's plate inside a trimmed hull's waterline loop:
  Inside X, Y and Waterline Of, this body or another, which follows its renaming and deletion),
  and a Tank has Free Surface Plates (records of the body mesh or a mesh of its own at the tank's
  surface level); both round-trip through the model file and are drawn by the engine. Test
  models of `trimmed_moonpool_cover` and `run_covered_tank.py` (half covered).
- **HydroBEM `ee6c71b`** (2026-11-13; HydroBEM's public repository, whose history starts at 0.3.0):
  the C runtime linked as a DLL (above); the DLL reports `961a48fbb552-dirty`, built from the tree
  of `ee6c71b` before that commit, which changed only the build flags it was built with and
  documents. C ABI 8, model record 13 and results format 5 unchanged, header identical.
- **HydroBEM `80bb0e5`** (2026-11-13, of its development history; C ABI 8, model record 13 and
  results format 5 unchanged, Rust API 56): the end of HydroBEM's D-211 restructuring, the
  results unchanged to the byte. The checks before a solve are one step: an unbuildable free
  surface is refused before any frequency is solved (it was after the first order), so a solve
  can fail before its first progress report, which the GUI ends like any failed solve (a test of
  it). The Python package and tests are unchanged; the user documentation copied again.
- **HydroBEM `2d6e8d4`** (2026-11-13, C ABI 8, API 55, model record 13 and results format 5
  unchanged; the DLL reports `2d6e8d4306de`, clean): HydroBEM's D-211 restructuring of the engine
  (rules by area, one scoped problem, the route as a type), the C header changed in a comment only.
  The Python package and tests merged from `be7b89f`: a role argument of an empty list is refused
  (`ValueError`) instead of taken as the default roles; new tests (`test_rule_messages.py`, whose
  two checks of the engine's sources skip here, and additions to eleven others). The examples'
  `floating_owc_cover` docstrings and README name the validation pages of the related cases.
- **HydroBEM `be7b89f`** (2026-11-12, C ABI 8 unchanged, API 55, model record 13; the DLL reports
  `619f7b4-dirty`, its engine sources those of `be7b89f`): `hb_model_check` also refuses what no
  solver route takes (`HB_ERR_UNSUPPORTED`, valid but not available), and a body's queries check
  that body alone, so the viewport draws a body while the model is incomplete. Generated
  free-surface plates (`inside`, `body`) and plates of a tank are new in the engine (in the GUI
  below). The Python package, its tests and the examples copied (two examples new:
  `trimmed_moonpool_cover`, `fish_tank_cylinders/run_covered_tank.py`); the test models written
  again. Less memory for a solve with large outputs (engine).
- **Test models from the examples** (2026-11-12): the Rust tests read HydroBEM's example cases
  (cylinder, beside a wall and in a channel, two and three truncated cylinders, the analytic
  truncated cylinder, the OWC chamber with its pressure surface or a damping lid, the cylinder with
  a thin heave plate, the dock, the fish tanks) instead of the GUI's former cases on the topics of
  the WAMIT demonstration runs (barge, moonpool, spar with strakes) and an OC4 platform. A
  patch mesh read from a GDF file is the analytic truncated cylinder's
  (`truncated_cylinder_gdf.hbm`), a section in parts the three cylinders as one body
  (`three_columns.hbm`). `scripts/make_testdata.py` builds them from the examples' `build()`,
  `cargo xtask testdata` saves them with the GUI's state into the crates' `tests/data`
  (`testdata/` and `scripts/make_test_meshes.py` removed).
- **Examples**: HydroBEM's examples at `80bb0e5` (15 cases as Python scripts importing
  `hydrobem_gui`, writing `.hbr` files the GUI opens), shipped in `dist/` and the wheel.
- **Tests and tools**: the Rust tests of the GUI's mapping, files and UI with a smoke test
  through egui_kittest; HydroBEM's Python tests on the ctypes binding without their slow
  references; test models of HydroBEM's example cases in each crate's `tests/data`
  (`scripts/make_testdata.py`, `cargo xtask testdata`);
  `scripts/import-engine.ps1`, `scripts/check-env.ps1`, `cargo xtask dist`,
  `scripts/build_wheel.py`, `cargo xtask licenses`. The engine's accuracy is HydroBEM's to test.
