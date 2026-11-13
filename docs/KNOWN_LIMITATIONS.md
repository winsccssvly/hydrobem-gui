# Known limitations

These are known constraints or unfinished work, not hidden features.

## Project and workflow

- Model files written elsewhere (Python) have no GUI state: uneven headings open as evenly spaced
  (with a warning); field points that are not a grid the GUI rebuilds open as a Point List
  (`FILES.md`); a body without a mass opens as Displaced with a warning.
- Model files support what the GUI edits; record ranges of a mesh other than the body's (a lid in
  global coordinates, a fixed damper, a surface or control surface of its own mesh, an
  interior surface of a library mesh) and trimmed meshes stored as meshes are refused with the
  object path. Radii of gyration open as Ixx ... Izz. A free-surface mesh of the potential QTF
  is a whole library mesh: `free_surface_ranges` is refused, and FDF files are not read (the
  engine's `load_fdf`); a GDF or .hbg mesh of panels in z = 0 serves.
- The QTF's Total is the GUI's sum of the engine's quadratic and potential parts (the engine
  gives the parts only); Newman's approximation has no total. The QTF heatmap colours the range of the values shown (no colour scale of one's own, no
  logarithmic scale). The QTFs are in the
  results file only, not in the WAMIT export (the engine writes none).
- Model and results files open only in the engine's present record version and format (the
  engine converts none). Geometry files (`.hbg`) of version 2, which cad2bem writes, are read as version 3 by the GUI
  (`gui_core::hbg_upgrade`); version 1 does not open.
- Field points inside a tank give the tank's radiation pressure and velocity alone (the engine
  solves the tank's fluid for the radiation only): their incident, scattering and diffraction
  parts are zero, and Total is the radiation of the motions. They are given in the tank's body
  axes from the grid's global points by the body's position and yaw; a trimmed body's tank points
  are not moved by the trim.
- No report generation or automatic recovery; a table exports as CSV one at a time. Only the last
  solve is kept in memory (save it as a results file to keep it).
- The free-surface Total (incident + scattering + Σ motion × radiation) is the GUI's sum of the
  engine's quantities, by the superposition HydroBEM states as its contract; the engine computes no
  total itself, and the lid, body-pressure and velocity outputs have no Total view.
- The window title does not mark unsaved changes; they are found when a prompt is due.
- Save and Save As keep their file dialog on the window's thread (the unsaved prompt waits for
  its answer): the window does not draw while it is open. The other file dialogs do not block it.
- `hydrobem_gui.show(model, wait=True)` waits for the GUI but does not return what the user saved
  (`ROADMAP.md`).

## Engine capability and mapping

- The body's own roll and pitch are rejected before the solve; a heeled or trimmed hull is given
  by Trim (the engine cuts the mesh at the free surface).
- Analytic meshes made in the GUI are the engine's quarter shapes (symmetry `xz_and_yz`, waterline on z = 0);
  the Cylinder with Dipole Discs has no moonpool (NMP = 0), and fewer patches than the engine
  allows (a cylinder's side alone, a barge's bottom alone) are not offered.
- Higher-order (patch) meshes are read from higher-order GDF files (with the SPL file of the same
  name, else with the discretization asked at import: a panel size or spans per patch, the spline
  and Gauss orders; the GUI does not read WAMIT's cfg file) or from geometry files (.hbg, with their discretization,
  else the same default) and kept as the engine's mesh object: the GUI edits their
  discretization, not their geometry. A patch mesh read from a GDF file can take its
  discretization from a Panel Size instead (the engine reads the file again); the size is shown
  for the session only, the rows are what is saved. A patch is drawn as a curved surface: the engine's
  tessellation (up to 24 x 24 quadrilaterals per patch, about 20 000 in all), shaded by the
  surface normals at its points and outlined at the patch boundaries only. It is a display: the
  solve uses the exact patch.
- Generalized and pressure coordinates take the units of a translation (m, N, kg ...), the
  normalization the engine marks them with (`coordinates`, rotation flag 0): a generalized mode's
  shape is a displacement per metre of its coordinate.
- Body output colours are the panels themselves when the points are the engine's panel centroids
  (every symmetry copy) and, in the higher order, the tessellated patches by output cell (each
  quadrilateral to its nearest point: the cell boundaries follow the tessellation, about 1/16 of a
  span, not the exact parameter lines); specified body points and listed field points are squares
  of each point's area or of the points' spacing: nothing is interpolated between them. Lid and
  pressure-surface points are triangulated in plan and coloured between them (display only), over
  the surfaces as solved; the band between the outermost points and the surface's edge (in the
  higher order, outside the outermost Gauss points) is not coloured. Lid and pressure-surface
  elevations and field velocities are not animated.
- A Body-Fitted grid follows the body's section at Z (the untrimmed mesh, as placed) seen from its
  centre: on each ray the outermost crossing, so a section that is not star-shaped about its centre
  (two hulls, a deep notch) is followed by its outline as a star; Convex Outline rings it instead,
  and Inside Rings cover the water between the columns (around each column the points stop where
  the rings cross it, not along its waterline). Further Depths use the rings of Z.
- Field Pressure and Field Velocity need field points: without them they are not requested
  (Field Velocity does not survive saving the model; Field Pressure reopens on).
- A lid in global coordinates is a mesh of its own; a lid placed with a body is one record range of
  that body's mesh (the engine's rule). A generalized mode's displaced hull (Details → Display
  Scale) shows the wetted panels moved by the engine's displacements, not the other surfaces.
- The memory estimate (Options Details, and a warning before Run) is the engine's; it leaves out
  the GUI's copies of the results.
- Whether a field point lies inside a body is the engine's solid-angle test: a point within about a
  gap width of the hull can occasionally get the wrong verdict. Points closer to a panel than its
  size are kept but warned of; Field Points Details count them.
- The Python API is HydroBEM's Python package as copied at HydroBEM `ee6c71b`, on the DLL: a
  later change of HydroBEM's package appears here only when copied again. Objects types the DLL
  lists without items (the model) are ordered by a rule (before the first type they
  hold): the C ABI gives no order of the types. `show(model)` without `wait` leaves its temporary model
  file in the system's temporary folder. `Model.close()` must not be called while another thread
  still uses the model. A GUI open from the environment locks `bin/HydroBEM.exe` and
  `bin/hydrobem.dll`: close it before `pip install -U`.
- The engine is the imported `hydrobem.dll`, which HydroBEM built; the commit shown is the one the
  DLL reports of its own build. `engine/IMPORT.json` records the HydroBEM commit at the import and
  the time of the DLL: a DLL older than that commit (the import warns) was built from an earlier
  one. The DLL is not committed to this repository: a checkout needs `scripts/import-engine.ps1`
  before it builds a working program.

- Animation moves the bodies rigidly (the six rigid motions); generalized and pressure modes, lids
  placed with a body, and listed field points are not animated. The free surface of the Motions
  view is the incident wave alone (no scattering or radiation), drawn around the bodies.

## Models and preprocessing

- GDF import reads the low-order panel format and the higher-order format (B-spline, flat and
  analytic patches); a higher-order file is recognised by its fourth line (patch count and
  geometry code). A .hbg file's `info` members are not kept (the engine does not read them), and
  an analytic mesh read from one has no editable dimensions (its patches are kept as read). A mesh
  is exported as .hbg only, not as GDF. CAD files (STEP, IGES) are not read: cad2bem, a program of
  its own, converts them into .hbg files. The per-patch checks (columns and colours of the warned
  and failed patches) come only from a cad2bem report beside the .hbg (`NAME.report.json`).
- `Displaced` computes displaced volume and mass with the engine's hydrostatics of the body alone;
  centre of gravity and rotational inertia are user inputs.
- IRR surfaces are drawn from the engine (`body_tessellation` of role interior_surface); when the engine rejects
  the body's inputs the viewport falls back to the mesh range or a footprint marker and Details
  give the reason.

## User interface

- The viewport is a viewer: field points can be picked, bodies and panels cannot; no CAD editing.
  Translucent triangles are sorted back to front per triangle; intersecting translucent surfaces
  can still show sorting artefacts. Very large meshes have no level of detail (outlines are skipped
  above 25 000 panels).
- Field points are picked by screen distance (8 px), without occlusion by bodies.
- The viewport renders in its own 4x multisampled target; it needs a GPU adapter (D3D12 or
  Vulkan; Windows' WARP software adapter also works, slowly).
- UI strings are English only. Hangul in file names and model names is shown (Malgun Gothic).
- Text entry follows egui: Enter or leaving a field commits it, Escape reverts; there is no undo.

## Distribution

- The project is Apache-2.0 (`LICENSE`; `license` of the Cargo workspace and of
  `python/pyproject.toml`, `License-Expression` of the wheel); no source file carries a licence or
  copyright header, the copied Python code included (HydroBEM's carry none either since
  2026-11-14): the licence is stated once, in `LICENSE` and `NOTICE`. `NOTICE` names what came from HydroBEM (its NOTICE is
  `engine/legal/NOTICE`), the Hulme (1982) values of the Python tests (not licensed by the project);
  and the published values the example scripts print.
- The examples are scripts: they open in the GUI only through the results file each writes
  (`out/<script>/*.hbr`), after it has run; no `.hbm` model ships with them. cad2bem (GPL)
  and its gmsh are neither run nor shipped by this product. The licences of the engine DLL and of every crate compiled in are listed in
  `THIRD_PARTY_NOTICES.md` and carried, with `NOTICE`, in the wheel's `.dist-info/licenses/`. A
  crate that ships no licence file gets the standard texts of its licence expression with its
  authors as the copyright holders; the fonts compiled into the exe (epaint_default_fonts: Ubuntu
  Light under the Ubuntu Font Licence, Noto Emoji under OFL-1.1, Hack under MIT and the Bitstream
  Vera licence, the emoji and icon fonts under MIT) carry their own texts from the crate's
  `fonts/`, and `cargo xtask dist` fails when a crate with a font licence ships none.

## Test coverage

- Covered: GDF reading and symmetry expansion, the engine hydrostatics, the field-point exclusion
  and domain rules, wave-case editing, every input through the model and back, suppressed bodies and
  inactive lids kept, the unsaved-change fingerprint, refused inputs, a model without a mass, native
  solves (box, RAO, two bodies, pressure surface, x wall, channel, lid, interior previews), engine
  warnings and located errors, cancellation, results files and the WAMIT and NumPy exports, every test model
  opened and solved with its layout, the scene layers, the table's CSV and copy
  text, the camera, and an end-to-end GUI smoke test with screenshots; the Python API through
  HydroBEM's Python tests on the DLL (`python/tests`, without their slow references).
- Left to HydroBEM: the engine's accuracy (published, analytic and WAMIT references, convergence
  with the mesh); the tests here check that the GUI passes its inputs and reads its results.
- Not covered: pixel-level viewport checks, mouse interaction (dragging, picking by click), native
  file dialogs; `hydrobem_gui.show()` and the `hydrobem-gui` command (checked by hand on an
  installed wheel).
