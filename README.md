# HydroBEM GUI

The graphical interface and Python API of HydroBEM, a frequency-domain panel method (BEM) for the
wave loads on floating and fixed bodies. One Python wheel, `hydrobem-gui` (Windows x64), holds:

- the engine `hydrobem.dll`: HydroBEM's engine through its C ABI, built by HydroBEM and imported
  here as a file (this repository does not build or depend on HydroBEM's sources);
- the Python API `hydrobem_gui`: models, solves and results; HydroBEM's Python package, calling the
  DLL with ctypes;
- the GUI `HydroBEM.exe`: written in Rust with egui and wgpu, on the same DLL.

Python 3.11 or later is required. CAD files (STEP, IGES) are not read here: cad2bem, a separate
program with its own GUI, converts them into geometry files (`.hbg`), which the GUI imports as
meshes.

## Install and start

Windows 10/11 x64 with a processor that has AVX2 and FMA. Install the wheel of the
[release](https://github.com/winsccssvly/hydrobem-gui/releases) into a Python environment:

```powershell
pip install hydrobem_gui-0.3.0-py3-none-win_amd64.whl
hydrobem-gui                                             # the GUI
python -c "import hydrobem_gui as hb; hb.show('case.hbm')"
```

The GUI and the engine use the Visual C++ runtime of Python and the Universal C Runtime of Windows;
the wheel carries no copy of the Microsoft runtime. Started from Python as above, the GUI takes
Python's runtime. Started directly (`<environment>\Lib\site-packages\hydrobem_gui\bin\HydroBEM.exe`),
it needs the Visual C++ runtime installed in Windows.

- The GUI's manual: [docs/manual](docs/manual/README.md)
- The Python API's guide and reference: [docs/python](docs/python/README.md)
- Examples: [examples](examples/README.md), HydroBEM's example scripts, installed with the wheel

## Features

- A project tree that follows HydroBEM's inputs: Meshes, Bodies, Lids, Fixed Dampers, Environment
  (Omegas or Periods, Headings), First Order (Field Points), Second Order, Options; the results by
  the engine's paths after Hydrostatics (Excitation, Motion, Body Points, Field Points, Lids, Drift,
  QTF, ...), labelled with the engine's names in title case
- A Details editor, tables of generated data (copy, CSV), result graphs (zoom, pan) and a 3D
  viewport (wgpu)
- Low-order GDF meshes (symmetry expanded), the engine's hydrostatics for the displaced volume and
  mass
- The higher-order method: B-spline and flat patch meshes (GDF and SPL, `.hbg`), analytic shapes
  (cylinders, spheres, barges, ...)
- Geometry files (`.hbg`) from cad2bem, with the patch checks of the report beside one
- Additional 6 x 6 damping and stiffness, irregular-frequency removal, dipole panels, pressure
  surfaces, damping lids, tanks, free-surface plates, generalized modes
- Infinite and finite depth, walls and channels, RAOs, mean drift and QTFs, field points and the
  free surface as a heatmap, animations
- Solves in the background, the progress of each worker, Stop
- Projects are HydroBEM model files (`.hbm`) and results files (`.hbr`), the same files as
  Python's (`hb.Model.load`)
- Exports: WAMIT-format files (period or rad/s), a NumPy archive (`.npz`: every result quantity,
  the motion equation and the bodies' placement; also from Python, `hb.save_npz`), tables as CSV,
  meshes as geometry files (`.hbg`)

## Development

Needed: Windows 10/11 x64 (AVX2/FMA), rustup, the Visual Studio 2022 Build Tools, Python 3.11 or
later with numpy and pytest, and HydroBEM's `hydrobem.dll` (imported with
`scripts/import-engine.ps1`; the DLL is not committed).

```powershell
cargo test --workspace
conda activate hydrobem-gui          # the project's environment (docs/DEVELOPMENT.md); activated for numpy's DLLs
cd python; python -m pytest -m "not slow"; python ..\scripts\gen_model_classes.py --check; cd ..
cargo build --release
cargo xtask dist
python scripts/build_wheel.py        # dist/hydrobem_gui-<version>-py3-none-win_amd64.whl
```

While developing, run `target/release/HydroBEM.exe`; a file as its argument opens it:
`HydroBEM.exe examples\floating_hemisphere\out\run\floating_hemisphere.hbr`. The procedures are in
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

`examples/` holds HydroBEM's example scripts (only `import hydrobem_gui as hb` changed). Each case
has a folder; each script makes its meshes, solves, compares with a published or analytic value
and writes its results file (`.hbr`) to `out/<script>/`, which the GUI opens (File → Open).

```powershell
python examples/floating_hemisphere/run.py
```

The models and meshes the tests read are in each crate's `tests/data`
(`crates/gui-core/tests/data/README.md`), made from HydroBEM's example cases by
`scripts/make_testdata.py` and saved with the GUI's state by `cargo xtask testdata`. The tests check
what the GUI adds (the project and engine-model mapping, reading results, files, the UI, the ctypes
binding); the engine's physics is validated in HydroBEM (published and analytic references).

## Repository layout

| Path | Contents |
|---|---|
| `python/hydrobem_gui` | The Python package: `_core.py` (ctypes over the DLL), the model and results API (HydroBEM's), `show()` and the `hydrobem-gui` command, `save_npz()` (the GUI's NumPy archive) |
| `python/tests` | The Python API's tests (HydroBEM's, pytest) |
| `crates/hydrobem-engine` | The safe Rust binding of the engine DLL (`Model`, `Value`, `Results`, progress) |
| `crates/gui-core` | The logic without UI: the project, the engine model, files, solves, result indexing, viewport geometry |
| `crates/hydrobem-gui` | The egui/wgpu application `HydroBEM.exe`: window, tree, Details, tables and graphs, 3D viewport, smoke test |
| `crates/xtask` | Test models, third-party notices, `dist/` (packed into the wheel by `scripts/build_wheel.py`) |
| `engine` | The imported engine: header `hydrobem.h`, licences, `IMPORT.json` (never edited; updated by `scripts/import-engine.ps1`; the DLL is not committed) |
| `docs` | Requirements, architecture, engine mapping, result definitions, limitations, development; the manual and the Python API's documents |
| `examples` | HydroBEM's example scripts (not tests; installed with the wheel) |
| `scripts` | Engine import, environment check (`check-env.ps1`), wheel build, test models (`make_testdata.py`), `_classes.py`, the Python documents' copy |

## Documents

For a developer new to the code, in this order:

1. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): the current state, code structure and data flow
2. [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md): the settled UI and physics rules
3. [docs/ENGINE_API.md](docs/ENGINE_API.md): the engine boundary and the shapes of result arrays
4. [docs/RESULTS.md](docs/RESULTS.md): result definitions, units, post-processing
5. [docs/KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md): what is incomplete or limited
6. [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md): build, tests, distribution
7. [docs/ROADMAP.md](docs/ROADMAP.md): what comes next
8. [docs/FILES.md](docs/FILES.md): model and results files, what the GUI keeps in a model, the WAMIT and NumPy exports
9. [CHANGELOG.md](CHANGELOG.md), [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)

## Licence

[Apache-2.0](LICENSE); notices in [NOTICE](NOTICE). The licences of the engine and of the
third-party components distributed with it are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

WAMIT is a trademark of WAMIT, Inc.; HydroBEM GUI writes files in the WAMIT file formats and is not
affiliated with, endorsed by or sponsored by WAMIT, Inc.
