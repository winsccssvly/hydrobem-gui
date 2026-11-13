# Files

The GUI reads and writes HydroBEM's own files only, through the engine: projects are model files
(`.hbm`), results are results files (`.hbr`); it adds the GUI state inside the model and nothing
else (`gui_core::files`). Python reads and writes the same files (`hb.Model.load`, `m.save`,
`hb.Model.load_results`), so a model made by a script opens in the GUI and the other way round.
Meshes are read from GDF and geometry files (`.hbg`, `REQUIREMENTS.md`).

## Project files (.hbm)

HydroBEM's model file (HydroBEM inputs §9): one binary file with every object and item of the
model, the meshes included, written and read by the engine (`Model::save`, `Model::load`).

- **Open** (`gui_core::files::load_project`): `Model::load`, then `read_model`. A results file
  (`.hbr`) opens as its inputs too. A model the GUI cannot represent is refused with the object path
  (`ENGINE_API.md`); inputs the GUI had to adapt are listed after opening.
- The model's `meshes` are the GUI's mesh library, in order, with their names.
- **Save** (of the model: before a Run, or after the inputs changed since; once solved Save writes
  the results file): `build_model` with the GUI state, written by the engine into a temporary file
  beside the target and renamed over it, so a failed save leaves the old file. The meshes are
  inside the file: no GDF files are needed again.
- **Unsaved changes**: a fingerprint of everything the file would hold (the inputs, the GUI state,
  each mesh as a checksum), compared with the one at the last open, save or New.

### The GUI state

The GUI keeps what the problem cannot hold in the model's `tool` list, object `hydrobem_gui`, item
`text`: JSON with `version: 1` (a newer version is ignored with a warning).

| Key | Contents |
|---|---|
| `finite_depth` | the Depth shown while Depth Type is Infinite |
| `wave_frequencies` | `edit_in_table`, `start`, `end`, `count` of the generator (used while it still describes the rows) |
| `analytic_meshes` | per analytic mesh made in the GUI, its library name and its shape (`kind`, `dimensions`, `lid`, `nonuniform`, `discs`): on opening it stays editable while its mesh object is still analytic with as many patches |
| `field_grid` | the field-point grid (`enabled`, `layout` Cartesian / Polar / BodyFitted, x/y start, end, count, `z`, the rings' `centre`, `radial_*`, `angle_*`, `fitted_body`, and a body-fitted grid's `profile`: the section's centre and its radius every half degree, to the nanometre, so that the points rebuild without the body) |
| `bodies` | every body in tree order: the GUI's body (`visible`, roll and pitch, the panel range, a given mass kept while Displaced is chosen, the added matrices and their table-edit flags, every dipole and pressure surface with the switched-off ones, …) and the names of its mesh objects (`mesh`, `dipole_meshes`, `pressure_meshes`, `damper_meshes`, `control_mesh`, `interior_mesh`) |
| `lids` | every lid in tree order, with its mesh object |
| `fixed_dampers` | every fixed damper with the switched-off ones, and its mesh object |
| (problem) | the added mass limits are the omegas 0 and inf (periods inf and 0) of `environment`, read back as the two checkboxes |
| `headings` | the heading generator (start, end, count), used while it gives the model's headings |

Suppressed bodies, lids and switched-off surfaces are objects of the model with `enabled = false`;
the engine solves the model as if they were absent. Lids without a mesh, and a suppressed body the
engine cannot hold, are kept whole here, in their place in the tree. A model without the GUI state
(written by Python) opens with its bodies and lids as given, the field grid rebuilt from the points
(first, last, smallest spacing; a Point List when no grid gives exactly those points and tanks),
uneven headings as an even spacing (with a warning) and uneven frequencies in table mode.

## Results files (.hbr)

File → Save (once Run has solved the project as it is), File → Save As (file type HydroBEM results)
and File → Open Results write and read HydroBEM's results file (HydroBEM
`docs/reference/results-and-files.md` §1.2 is the specification). The engine writes and reads it
(`Results::save`, `Model::load_results`); the GUI adds nothing of its own.

- **Contents**: what the engine writes from the results of a solve (the `Results` the GUI keeps in
  `SolveResult::results`): the model that was solved (the inputs when Run was pressed, not as
  edited since, with the GUI state in its `tool` object), the meta data of the run (HydroBEM
  version, API version, commit, compiler, time, route, thread counts, engine seconds), the warnings
  with where they apply, and every computed quantity in the engine's type and layout, the per-body
  hydrostatics included. Written to a temporary name beside the target and renamed, so a failed
  save leaves an existing file intact.
- **Opening**: `Model::load_results` gives the model with its results. The GUI reads its project
  from the model as from a model file (`read_model`: what it cannot represent is refused) and the
  results as after a solve (`SolveResult::new`): the field points are those of the model, the
  points in the sea then each tank's (placed by its body), with where their results are
  (`field_points::model_field_points`). The results file becomes the project's file: Save writes
  it again (with the results, while they are those of the inputs).
  The examples' scripts write such files (`examples/*/out/<script>/*.hbr`).

## WAMIT-format export

WAMIT is a trademark of WAMIT, Inc. HydroBEM and this GUI write files in the WAMIT file formats;
they are not affiliated with, endorsed by or sponsored by WAMIT, Inc.

File → Export → WAMIT (s) or WAMIT (rad/s) has the engine write the WAMIT-format files of the last
solve (`hb_results_write_wamit`, HydroBEM `results-and-files.md` §3-§5; `Results::write_wamit` in
`crates/hydrobem-engine`, `gui_core::files::export_wamit`): `NAME.out` and the numeric files of the
same name (`.1`, `.2`, `.2fk`, `.2sc`, `.3`, ..., `.hst`, `.fpt`, `.pnl`, those of the quantities
solved). The first column is the period in s (WAMIT (s), IPERIN = 1) or the wave frequency in
rad/s (WAMIT (rad/s), IPERIN = 2); the headers are dated at the export. Files of that name already
there are replaced. Python writes the same files with `Results.write_wamit(stem)` (period) or
`Results.write_wamit(stem, frequency="omega")`.

## NumPy export (.npz)

File → Export → NumPy (.npz), once there are results, writes every quantity of the last solve as
one NumPy archive (`gui_core::files::export_npz`; the GUI writes it, not the engine): a ZIP of
stored `.npy` files (NumPy format 1.0, C order, `gui_core::npz` and `gui_core::zip`; ZIP64 past
4 GiB) that `numpy.load` reads without pickling. The engine's data are copied as they are, no
value recomputed or interpolated. With the motion equation and the placement of each body, the
archive is enough for a calculation of its own (a time-domain one) without the engine.

From Python, `hb.save_npz(results, "case.npz")` writes the same archive for results solved or
loaded there (`python/hydrobem_gui/npz.py`, this package's own module, not HydroBEM's; `np.savez`,
stored members). The members, their order, types and the `meta` texts are the GUI's; the Rust test
`npz_python_texts_match` (`crates/gui-core/tests/files.rs`) fails when the module's texts differ
from `export_npz`'s, and `python/tests/test_npz.py` checks the arrays.

| Key | Contents |
|---|---|
| each quantity the results have | under its engine name (`omegas`, `headings`, `radiation_mode_ids`, `coordinates`, `added_mass`, `motion`, `field_points_pressure_radiation`, `hydrostatics_stiffness`, ...), in the engine's element type (float32/64, complex64/128, int32/64) and dimensions (`Results::shape`, the F/H/D/K/P order of `RESULTS.md`) |
| `bodies` | the names of the solved bodies, the B axis (Unicode) |
| `motion_equation_inertia`, `motion_equation_stiffness`, `motion_equation_damping` | [6B, 6B] float64: the engine's motion equation of the model as solved (`Results::solved_model`, `Model::motion_equation`): the rigid-body mass and inertia, the stiffness (hydrostatic, weight and additional) and the additional damping, body axes about each rotation centre R; the rigid coordinates only (pressure and generalized coordinates: `mode_restoring`, `pressure_mode_damping` and `coordinates`). Absent when the engine has no motion equation (a body without a centre of gravity, a mass or inertia: `meta.motion_equation_ready` false) |
| `body_position` [B, 3], `body_yaw` [B], `body_rotation_center` [B, 3], `body_trim` [B, 3] | float64, the bodies of the model as solved: the global position of the body origin (m), the yaw about the global z axis (deg, before the translation), the rotation centre R (body-local, m), the trim (elevation m, pitch and roll deg; zero without one). The centre of gravity is `hydrostatics_center_of_gravity` |
| `meta` | a JSON text (zero-dimensional, Unicode): `format` "hydrobem-gui npz", `version` 1, the `route`, `run` (HydroBEM version, commit, compiler, linear algebra, build type, time, API version, threads, engine seconds), `warnings` (severity, code, message), `quantities`: per quantity its `dims` (the axes, e.g. "frequency,heading,coordinate"), `unit` and `description` from the engine's quantity table; `motion_equation_ready`, `second_order_motions` (the drifts and QTFs are of the motions solved with this motion equation: a calculation with other stiffness or damping moves differently from what they assume) and `model`: the axes, unit and description of the motion equation and placement keys |

```python
z = np.load("case.npz")
meta = json.loads(z["meta"][()])
a = z["added_mass"]          # [F, D, K]
```

Written to a temporary name beside the target and renamed; a file of that name is replaced. The
field points of a tank are not separate keys: `tanks_pressure_radiation` holds them as the engine
does (`tanks`, `tanks_ids`).

## Test models

`crates/*/tests/data/*.hbm` (`crates/gui-core/tests/data/README.md`) are HydroBEM's example cases,
built by `scripts/make_testdata.py` and saved by the GUI with `cargo xtask testdata`. Their meshes
are made from vertices (no source file), but `truncated_cylinder_gdf.hbm`, whose mesh is read from
`truncated_cylinder_patches.gdf` and keeps the file name alone as its source (no machine path in a
model of the repository).

## Checks

- `crates/gui-core/tests/files.rs`: every test model opens and solves with its layout; round trips;
  a solved model saves its results and reopens with the same frequencies, field points, bodies and
  D offsets; a results file also opens as a project; a model file is not results; refused inputs.
- The smoke test saves the results of its solve, starts a new project, reopens them and shows them.
