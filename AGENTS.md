# HydroBEM GUI working rules

- The application is Rust (edition 2024, toolchain pinned in `rust-toolchain.toml`) with egui/eframe
  and wgpu, one cargo workspace: `crates/hydrobem-engine` (the binding of the engine DLL),
  `crates/gui-core` (the UI-free logic), `crates/hydrobem-gui` (HydroBEM.exe), `crates/xtask`
  (test models, licences, distribution). Windows x64 only.
- The product is one Python wheel (`hydrobem-gui`): `python/hydrobem_gui` (the Python API, ctypes
  over the DLL in `_core.py`) with the GUI and the DLL in its `bin`. Python is required. The Python
  code and tests were copied from HydroBEM's package; keep them in step by copying, never by
  importing HydroBEM. Run `python -m pytest` in `python/` (conda environment activated) and
  `python scripts/gen_model_classes.py --check` before a commit that touches them.
- cad2bem (CAD conversion) is a separate program with its own GUI; this GUI neither finds nor runs
  it (removed 2026-11-07) and does not read CAD files. It reads the geometry files (.hbg) cad2bem
  writes, and the report beside one, as any mesh file.
- `crates/gui-core` must stay free of egui, eframe and wgpu; UI code belongs in `crates/hydrobem-gui`.
- The engine is HydroBEM's `hydrobem.dll` (its C ABI, `engine/hydrobem.h`), built by HydroBEM and
  imported into `engine/` by `scripts/import-engine.ps1`; HydroBEM.exe loads it at run time from
  beside itself. Never edit `engine/`; import again to update it. Never modify or build the
  separately maintained HydroBEM repository; only read from it (HydroBEM makes the DLL).
- Call the DLL only through `crates/hydrobem-engine` (`Model`, `Value`, `Results`, ...;
  `docs/ENGINE_API.md`), which follows `engine/hydrobem.h` and its ABI version. Do not call
  `hydrobem.exe` or the Python API.
- Name project structures and fields after the engine's objects and items (HydroBEM's model); only
  GUI-only concepts get names of their own.
- Keep body-owned data (position, mesh, matrices, IRR, surfaces, inertia) beneath each body in the
  project model. Keep damping lids, environment, waves, options, outputs, field points, and
  results at project scope.
- Before a commit: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (dependencies build optimized in the dev profile). Build
  `cargo build --release` before producing `dist/` (`cargo xtask dist`).
- Project crates deny warnings and clippy's default lints (`[workspace.lints]`); do not silence
  them in project code, fix them. `unsafe` is denied except where a Windows call or a call of the
  engine DLL needs it (`crates/hydrobem-engine`), with a `SAFETY` comment.
- The engine needs AVX2/FMA: `.cargo/config.toml` sets the target features (as HydroBEM's); keep it.
- Pin third-party versions exactly in the workspace `Cargo.toml` and regenerate
  `THIRD_PARTY_NOTICES.md` with `cargo xtask licenses`. Prefer permissively licensed crates
  (MIT/Apache/BSD/Zlib); no GPL-only dependency.
- For egui/eframe/wgpu APIs, read the pinned crate sources in the cargo registry before relying on
  memory: their APIs change between versions.
- Before changing behavior, read `docs/ARCHITECTURE.md`, `docs/REQUIREMENTS.md`, `docs/ENGINE_API.md`,
  and `docs/KNOWN_LIMITATIONS.md`.
- Update the relevant document and `CHANGELOG.md` whenever a UI rule, engine mapping, result
  layout, or known limitation changes.
- Do not infer result dimensions from array length; use `Results::shape` and keep the documented
  F/H/D/K/P ordering.
- Treat display interpolation and smoothing as visualization only. Do not present it as additional
  solver resolution.
- After UI changes, run the GUI smoke test (`cargo test -p hydrobem-gui --test smoke`) and review its
  screenshots in `target/smoke/`.

## Verifying against the state outside the code

Tests start from a clean state; the user's does not. A change can pass every test and still fail
for a user because of state the tests never had.

- A value the UI shows comes from several places: the settings remembered in `app.ron`
  (`%APPDATA%\HydroBEM\data`), the project file (its GUI state), the GUI's defaults and the
  engine's. Before saying what the user will see, follow every source in order of
  precedence; a test of it starts from settings as a user's hold them (old names, values of
  another file), not from `Settings::default()`.
- After importing the DLL or changing the GUI: rebuild and reinstall the wheel
  (`docs/DEVELOPMENT.md`), then run `scripts/check-env.ps1`, which compares what the
  `hydrobem-gui` environment has installed with `engine/IMPORT.json` and `target/release`.
- Tests read only files of this repository (each crate's `tests/data`, `python/tests/data`),
  never another repository's working tree, which changes while its own sessions work. `examples/`
  is a public folder for users (HydroBEM's example scripts, copied as the Python package is), not
  tests: no test reads it; a test needs a model or mesh in its crate's `tests/data` (see
  `crates/gui-core/tests/data/README.md`), made from the examples by `scripts/make_testdata.py`.
- After a script edits files, check its result before the next command; run a script a document
  describes before relying on what the document says it does.
- Say what was verified and how (test, screenshot, the real app); say plainly what was not
  checked in the real app instead of stating the outcome.
