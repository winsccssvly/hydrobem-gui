# The Python API: guide and reference

The package `hydrobem_gui` of the wheel `hydrobem-gui` is HydroBEM's Python API: a script builds a
model, checks it, solves it and reads its results, and the GUI opens the same model and results
files (`hb.show(m)`, File → Open). The documents of this folder are HydroBEM's user documentation,
copied with the package (`import hydrobem_gui as hb` where HydroBEM writes `import hydrobem as hb`);
the theory ("theory §n" in these documents), the validation report and the engine's Rust and C
references are in HydroBEM's own documentation. The GUI has its own manual: [docs/manual](../manual/README.md).

## Reading paths

| Reader | Path |
|---|---|
| New user | [Getting started](guide/getting-started.md) (installation and a first problem), then [working with the model](guide/working-with-the-model.md) and the topics of the guide as needed |
| Looks something up | The reference: the inputs by object, the queries, the results, the output files |

## The guide

| Page | Contents |
|---|---|
| [getting-started.md](guide/getting-started.md) | Installation and a first problem, a floating cylinder, step by step |
| [working-with-the-model.md](guide/working-with-the-model.md) | The objects and items of the model, changing and checking it, model and results files |
| [meshes-and-bodies.md](guide/meshes-and-bodies.md) | Meshes and their parts, checking the geometry, bodies and their mass properties, several bodies, trim, leaving an object out |
| [environment-and-options.md](guide/environment-and-options.md) | The water, walls and channels, frequencies and headings, what the first order computes, the two methods |
| [results.md](guide/results.md) | The results, their axes and the output files |
| [special-surfaces.md](guide/special-surfaces.md) | The interior surface, thin and porous plates, air cushions, free-surface plates, damping lids, field points |
| [tanks-and-generalized-modes.md](guide/tanks-and-generalized-modes.md) | Internal tanks and generalized modes |
| [second-order-loads.md](guide/second-order-loads.md) | The mean drift and its methods, the control surface, the QTFs, Newman's approximation, the free surface of the second-order potential |
| [performance.md](guide/performance.md) | Threads, memory, symmetry and the progress of a solve |

## The reference

| Page | Contents |
|---|---|
| [inputs-model.md](reference/inputs-model.md) | Editing the model, the tree of the inputs, the environment, the waves, the options, the model file, suppressed objects |
| [inputs-meshes.md](reference/inputs-meshes.md) | Meshes, the items mesh and ranges, the geometry file (`.hbg`) |
| [inputs-bodies.md](reference/inputs-bodies.md) | Bodies, trim, couplings, the surfaces of a body |
| [inputs-model-objects.md](reference/inputs-model-objects.md) | Lids, fixed dampers, field points |
| [inputs-orders.md](reference/inputs-orders.md) | First and second order, the free surface of the potential QTF |
| [inputs-rules.md](reference/inputs-rules.md) | The model rules and the supported combinations |
| [queries.md](reference/queries.md) | Every query of a model |
| [results.md](reference/results.md) | The results and the results file (`.hbr`) |
| [file-formats.md](reference/file-formats.md) | Normalization, the WAMIT-format numeric files and the `.out` summary |
| [references.md](references.md) | The literature cited |

The examples of `examples/` (installed with the wheel) are complete scripts, one case each.

## What this package adds to HydroBEM's

Two modules are this package's own, not HydroBEM's, and are not in the copied documents:

- `hb.show(model or file)` and the `hydrobem-gui` command open the GUI (`gui.py`).
- `hb.save_npz(results, "case.npz")` writes the GUI's NumPy archive (File → Export → NumPy):
  every result quantity under its engine name, `bodies`, the motion equation
  (`motion_equation_inertia`, `_stiffness`, `_damping`), the bodies' placement (`body_position`,
  `body_yaw`, `body_rotation_center`, `body_trim`) and `meta`, a JSON text
  ([FILES.md](../FILES.md#numpy-export-npz)) (`npz.py`).

```python
r = hb.load_results("run.hbr")      # or m.solve()
hb.save_npz(r, "run.npz")
z = np.load("run.npz")
```
