# Test data of gui-core

The models and meshes the tests of `hydrobem-gui-core` read (`src` unit tests and `tests/`); every
model is here, as `tests/files.rs` opens and solves each. `crates/hydrobem-gui/tests/data` holds
copies of those the GUI's tests read (a subset). Tests never read `examples/`.

Written by `python scripts/make_testdata.py` (HydroBEM's example cases: each example's `build()`,
with the outputs, frequencies and headings the tests need) and then `cargo xtask testdata` (opened
in the GUI and saved with its state); do not edit them by hand. Write them again when the examples
or the engine's model record change, and change the tests with them.

| Model | Example | What the tests use it for |
|---|---|---|
| `cylinder.hbm`, `cylinder.gdf` | `cylinder` | R = 1 m, T = 0.5 m, a quadrant of 192 panels; 9 frequencies 0.4 ... 2 rad/s, 3 headings, motions, a 25 x 25 field grid |
| `cylinder_wall.hbm` | `cylinder` | at y = 2 m beside the wall y = 0 |
| `cylinder_channel.hbm` | `cylinder` | in a channel 8 m wide, 5 m deep |
| `two_truncated_cylinders.hbm` | `two_truncated_cylinders` | two bodies, motions, a field grid |
| `three_truncated_cylinders.hbm` | `three_truncated_cylinders` | three bodies, body-fitted rings (GUI state), a solve slow enough to watch |
| `three_truncated_cylinders_patches.hbm` | `three_truncated_cylinders/run_higher_order.py` | analytic patches of three bodies (opened only) |
| `truncated_cylinder_patches.hbm`, `.gdf`, `.spl` | `truncated_cylinder_finite_depth/run_higher_order.py` | the analytic cylinder R = 1 m, T = 1 m (two patches); the GDF (IGDEF -1) and SPL written by the script, as the engine keeps an analytic mesh in the model |
| `floating_owc_chamber.hbm` | `floating_owc_chamber` | the chamber's pressure surface and mode (D = 7), ka = 0.4, 1, 2 |
| `floating_owc_chamber_lid.hbm` | `floating_owc_chamber` | the same tube, a damping lid (0.1) on its chamber instead |
| `cylinder_with_heave_plate.hbm` | `cylinder_with_heave_plate/run_thin_plate.py` | the plate a dipole sheet, on a mesh three times coarser than the example's |
| `dock.hbm` | `dock` | a free-surface plate, the body of zero draft |
| `fish_tank_cylinders.hbm` | `fish_tank_cylinders` | a tank, a massless body |
| `fish_tank_covered.hbm` | `fish_tank_cylinders/run_covered_tank.py` | the tank half covered: a free-surface plate of the tank, a mesh of its own |
| `trimmed_moonpool_cover.hbm` | `trimmed_moonpool_cover` | a trimmed hull with a generated free-surface plate in its moonpool |
| `three_columns.hbm` | `three_truncated_cylinders` | the three cylinders as one body (one mesh of the three columns where the example places them): a section in parts |
| `truncated_cylinder_gdf.hbm` | `truncated_cylinder_finite_depth/run_higher_order.py` | the analytic cylinder read from `truncated_cylinder_patches.gdf` and `.spl` (written by `cargo xtask testdata`): a patch mesh read from a file, its source the file name alone |
