# Development

## Requirements

- Windows 10/11 x64 with a processor with AVX2 and FMA.
- Rust via rustup; `rust-toolchain.toml` pins the toolchain (1.99.1) with clippy and rustfmt.
- The Visual Studio 2022 Build Tools (the MSVC linker and the Windows SDK).
- A GPU adapter for the viewport and the smoke test (D3D12 or Vulkan; WARP works).
- Python 3.11 or later with numpy, and for the tests pytest and scipy (ruff to lint), in an
  environment of this project only (not HydroBEM's). With conda, from conda-forge:

  ```powershell
  conda create -n hydrobem-gui --override-channels -c conda-forge python=3.11 numpy pytest scipy ruff
  conda activate hydrobem-gui
  ```

  The built wheel is installed into it as well (`pip install --no-deps --force-reinstall
  dist\hydrobem_gui-*.whl`) for the `hydrobem-gui` command and `show()`; the tests still run on the
  source tree. The installed wheel is a copy: build and install it again after an engine update
  (below) or a change a user of the environment should see.

  The environment must be activated (or its `Library\bin` on PATH): numpy's own DLLs are found
  there, and without them numpy stops the process. The package is not installed for development:
  the tests put `python/` on the path (`python/tests/conftest.py`); in VS Code, `pyrightconfig.json`
  does the same for Pylance (hover help and completion of `hydrobem_gui`).
- The engine DLL in `engine/hydrobem.dll` (not committed): build it in HydroBEM (`cargo build
  --release -p hydrobem-c`) and import it (Engine updates below).

Every crate comes from crates.io at the version pinned in `Cargo.toml` / `Cargo.lock`. The build
copies `engine/hydrobem.dll` beside the programs (`target/<profile>/`); `HYDROBEM_DLL` names
another DLL to load.

## Build and run

```powershell
cargo build                     # dev: our crates at opt-level 1, the dependencies at 3
cargo run -p hydrobem-gui       # HydroBEM.exe (a console window in dev builds for messages)
cargo build --release           # fat LTO, one codegen unit: several minutes
```

`target/release/HydroBEM.exe [file.hbm|file.hbr]` is the application. A quick optimized build
without LTO: `cargo build --profile quick`.

## Checks before a commit

```powershell
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The tests:

| Test | What it checks |
|---|---|
| `gui-core` unit tests | project rules, mesh reading and symmetry, the engine hydrostatics, the model round trip, inertia moves, grid rebuilding, field-point exclusion and walls, result units, formatting, scene layers and heatmap range, the import record |
| `gui-core/tests/solver.rs` | solves: box, RAO and Total, two bodies, pressure coordinates, x wall = mirror image, channel, lid, interior previews, warnings, located errors, cancel, memory estimate and solving anyway (the engine's check off) |
| `gui-core/tests/files.rs` | model files: every test model, round trips, suppressed bodies, the fingerprint, a Python-like model, refused inputs; results files; the NumPy export (members, types, shapes, bytes, meta; motion equation and placement of three bodies, a trim, a pressure mode; the texts of the Python module `npz.py` the same as the GUI's) |
| `gui-core/tests/features.rs` | the engine objects beyond the basics through the model and back and solved: tanks, dipole dampers, control surfaces and their drift, trim, body points, Haskind excitation, pressure and momentum drift, field velocity, a channel's reflection; Panel Size of a patch mesh read from a GDF |
| `hydrobem-gui` unit tests | CSV and copy text, the camera and picking, the export file names |
| `hydrobem-gui/tests/body_fitted.rs` | the Body-Fitted Grid of a body whose section is in parts (three columns of one body): the convex outline and the rings between the columns |
| `hydrobem-gui/tests/file_drop.rs` | files dropped onto the window: a model opens, results open as results, anything else is named in the status bar |
| `hydrobem-gui/tests/mesh_tables.rs` | the mesh library's tables (panels, vertices, patches, corners), highlighting from the table and picking back, a geometry file with cad2bem's report, Panel Size |
| `hydrobem-gui/tests/settings.rs` | the settings in `app.ron` as a user's hold them (an older version's, without the choices added since) |
| `hydrobem-gui/tests/table_input.rs` | the output table by pointer and keyboard events: click, drag, row and column headers, Ctrl+C |
| `hydrobem-gui/tests/viewport_mouse.rs` | the viewport by pointer events: a right drag pans without the menu, a right click opens it, a left drag rotates |
| `hydrobem-gui/tests/wave_amplitude.rs` | the wave amplitude of the result views: which values it scales; not saved |
| `hydrobem-gui/tests/solve_progress.rs` | while solving: the status bar counts the frequencies done; the table beside Run shows each worker's frequency, ω and percentage; Stop closes it; a solve the engine refuses before any progress ends with its message |
| `hydrobem-gui/tests/stop_and_exit.rs` | Exit while solving asks; Cancel goes on; Stop and Exit stops the solve, then the unsaved prompt, then the window closes |
| `hydrobem-gui/tests/file_menu.rs` | File menu: Save, Save As, Export (WAMIT and NumPy, once solved); Save writes the `.hbr` once solved, the `.hbm` before or after a change; results out of date marked (`06c`, `06d` screenshots) |
| `hydrobem-gui/tests/open_untitled.rs` | `--untitled`: a model opens as a new unsaved project, its file not kept nor remembered |
| `hydrobem-gui/tests/smoke.rs` | the whole app through egui_kittest with wgpu: opens, edits, solves, results views, heatmap, results and model files, the unsaved prompt, recent files, test models; screenshots in `target/smoke/` |

After a UI change, review the screenshots in `target/smoke/`.

The user manual (`docs/manual/`) takes its figures from these screenshots (`docs/manual/images/`).
Retake a figure after a UI change it shows. The status bar of a screenshot names the file opened
or saved with its absolute path, which holds the machine's user and folders: before a figure is
committed, its status message shows the file name alone (`Opened cylinder.hbm`).

The Python package (`python/hydrobem_gui`, on the DLL of `engine/`) and its tests:

```powershell
cd python
python -m pytest                   # about 800 tests, two minutes
python ..\scripts\gen_model_classes.py --check
```

`HYDROBEM_TEST_INSTALLED=1` runs them on an installed wheel instead of the source tree.
`ruff check python scripts` lints with the settings of `python/pyproject.toml`.

The tests check what this repository adds over the engine: the GUI's project and its engine
model, the reading of the results by their layout, files, the UI, and the ctypes binding
(`_core.py`, through HydroBEM's Python tests). The engine's physics is HydroBEM's to test: no test
here compares with published or analytic values or with WAMIT, and the copied tests marked `slow`
(HydroBEM's convergence studies and references) stay in the copied files but are not run
(`python/pyproject.toml`).

The tests read their models and meshes from their crate's `tests/data`
(`crates/gui-core/tests/data/README.md`). `examples/` is a public folder for users, not tests: no
test reads it; `cargo xtask dist` ships it and `scripts/make_testdata.py` builds the test models
from its cases.

## Engine updates

```powershell
./scripts/import-engine.ps1 -Source ..\HydroBEM
cargo test --workspace
cd python; python -m pytest; python ..\scripts\gen_model_classes.py --check
```

The script replaces `engine/` (the DLL, `hydrobem.h`, the licences) and its `IMPORT.json`, and
warns when the DLL is older than HydroBEM's last commit; never edit `engine/` by hand, and never
build HydroBEM from here. When the ABI version changes (`HB_ABI_VERSION` of `engine/hydrobem.h`),
read HydroBEM's `docs/reference/c-api.md` and `CHANGELOG.md` and update both bindings: the Rust
one (`crates/hydrobem-engine`: `ABI_VERSION`, the functions and the table columns it reads) and the
Python one (`python/hydrobem_gui/_core.py`: `ABI_VERSION`, `_SIGNATURES`, the columns), then
`docs/ENGINE_API.md`. A schema change (new items or object types) also needs
`python scripts/gen_model_classes.py` to rewrite `_classes.py`.

Then build the wheel (`cargo build --release`, `cargo xtask dist`, `python scripts/build_wheel.py`)
and install it into this project's environment, so that `hydrobem-gui` and `import hydrobem_gui`
there use the new engine too. Then check the environment against the repository (read only; it
exits 1 when the installed wheel is not what `engine/IMPORT.json` and `target/release` say):

```powershell
./scripts/check-env.ps1 -Python <env>\python.exe
```

## Conventions

- Work on a branch; one behavioural concern per commit, with its documents and `CHANGELOG.md`
  (`[Unreleased]`). Commit messages: an imperative summary line of at most 72 characters; the body
  says why and names any engine-mapping, result-layout or requirement change.
- Rust is formatted by `rustfmt.toml` (120 columns). Plain data and `Arc` for shared meshes and
  results; the UI state lives in `App`. Comments explain intent or a constraint that is not
  obvious, not what the next line does.
- Logic tests go in `crates/gui-core` (unit tests beside the code, integration tests in
  `crates/gui-core/tests/`), UI behaviour in `crates/hydrobem-gui/tests/` and the smoke test. Add
  one for every new engine option, result mapping or model rule; test what the GUI does with the
  engine, not the engine's accuracy. Never weaken a test to make it pass: fix the code or document
  the limitation.

## Examples, test models and the WAMIT demonstration models

`examples/` holds HydroBEM's examples (its `examples/`, the tracked files: the scripts and the
README, not `out/`), copied at HydroBEM `2def9bb` (2026-11-14) with `import hydrobem as hb` → `import
hydrobem_gui as hb` in each script and the README written for this package. Copy them again when
HydroBEM's change (with the Python package at the same commit), and run each script on the
installed wheel: it ends with `check: ... ok`.

The test models are HydroBEM's example cases: `scripts/make_testdata.py` imports each example
script's `build()` (from `examples/`), sets the outputs, frequencies and headings the tests need,
and writes the models and a few mesh files to `target/testdata`; `cargo xtask testdata` opens each
in the GUI (printing what the GUI adapts, as radii of gyration to inertia) and saves it with the
GUI's state into the `tests/data` of the crates that read it
(`crates/gui-core/tests/data/README.md`). Run both again when the examples or the model record
change, then the tests.

```powershell
python scripts/make_testdata.py      # target/testdata, from examples/ (the hydrobem-gui environment)
cargo xtask testdata                 # crates/*/tests/data from them
```

`cargo xtask wamit <folder>` writes HydroBEM models of WAMIT's v7.5 demonstration runs (`test01.hbm`,
...) into a folder that holds WAMIT's demonstration meshes (`test01.gdf`, ...). The meshes are not
in this repository (their terms do not allow redistribution) and no test reads the models.

## Distribution: the wheel

```powershell
cargo build --release
cargo xtask dist
python scripts/build_wheel.py
```

`cargo xtask dist` stages `dist/`: `HydroBEM.exe` (the GUI subsystem: no console window),
`hydrobem.dll`, `examples/` (the scripts and their README), `licenses/hydrobem/` (the engine's licence
files), `licenses/crates/` (the licence texts of every crate compiled in, or the standard texts
of its licence expression when it ships none; no font is compiled in), `LICENSE`,
`NOTICE` and `THIRD_PARTY_NOTICES.md`.
`scripts/build_wheel.py` (standard library only) packs them with `python/hydrobem_gui` into
`dist/hydrobem_gui-<version>-py3-none-win_amd64.whl`: the package with `bin/HydroBEM.exe`,
`bin/hydrobem.dll` and `examples/`, the notices in its `.dist-info/licenses/`, the `hydrobem-gui`
command (gui-scripts) and the dependency on numpy. The version is `python/pyproject.toml`'s. One
wheel serves every Python from 3.11: it holds no compiled Python. `cargo xtask licenses`
regenerates `THIRD_PARTY_NOTICES.md` after a dependency changes.

## The Python package

`python/hydrobem_gui` and `python/tests` are HydroBEM's Python package and tests
(`python/hydrobem`, `tests/python`, `tests/data`) copied at HydroBEM `34adff1` (2026-11-04) and brought to `2def9bb` (HydroBEM's public repository) (API 57, C ABI 8, model record 13, 2026-11-14), renamed,
with the extension module (PyO3) replaced by `_core.py`: the same classes and functions over the
DLL's C ABI with ctypes (`Results.selection` by `hb_results_selection`, the check of
`Model.new_results` by `hb_results_check`). HydroBEM's later changes are copied by hand when wanted; nothing is shared
at build or run time. `scripts/gen_model_classes.py` (copied as well) writes `_classes.py` from the
DLL's schema (HydroBEM's also writes its `docs/reference/inputs.md`; this copy writes only
`_classes.py`); `scripts/gen_cylinder_reference.py`, `gen_cylinder_array_reference.py` and
`gen_sphere_array_reference.py` (HydroBEM's semi-analytic references) are copied too, for
`test_free_surface_plates.py` and `test_multiple_bodies.py`. This repository's own, not copied:
`gui.py` and `npz.py` (`hb.show`, `hb.save_npz`) with `test_gui_runtime.py` and `test_npz.py`. Left out: the tests of the extension module, of the C header with a C compiler, of
HydroBEM's example script, `test_example_references.py` (it reads `examples/`, which no test here
reads), and the two tests of `test_result_views.py` on HydroBEM's
`gen_result_views.py` and its reference document. Changed: the progress polls from another thread (`test_progress.py`, three places) start with
the solve (a poll before the thread reaches the engine is the empty one), and the engine's version
is checked apart from the package's. The headers of the reference data in `python/tests/data` name the scripts that made them
(`gen_green_*`, `extract_hulme.py`, ...); those scripts stay in HydroBEM.

To bring HydroBEM's later Python changes over (`git diff 2def9bb <new> -- python tests` in HydroBEM
shows them):

1. Copy the changed files of `python/hydrobem/` into `python/hydrobem_gui/`, except `_core.pyd`;
   keep this repository's `_core.py`, `gui.py`, `npz.py` and `__init__.py` (merge a change of
   HydroBEM's `__init__.py` by hand: it exports `show`, `exe_path`, `dll_path`, `save_npz` and has
   its own version).
2. Copy the changed tests of `tests/python/` into `python/tests/` and `tests/data/` into
   `python/tests/data/`, leaving out `test_extension_module.py`, `test_c_abi.py`,
   `test_examples.py`; keep the two changed tests above.
3. Rename the imports: `import hydrobem as hb` → `import hydrobem_gui as hb`, `from hydrobem` →
   `from hydrobem_gui`, `hydrobem._core` / `hydrobem.errors` → `hydrobem_gui.…`; test data paths are
   `Path(__file__).resolve().parent / "data"`.
4. A new method of HydroBEM's extension module needs its ctypes counterpart in `_core.py`.
5. Copy the user documentation again at the same commit:
   `python scripts/copy_hydrobem_docs.py <HydroBEM checkout> <commit>` writes `docs/python/guide/`,
   `docs/python/reference/` and `references.md` (the package renamed, the installation section
   this wheel's, a link to a document not copied left as its text or, with `--url`, linked into
   HydroBEM's published documentation); `docs/python/README.md` is this repository's own.
6. Run the Python tests and `gen_model_classes.py --check`; record the new commit here, in
   `THIRD_PARTY_NOTICES.md` (through `crates/xtask`) and in `CHANGELOG.md`.

The DLL and the Python code should come from compatible HydroBEM commits: the Python package may
only call what the DLL's ABI has, which the tests check.

## A release

1. The version is in three places that must agree (`scripts/build_wheel.py` checks it):
   `[workspace.package] version` of `Cargo.toml`, `version` of `python/pyproject.toml` and
   `__version__` of `python/hydrobem_gui/__init__.py`.
2. Move the `[Unreleased]` entries of `CHANGELOG.md` under the version and its date.
3. Run every check (above), then build the wheel (Distribution) and install it in a clean
   environment: `hydrobem-gui` opens the GUI, `python -c "import hydrobem_gui as hb; print(hb.build_info())"`.
4. Tag the commit `v<version>`. Where wheels are published is not decided yet (no public index).
