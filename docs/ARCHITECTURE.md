# Architecture

## Overview (2026-11-04)

- Application: Rust 2024 (toolchain 1.99.1), egui/eframe 0.37 with wgpu 30, Windows x64. Graphs:
  egui_plot 0.38. 3D: an own wgpu renderer (offscreen 4x MSAA with depth) in an egui paint callback.
- Engine: HydroBEM's `hydrobem.dll` (HydroBEM 0.3.0 at `ee6c71b`, API 56), the C ABI of its engine
  (ABI **8**, `engine/hydrobem.h`), built by HydroBEM, imported into `engine/` (`engine/IMPORT.json`) and loaded by `HydroBEM.exe` at
  run time from beside it (since 2026-11-04; before, the engine crate was vendored and compiled
  in). `crates/hydrobem-engine` is the safe binding. The earlier C++ GUI, which bound C API 21, was
  replaced by the Rust rewrite (merged 2026-11-01).
- Product: one Python wheel `hydrobem-gui` = the Python API `hydrobem_gui` (HydroBEM's Python
  package copied at `ee6c71b`, its PyO3 core replaced by ctypes over the DLL: `python/`), the GUI
  and the DLL (`scripts/build_wheel.py`). Python is required; HydroBEM stays the development
  project (Rust + PyO3), this repository only copies its DLL (and, by hand, its Python code).
- CAD: cad2bem, a separate program with its own GUI, converts CAD files into geometry files
  (.hbg); this GUI imports those as any mesh file and does not run cad2bem (until 2026-11-07 it
  did, through its command line).
- Files: projects are the engine's model files `.hbm`, results its `.hbr` (`FILES.md`). The examples are HydroBEM's Python scripts (`examples/README.md`); the test
  models (`crates/*/tests/data`) are HydroBEM's example cases, `.hbm` written by
  `scripts/make_testdata.py` and `cargo xtask testdata`.
- The features `REQUIREMENTS.md` describes, plus zoomable graphs, the
  Auto diffraction formulation, persisted view settings and `HydroBEM.exe <file>`.
- Checks: clippy with `-D warnings`; the Rust tests (the project and its engine model, the results'
  layouts, files, every test model, the GUI smoke test through egui_kittest with real wgpu
  rendering; the engine's physics is HydroBEM's to test); the Python tests (`python/tests`,
  HydroBEM's, about 800 without its slow references) and
  `scripts/gen_model_classes.py --check` (`DEVELOPMENT.md`).

What comes next is in `ROADMAP.md`. Where documents disagree, the order of authority is:

1. The engine's C ABI (`engine/hydrobem.h` and HydroBEM's `docs/reference/c-api.md`, `inputs.md`,
   `results-and-files.md`) for the model and the shapes.
2. Executable tests.
3. `REQUIREMENTS.md` and `RESULTS.md`.
4. Current implementation for undocumented details.

## Boundaries

```text
HydroBEM.exe                         crates/hydrobem-gui (egui/eframe, wgpu)
  app ── tree, details, results_view, output (table + egui_plot), viewport (wgpu renderer)
        │
hydrobem-gui-core (no UI)            crates/gui-core
  project, mesh, engine_model (build_model / read_model), files (.hbm, .hbr, WAMIT and NumPy
  exports), npz and zip (the NumPy archive), solve, results (views), field_points, scene
  (viewport geometry), format
        │
hydrobem_gui (Python, ctypes)        python/hydrobem_gui
  Model, objects, solve, Results (numpy), show() → HydroBEM.exe --untitled, save_npz() (the
  GUI's NumPy archive)
        │ C ABI
hydrobem-engine (no UI)              crates/hydrobem-engine
  the safe binding of hydrobem.dll, loaded at run time: Model, Value, Results, queries, progress
        │ C ABI (engine/hydrobem.h)
hydrobem.dll (HydroBEM, imported)    engine/
  HydroBEM's engine: model document, rules, BEM assembly, solve, results, files
```

`gui-core` has no UI dependency, so the model, engine and result logic are testable without a
window and could back a command-line tool. `hydrobem-gui` is a library with a thin `main.rs`, so the
smoke test drives the same `App` the executable shows. `xtask` writes the test models, the
licence notices and `dist/`.

## Source map

| File | Responsibility |
|---|---|
| `gui-core/project.rs` | The project as the engine's objects: `Body`, `DampingLid`, `Environment` (with the domain), `Options`, `FirstOrder`, `SecondOrder`, `Waves`, `FieldPoints` (the GUI's grid of them), the `fixed_dampers`; the project container. Their fields carry the engine's names |
| `gui-core/mesh.rs` | Low-order meshes read by the engine (`load_mesh`), mesh objects ↔ GUI meshes, symmetry expansion, checksums |
| `gui-core/engine_model/` | The project as an engine model and back: `write.rs` (`build_model`, the one path to the engine), `reader.rs` and `read.rs` (`read_model`), `queries.rs` (hydrostatics, panels by role, mode shapes, interior previews), `mod.rs` (the GUI state JSON, build options) |
| `gui-core/field_points.rs` | Submitted field points: the grid points the engine accepts (`classify_points`) |
| `gui-core/files.rs` | Model and results files through the engine, atomic saves, the unsaved-change fingerprint; the exports: WAMIT-format files (the engine's) and the NumPy archive (`export_npz`) |
| `gui-core/npz.rs`, `zip.rs` | The NumPy archive: `.npy` headers and arrays, and the ZIP of stored members (ZIP64) it is written as |
| `gui-core/analytic.rs` | Analytic patch meshes made in the GUI from the engine's named shapes and their dimensions |
| `gui-core/animation.rs` | The animation of a linear response (the real part of its amplitude times e^{iωt}, a display scale) |
| `gui-core/field_layout.rs` | The field-point grid layouts: Cartesian, Polar, Body-Fitted rings about a body's section |
| `gui-core/hbg_upgrade.rs` | Geometry files of version 2 read as version 3 (cad2bem's files) |
| `gui-core/patch_report.rs` | The report beside a geometry file (`NAME.report.json`): per patch its checks and discretization |
| `gui-core/solve.rs` | The solve request (built on the UI thread), the worker thread, progress, cancel, located errors |
| `gui-core/workers.rs` | The progress table by worker: the engine's frequency workers and second-order steps, their status text |
| `gui-core/results.rs` | The D-axis layout, coefficient / FHD / field indexing, the dynamic Total, units, warning names |
| `gui-core/scene/` | Viewport geometry: `mod.rs` (`build_scene`, the scene types, axes), `bodies.rs` (bodies by role, IRR, trim cut, lids, normals), `water.rs` (free surface, walls and sea bed), `overlays.rs` (body and field overlays, heatmap, field plane, markers), `builder.rs` (the geometry builder, colours) |
| `gui-core/format.rs` | Numbers for tables and Details, CSV numbers and text, parsing typed numbers |
| `gui-core/engine_info.rs` | The engine DLL's import record and the build it reports |
| `hydrobem-engine/ffi.rs` | The C ABI: its structs and the table of the DLL's functions, loaded once (`HYDROBEM_DLL`, beside the program, one folder up), ABI checked |
| `hydrobem-engine/table.rs` | The DLL's tables (named columns), statuses and errors, texts |
| `hydrobem-engine/model.rs` | `Model`: objects, items, meshes, queries, solve (progress callback, cancel and progress snapshot from any thread) |
| `hydrobem-engine/results.rs` | `Results`: views of the quantities the DLL holds, meta data, diagnostics |
| `hydrobem-engine/types.rs`, `codes.rs` | Values, query answers, progress, schema, build; quantity, role, status codes |
| `hydrobem-engine/error.rs` | The engine's errors and the place they name; the diagnostics (warnings) of a model |
| `hydrobem-gui/app/` | `mod.rs` (`App`, `Settings`, the frame), `menus.rs` (menus, shortcuts, status bar, dropped files), `files.rs` (open, save, exports), `solve.rs` (Run/Stop, progress), `edits.rs` (tree and Details actions, the derived data and scene), `modals.rs` (dialogs) |
| `hydrobem-gui/tree.rs` | Tree nodes, context menus, the results subtree |
| `hydrobem-gui/widgets.rs` | Details rows (commit-on-Enter/focus-loss editors, choices, file drop), section headers, tree rows |
| `hydrobem-gui/details/` | Details and tables of the input nodes: `mod.rs` (dispatch, the engine's derived values cached by the inputs, mesh choice), `body.rs` (a body and its surfaces, tanks, matrices), `mesh.rs` (lids, library meshes, analytic shapes), `analysis.rs` (environment, waves, first and second order, field points, options), `tables.rs` (the generated tables and their edits) |
| `hydrobem-gui/results_view.rs` | Details, tables, graphs and heatmap values of the result nodes |
| `hydrobem-gui/output.rs` | Table (selection, copy, CSV, editing) and graphs, split or tabbed |
| `hydrobem-gui/viewport/` | Orbit camera, picking, overlays, the wgpu renderer |
| `hydrobem-gui/style.rs` | The light theme and the fonts, read from Windows (the executable carries none) |
| `hydrobem-gui/system.rs` | What the operating system reports (memory, processors) |

## Data flow

egui is immediate-mode: every frame draws the whole window from the state in `App`.

1. Details widgets edit `Project` (owned by `App`) directly; a text or number field commits on
   Enter or focus loss.
2. Each frame `App` takes the project's fingerprint (`files::project_fingerprint`). When it changed,
   the engine's derived values are recomputed (`details::Derived`: field-point classification,
   memory estimate, interior previews) and the scene is rebuilt (`scene::build_scene`); otherwise
   both are reused. The fingerprint also tells unsaved changes.
3. Run waits one frame (an open editor commits), builds `SolveRequest` (a fresh engine model), checks
   the memory estimate, and starts `SolveHandle`: the model solves on a worker thread, its stage
   reports come over a channel and wake the UI, and each frame reads the engine's progress snapshot
   (`SolveHandle::progress`). Stop cancels through the model's `CancelToken`.
4. The result (`Arc<SolveResult>`) holds the engine's `Results`; the tree's results subtree shows what
   is available. Result views index it with result-time body names and field grid, not the possibly
   edited current project.
5. The viewport passes the scene (an `Arc`, uploaded to the GPU once per scene) and the camera to a
   paint callback; the renderer draws offscreen with its own depth and MSAA and copies the image into
   egui's pass.

## Coordinate and ownership rules

- Body mesh coordinates are body-local; Position places the body globally; Reference (R) is body-local.
- The engine accepts yaw only. Roll/pitch are display-only and rejected before a solve.
- Additional matrices, IRR, dipole panels, pressure surfaces, modes, mesh, position, and inertia are body-owned.
- Damping lids, the environment, the waves, the field points, the options, the outputs and the results are project-wide.

## GUI-side calculations (not engine features)

Symmetry expansion for display, the dynamic free-surface Total, the heatmap and its interpolation between the field points
(display only), panel outlines and the walls/sea bed drawing, the unsaved-change fingerprint. All
physics — mesh reading, hydrostatics, field-point rules, wave numbers, the solve — is the engine's.
