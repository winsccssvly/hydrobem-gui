# HydroBEM GUI user manual

HydroBEM GUI (`HydroBEM.exe`) is the graphical interface of HydroBEM, a frequency-domain panel
method (BEM) for wave loads on floating and fixed bodies. In the GUI you build a model (meshes,
bodies, environment, wave frequencies and headings, what to compute), solve it with HydroBEM's
engine (`hydrobem.dll`), look at the results in tables, graphs and the 3D viewport, and save or
export them.

This manual explains how to use the program. It does not explain the theory: for that, and for the
meaning and conventions of every quantity, see HydroBEM's documentation.

## Installation

The GUI is part of the Python wheel `hydrobem-gui`, together with the Python API `hydrobem_gui`
and the engine DLL. You need:

- Windows on x86-64 (Windows x64 only), with a processor that has the AVX2 and FMA instructions;
- Python 3.11 or later and NumPy.

Install the wheel into a Python environment (a conda environment or a virtual environment):

```bat
pip install hydrobem_gui-0.3.0-py3-none-win_amd64.whl
```

Start the GUI in one of these ways:

| How | What opens |
|---|---|
| `hydrobem-gui` in a terminal of that environment | an empty project |
| `hydrobem-gui barge.hbm` or `hydrobem-gui barge.hbr` | that model or results file |
| `HydroBEM.exe` in the `bin` folder of the installed package | an empty project (double-click; no console window). Started this way it needs the Visual C++ runtime installed in Windows (most computers have it); the two ways above use Python's |
| `hb.show()`, `hb.show("barge.hbm")` from Python (`import hydrobem_gui as hb`) | empty, or with that file |
| `hb.show(m)` with a `hb.Model` | that model as a new, unsaved project (Save asks for a file name) |

The GUI and its engine use the C runtime of Python (`vcruntime140.dll`) and of Windows: the wheel
carries no copy of the Microsoft runtime.

The GUI and the DLL in the `bin` folder are locked while the GUI runs: close it before you
upgrade the wheel with `pip install -U`.

The program uses the fonts of Windows (Segoe UI, Consolas; Malgun Gothic for Hangul file names)
and needs a graphics adapter for the 3D viewport (Direct3D 12 or Vulkan; Windows' software
adapter also works, slowly). The user interface is in English only.

If the engine DLL is missing, or is of another version than the GUI expects, a message box says
so at start-up and the program exits: install the wheel again.

## Scripting

Everything the GUI edits is a HydroBEM model, and the GUI's files are HydroBEM's own model and
results files. A model built by a Python script opens in the GUI and the other way round. For
scripting, see the Python API documentation: [docs/python](../python/README.md), starting with
[Getting started](../python/guide/getting-started.md).

## Reading order

Read chapters 1 to 6 in order the first time; chapter 7 when you save and export.

| Chapter | Contents |
|---|---|
| [1. The window](01-window.md) | The project tree, Details, the 3D viewport, the output pane, the status bar, Run, the settings the program remembers |
| [2. Meshes](02-meshes.md) | The mesh library: importing GDF and `.hbg` files, analytic meshes, the mesh tables, panel size of patch meshes, exporting a mesh |
| [3. Bodies](03-bodies.md) | A body's mesh and panel ranges, position, trim, mass and inertia, modes, the interior surface, and the surfaces and items a body can own |
| [4. Environment and outputs](04-environment-and-outputs.md) | Lids, fixed dampers, the water and its walls, frequencies and headings, first- and second-order outputs, field points, options |
| [5. Solving](05-solving.md) | Run, the engine's checks, the progress, Stop, warnings and errors, results out of date |
| [6. Results](06-results.md) | The Results tree, tables and graphs, heatmaps, animation, drift and QTF views, hydrostatics |
| [7. Files and export](07-files.md) | Project and results files, the unsaved-changes prompt, WAMIT-format, NumPy and CSV export |

A typical first session: import a mesh (chapter 2), make a body of it and give it a mass and
inertia (chapter 3), set the frequencies and headings (chapter 4), press Run (chapter 5), look at
Added Mass and Motion (chapter 6), and save the results (chapter 7).
