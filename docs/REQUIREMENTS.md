# Accepted requirements

This file records product behavior established during development. Do not silently replace
these rules with defaults from another hydrodynamics program. Items marked *(C++ port)* were
added or changed when the application moved from C# WPF to C++/Qt on 2026-10-25; items marked
*(Rust port)* when it moved to Rust, egui and wgpu on the final HydroBEM engine on 2026-11-01.

## Application structure

- The application is Rust with egui/eframe and wgpu and targets Windows x64. *(Rust port)*
- The only numerical engine is HydroBEM's `hydrobem.dll` (the C ABI of its engine), built by
  HydroBEM, installed beside HydroBEM.exe and loaded at start-up; the GUI calls it only through
  `crates/hydrobem-engine` (`ENGINE_API.md`). A missing DLL, or one of another ABI or other result
  quantities, is named in a message box at start-up and HydroBEM.exe exits. *(DLL, 2026-11-04)*
- HydroBEM.exe does not run `hydrobem.exe` nor import Python: Python starts the GUI (`show`, the
  `hydrobem-gui` command), never the reverse.
- Do not modify the separate HydroBEM source repository.
- The final application is a GUI-subsystem executable; double-clicking `HydroBEM.exe` (the
  wheel's `bin/`, or `dist/`) must not open a console window, and it runs with `hydrobem.dll` beside it and
  the C runtime as a DLL: the Visual C++ runtime (`vcruntime140.dll`) and the Universal C Runtime of
  Windows. Nothing of the Microsoft runtime is distributed: started from Python (`show`, the
  `hydrobem-gui` command) it takes Python's runtime (`gui.py` puts its folder first on the PATH);
  started directly it needs the Visual C++ runtime installed in Windows. *(2026-11-13)*
  It needs a processor with AVX2 and FMA and says so at start-up otherwise. *(Rust port)*
- `HydroBEM.exe <file.hbm|file.hbr>` opens that model or results file. *(Rust port)*
- HydroBEM is distributed as one Python wheel, `hydrobem-gui` (Windows x64, Python ≥ 3.11, numpy):
  the Python API `hydrobem_gui` on `hydrobem.dll`, and the GUI and the DLL in its `bin` folder.
  Python is required. `hydrobem-gui [file]` (no console window) or the exe in the environment's
  folder starts the GUI; `hydrobem_gui.show()` opens it empty, `show(path)` with a model or results
  file, `show(model)` with a `Model` as a new unsaved project (`HydroBEM.exe --untitled
  model.hbm`: no file, Save asks for one, closing asks to save, not a recent file). *(Python
  package, 2026-11-04)*

## Main layout

- Left upper pane: project tree. Left lower pane: Details for the selected tree item.
- Details scroll vertically only; long values are clipped to the pane (the mesh file path shows
  in full as a tooltip). The mouse wheel changes a drop-down only after it was clicked or tabbed into. *(C++ port)*
- A drop-down is the whole value cell (no button inside it): its text starts where the text of the
  other cells does, an arrow at the right, a shade when hovered or open. *(Rust port)*
- Right upper pane: 3D viewport. Right lower pane: generated table and, only when relevant, graphs.
- Output defaults to table-left/graph-right. View → Output Layout switches between split and tabbed layouts.
  The output layout, View → Show Field Points, View → Show Free Surface, View → Show Normals,
  View → Show Spans and the Interpolate choice are remembered between sessions, with the recent files and the window size. *(Rust port)*
- When a result has no meaningful graph, the table occupies the output area and the graph pane/tab is hidden.
- Table headers put the unit on a second line. Columns are at least as wide as their header and
  values; spare width is shared, and a narrow pane scrolls sideways instead of clipping. *(C++ port)*
- Ctrl+C copies the selected cells as tab-separated text (pastes into Excel cell by cell);
  Ctrl+Shift+C adds a one-line header row. Both, plus Select All, are on the table's right-click
  menu. Values are copied as displayed. *(C++ port)*
- Ctrl+V pastes tab-separated text (Excel's copy) into an editable table from the top-left cell of
  the selection; one value fills the whole selection. Cells beyond the table's rows, in read-only
  columns, or not numbers are left out, and the status bar counts the values pasted and refused.
- Tables, Details and the graphs' boxes show numbers with up to six decimals, without trailing
  zeros; a magnitude below 1e-4 or of five digits or more before the point (1e4: forces in N,
  masses in kg, ...) in exponent notation with seven significant digits (`1.258368e+04`), which
  reads back when typed. Graph ticks of large values are in exponent notation too, and so are
  those of an axis showing only small values (a tick step below 1e-3, the axis within ±1:
  `1.2e-03`, not `0.0012`; zero stays `0`); a small step about a value of order one keeps its
  decimals (`1.0002`).
- Export CSV (table right-click menu) writes the whole table: RFC 4180,
  UTF-8, CRLF, headers on one line, numbers at full precision (the shortest text that reads back
  as the value; float32 results as their float). *(C++ port)*
- Open (Ctrl+O), Save (Ctrl+S), and Save As (Ctrl+Shift+S) in the File menu read and write
  HydroBEM model files (`*.hbm`, `FILES.md`, the files of `hb.Model.load` / `m.save`), read
  and written by the engine: the problem as the engine's objects with the meshes inside, the GUI
  state (visibility, editor modes) in the `tool` object `hydrobem_gui`; suppressed and switched-off
  objects are in the model with `enabled = false`. Opening
  replaces the model and clears the results. *(Rust port: TOML case files are gone from HydroBEM)*
- Save (Ctrl+S) writes HydroBEM's results file (`.hbr`, `FILES.md`: the model of the solve
  with its results) once Run has solved the project as it is, else the model file (`.hbm`): beside
  the project's file under its name (`barge.hbm` → `barge.hbr`), or where the dialog says when the
  project has no file. A change of the inputs after the Run makes Save write the model again (the
  results are not of it; the `.hbr` already written stays). The results file written becomes the
  project's file and the project saved; a results file opened is the project's file too, Save
  writing it again. Save As offers both file types then (results first) and writes the one chosen;
  without results of the project as it is, the model only. The File menu has Save, Save As and the
  Export submenu (no Save Project, Save Results As or Export Table as CSV). *(2026-11-08)*
- File → Export → NumPy (.npz), once there are results, writes every quantity of the results
  under its engine name, in the engine's type and dimensions, with `bodies`, the engine's motion
  equation (rigid inertia, stiffness, damping) and each body's position, yaw, rotation centre and
  trim of the model as solved, and `meta` (a JSON text of the run, the warnings and each key's
  axes, unit, description), readable by `numpy.load` without pickling (`FILES.md`): enough for a
  calculation of its own without the engine. *(2026-11-13)*
- File → Export, enabled once there are results, holds WAMIT (s), WAMIT (rad/s) and NumPy (above):
  results as files; the table shown exports as CSV from its own menu (no Excel export,
  removed 2026-11-13). WAMIT (s) and WAMIT (rad/s) write HydroBEM's
  WAMIT-format files, `NAME.out` and the numeric files of the same name (`.1`, `.2`, `.3`, `.4`,
  `.hst`, ...), as `Results.write_wamit` of the Python API does, with the period in s (WAMIT's
  IPERIN = 1) or the wave frequency in rad/s (IPERIN = 2) as their first column; files of that
  name already there are replaced. *(2026-11-08)*
- File → New Project (Ctrl+N) starts an empty project; File → Open Recent lists the last eight
  opened or saved files. New, Open, Open Recent, Open Results and closing the window ask to save unsaved changes
  (Save / Discard / Cancel). Closing the window while a solve runs asks first (Stop and Exit /
  Cancel): Stop and Exit stops the solve, and the window closes once it has stopped (after the
  unsaved prompt), never waiting for the engine. *(2026-11-04)* A project is unsaved when the model it would be saved as (its objects,
  each mesh as a checksum) differs from the one last opened, saved or created, so every input and
  GUI state counts. *(C++ port)*
- A .hbm dropped onto the window opens as File → Open Project, a .hbr as File → Open Results, with
  the same unsaved prompt; while it is dragged over the window a blue frame says what a drop opens.
  A mesh file (GDF or the engine's geometry file .hbg) is imported when dropped onto Meshes or one
  of its meshes in the tree (several at once, the last selected; the rows are framed while it is
  dragged over them, "Drop to import" on a white label) and replaces a mesh on a Source File field;
  anywhere else it does nothing. A CAD file (STEP, IGES) dropped anywhere is not read: the status
  bar says that cad2bem, a program of its own, converts it into a geometry file to import. Other
  files are refused in the status bar.
  During a drag from another program the window gets no mouse moves, so the drop targets use the
  system's cursor (GetCursorPos). *(Rust port)*
- The Run button (green, rounded, a play icon and "Run") sits near the right end of the menu row and
  starts the engine's solve (also F5 and Solve → Run). While a solve runs it turns into a red Stop
  button that requests cancellation. *(C++ port)* Clear Results right of it (and Solve → Clear Results)
  forgets the last solve; the Results node then says to Run. *(Rust port)*
- The inputs stay editable after a Run (and during one: the solve has the inputs at Run); the
  results stay shown. Once the inputs differ from those of the results (solved, or read with a
  results file), the results are out of date: the status bar's right end says "Results out of date"
  in amber (in place of "Completed · N frequencies"; the tooltip explains), and the details of every
  results node begin with that message. Undoing the change makes them current again; Run, Clear
  Results or opening a file ends it. Save writes the model then, not the results. *(2026-11-08)*

## Project tree

The tree is HydroBEM's input tree (`docs/reference/inputs.md` of HydroBEM) — the model's objects
first (meshes, bodies, lids, fixed dampers), then the environment, the first and second order and
the options — then its result tree (its global paths); each node is one engine object, list or result group, and every
label is the engine's name in title case (`first_order` → First Order, `field_pressure` → Field
Pressure, `rotation_center` → Rotation Center). A Details label cut short by its column shows
whole on hover. Only the GUI's own concepts keep names of their own (the frequency and heading
generators, the field-point grid, Panel Range, Visibility, Total). *(Rust port, 2026-11-01;
the engine's trees and names since 2026-11-10)*

```text
Meshes
  <mesh name>                 the mesh library
Bodies
  <body name>
    Additional Stiffness      the body's items, once each
    Additional Damping
    Mode Matrices             mode_mass, mode_damping, mode_stiffness
    Body Points
    Control Surface
    Dipole K                  the body's lists, any number each
    Dipole Damper K
    Tank K
    Pressure Surface K
    Free Surface Plate K      massless rigid plates in z = 0 (the water below only)
    Generalized Mode K
    Coupling K                one per other body
Lids
  <lid name>
Fixed Dampers
  Fixed Damper K              porous sheets at rest
Environment                   depth, density, gravity, domain, channel
  Omegas | Periods            environment/omegas or periods (by Frequency Input)
  Headings                    environment/headings
First Order                   radiation, diffraction, motion, the field and body outputs
  Field Points                first_order/field_points (and the points of the tanks)
Second Order                  the drifts, the QTFs, the potential QTF's free surface
Options                       how the engine solves
Results
  Hydrostatics
  Added Mass, Radiation Damping
  Excitation                  Diffraction, Froude Krylov, Scattering, Haskind
  Motion
  Body Points                 Pressure, Velocity
  Field Points                Pressure Radiation, Incident, Scattering, Diffraction, Total; Velocity
  Lids                        Elevation
  Pressure Surfaces           Elevation, Pressure Mode Damping
  Kochin, Drift, QTF
```

A result node is shown when the solve computed its quantity; a body is chosen in its Details (the
engine's `bodies[b]` part). Details and the output pane title a node by its path ("Lids ·
Elevation").

- Bodies, Lids and Fixed Dampers initially contain nothing; Meshes contains no mesh.
- Right-click Meshes → Import Mesh… (a GDF or .hbg file); right-click a mesh to export it as a
  geometry file (Export Mesh (.hbg)…, every mesh kind) or to delete it (only when unused). A patch
  mesh read from a .hbg keeps its discretization and its patch names, shown in a Name column of
  its Discretization table. A file without a discretization takes the recommended rows of the
  report HOPatch (cad2bem's former name) wrote with its protocol 1 beside it (`NAME.report.json`, matched by patch name, not older
  than the file by more than a minute), else 4 x 4 spans of order 3 (quadratic) per patch. With the report the
  table also has CAD Face, Face Type, State and
  Checks (the checks that are not ok), the mesh's Details name the report and count its ok, warn
  and fail patches (red when one failed), and the status bar says it was read. The report is
  saved with the project (the GUI state, by mesh name, while it names the mesh's patches); a
  project of an earlier GUI finds it again beside the file. A patch mesh shown alone colours the
  patches whose checks warn (amber, Check Warning) or fail (red, Check Failed). *(Rust port)*
- CAD files (STEP, IGES) are converted by cad2bem, a program of its own with its own GUI (the
  meshing tool before the solve); this GUI neither finds nor runs it. The geometry file (.hbg) it
  writes is imported as any mesh file, with its report when one is beside it (above). *(Import CAD
  and Reimport CAD, which ran cad2bem through its command line, removed 2026-11-07)*
- The native file dialogs that open or export (Open Project, Open Results, Import Mesh, Read Mesh
  Again, Export Mesh, Export) run on a thread of their own: the
  window keeps drawing meanwhile (the clock of a solve goes on). One at a time. *(2026-11-04)*
- Right-click Bodies → New Body; right-click Lids → New Lid; right-click Fixed Dampers → New
  Fixed Damper. Right-click a body, lid or fixed damper to delete it.
- Right-click a body to add its children: Additional Stiffness, Additional Damping, Mode Matrices,
  Body Points and Control Surface once each (no number); Dipole, Dipole Damper, Tank, Pressure
  Surface, Generalized Mode and Coupling any number of times (numbered 1, 2, … in their order).
  Right-click a child to delete it. A Free Surface Plate (any number) is records of the body mesh
  in z = 0, a library mesh in body axes, or Generated in a Waterline: the engine makes it, the
  water of z = 0 inside the waterline loop of a trimmed hull (this body's or another body's,
  Waterline Of) that holds the point Inside X, Y, in that hull's axes after its trim
  *(2026-11-12)*; a body of plates alone (no mesh of its own) solves with the mesh of its first
  plate. A Tank's Free Surface Plates (any number) are records of the body mesh or a library mesh
  at the tank's surface level (a deck, a cover) *(2026-11-12)*. The engine refuses plates with the ω = ∞ limit, the drifts and
  QTFs, a channel, wavemakers, trim and tanks, naming the reason. *(2026-11-11)*
- The viewport draws the problem as the engine builds it (its panels by role: wetted, interior,
  dipole, damper, pressure, tank, control surface, lid), in every symmetry copy; when the engine
  cannot build it the GUI draws the surfaces itself from the inputs, each in its colour (of a patch
  mesh too: the wetted patches as hull, the records of its interior surface, dipoles, pressure
  surfaces, dampers and tanks in theirs), and a note at the bottom of the viewport gives the
  engine's reason ("The engine cannot build the problem: …"). *(Rust port; patch meshes and the
  note since 2026-11-05)*
- View → Show Normals (off by default, remembered): a mesh shown alone (selected under Meshes)
  carries a red arrow from the centre of each panel or patch, in every symmetry copy, along its
  normal into the fluid as the engine takes it — of a panel its vertex order, of a patch the
  engine's surface normal at its middle — about 0.6 √area long (0.4 % to 6 % of the scene); at
  most 20 000 arrows (every k-th beyond). A pressure surface must point down, into the water.
  *(Rust port, 2026-11-05)*
- A new Body is collapsed by default; adding a child expands it.
- The mesh and the interior surface appear in Body Details, not as separate body child nodes.
- The viewport shows what the selection is: a mesh of the library alone, in its own coordinates;
  a body (or one of its children) alone, as placed; a lid alone. Meshes shows nothing (the meshes
  of a library overlap). Every other node shows all bodies, lids, walls and the field points.
- A patch mesh shown alone draws, besides its patch outlines, the spans of its discretization
  (legend "Spans (nu x nv)"; View → Show Spans, on by default, remembered): on each patch the curves between its nu x nv spans, the engine's even
  division of the patch's parameter square (`Model::patch_points`), in every symmetry copy. An edit
  of the discretization redraws them. The display tessellation is not the discretization.

## Body

- Body Details contain Body, Mesh, Interior Surface, Trim, Position, Rotation Center, Modes, Excitation and Inertia Properties.
- A body chooses its mesh from the library (Mesh: a drop-down with the library's meshes, "(none)" and
  "Import GDF…"). Panel Range off: every stored record of the mesh is the body's surface; on: the
  records Start Panel … End Panel. The records its IRR, dipoles and pressure surfaces take as panel
  ranges are left out of its wetted surface (a record has one role, the engine's rule).
- A mesh's Details show its file, title, panel counts, symmetry, ULEN, the engine's geometry of the
  mesh alone (volume, centre of volume, area, waterplane, bounds) and quality (panel sizes, aspect,
  orientation and edge findings), and who uses it. Its table has two tabs: Panels, one row per
  stored record with the engine's centroid, unit normal into the fluid and area
  (`Model::mesh_elements`), its size, aspect and quality flags (orientation, free edge,
  non-manifold); Vertices, the four vertices of each record as stored (a triangle repeats one).
  A patch mesh has Discretization (editable), Patches and Corners. The tab stays chosen by name.
- The Meshes table lists the library: name, kind, symmetry, records, the engine's volume, area and
  draft of the mesh alone, the counts of the quality findings and the users.
- With a mesh shown alone, the rows selected in its table (any tab) highlight their records on it,
  in every symmetry copy (filled orange, panels outlined in red); a click on the mesh picks the
  nearest record under the pointer, selects its rows in the table (scrolled to them) and names it
  in the status bar. A click on empty space in the viewport (a click, not a drag) releases the
  picked record or field point and the table's selection. *(Rust port; click away since
  2026-11-05)*
- Visibility affects display. Suppressed bodies are excluded from the solve: they stay in the model
  with `enabled = false`, and the engine solves as if they were absent (the couplings to them with
  them; a lid placed with one is refused: suppress it too). Renaming a body renames its references
  (lids placed with it, couplings, the field points' tank); deleting it removes them.
- A Coupling of a body names another body and holds two 6 x 6 matrices, Stiffness and Damping (the
  Matrix choice shows one, edited in Details or the table): this body's forces from the other's
  motions, about the origins of each body's additional matrices. The other direction is a coupling
  of the other body. *(Rust port, engine API 34)*
- Switching off a dipole, damper, tank or pressure surface (or suppressing a placed lid) solves as
  if it were absent; its records stay out of the body's wetted surface (the engine's references do
  not change).
- A surface given as records of the body mesh (dipole, damper, pressure surface, tank, tank sheet,
  IRR range, placed lid, generalized-mode piece) has Further Ranges besides Start/End Panel, typed
  as "129-140, 150"; a body has Unused Records, records inside its range no role takes.
- A higher-order GDF file imports as a patch mesh with its SPL file when there is one. Without
  one (a GDF file holds only the geometry) the import asks first, for every such file of the
  import at once (Higher-Order GDF: Discretization): a Panel Size [m] (the engine's rule, as
  WAMIT's PANEL_SIZE; chosen by default, 1 m) or Spans per Patch nu x nv (4 x 4), and the Spline
  Order (3), Outer and Inner Gauss Orders (3, 4), WAMIT's KSPLIN, IQUADO, IQUADI and their
  defaults. Import reads the files with it, Cancel reads none; nothing is read before. Reading a
  library mesh again from such a file (Source File) asks the same. The mesh's Details show the
  kind and the patches, its table the discretization per patch (editable). A body of a patch
  mesh is solved by the higher-order method. *(Rust port; the question since 2026-11-05)*
- The Details of a patch mesh say where its discretization rows come from (Discretization: the
  SPL file beside the GDF file; the spans per patch chosen at import; the default, 4 x 4 spans
  per patch of order 3 with Gauss 3 and 4, for a geometry file without rows; the geometry file's own; cad2bem's recommendation; the analytic
  shape's; a panel size of s m; edited in the table; as saved with the project) and the Unknowns
  they make: per stored patch (nu + ku - 1)(nv + kv - 1) B-spline coefficients, WAMIT's NEQN for
  the same patches. *(Rust port, 2026-11-05)*
- Panel Size [m] in the Details of a patch mesh read from a GDF file (a geometry file keeps the
  discretization it holds): a size typed and committed makes the engine read the file again with
  that panel size, as WAMIT's PANEL_SIZE — per patch n = floor(L / size) + 1 spans along each
  direction (L the longer of its two edges there), spline order 3, Gauss orders 3 and 4 — and
  every user takes the new rows (the status bar gives the spans in all). A relative source file
  (a shipped model) is beside the project. Refused (a message) when the file is gone or no longer
  has the mesh's patches. The size is shown until a row of the table is edited; the rows are saved,
  not the size. *(Rust port, 2026-11-05)*
- Meshes → New Analytic Mesh makes one of the engine's analytic shapes (Circular or Elliptic
  Cylinder, Sphere, Ellipsoid, Barge, Moonpool Cylinder, Cylinder with Dipole Discs) from its
  dimensions (`Model::analytic_mesh`), a quarter with symmetry `xz_and_yz`. Its Details (Analytic Shape)
  edit the shape, the dimensions, the Interior Free Surface Patch (the last patch), the
  Non-Uniform Mapping and the discs; every change makes the mesh again for its users (keeping the
  discretization per patch while the patches stay the same; the engine's refusal goes to the
  status bar and the shape stays). A body that takes such a mesh keeps Detect, which takes the lid;
  a body with no interior surface gets the lid patch as its Panel Range, and one on the previous
  mesh's lid goes back to Detect when the new mesh has none. *(Rust port; Detect 2026-11-14)*
- A Generalized Mode's Display Scale shows the hull displaced by that many units of the coordinate.
- Field Points have the Input Grid or Point List: a list of points, each with its tank (Tank 0 the
  exterior, k the k-th tank of the bodies), edited in the table; switching to a list starts from
  the grid's points. The results of listed points are shown as squares.
- The Field Points lie In Tank: Exterior or one tank of a body (by its body and place).
- Position is global translation. Rotation Center is in body-local mesh coordinates (R).
- `Mass` has exactly two choices (the engine's `mass`):
  - `Displaced` (default, `mass = "displaced"`): mass is read-only and equals displaced volume
    times environment density.
  - `Given`: mass is editable; switching from `Displaced` retains the last calculated value.
- Volume is always read-only and recalculated after mesh import or a Rotation Center change.
- Center of Gravity X/Y/Z (body-local) is part of Inertia Properties; it is required by the
  engine for motions. *(C++ port)*
- `Ixx`, `Ixy`, `Ixz`, `Iyy`, `Iyz`, and `Izz` are editable, default to zero, and are taken about R.
- Additional damping and stiffness, when added, are full 6x6 matrices about R. Details and table editing reflect one another immediately.
- The Interior Surface (irregular frequency removal) is `None`, `Projected`, `Waterline`, `Graded` *(C++ port)*, `Panel Range`, `Mesh` *(Rust port)* and
  `Detect` (the engine finds the wetted records lying in z = 0, an analytic mesh's lid among them;
  HydroBEM's default and a new body's *(2026-11-14)*). Start/End Panel are
  enabled only for Panel Range. The interior surface (generated or a selected range) is shown as the engine builds
  it, for display only: the solve passes the method and the engine builds the surface again. Panel ranges
  (IRR, dipole, pressure) are stored records and are shown in every symmetry copy. *(C++ port)*
- A body with a patch mesh offers None, Detect, Panel Range and Mesh, and Waterline when trimmed (the
  higher-order method takes the interior surface as patches: those of the body's mesh in z = 0 or a
  range of them, a patch mesh of its own, which the engine appends to the body's records, or the
  waterline's of a trimmed hull); another method kept from before is named in red, and so is a Mesh
  that is not a patch mesh. A body whose analytic mesh has its lid patch, when neither Detect, the
  IRR range nor the body's Panel Range leaves it out of the wetted surface, is warned in red that the
  lid would be solved as hull at z = 0 (Detect or Panel Range on the lid, or the lid off in the
  mesh's Details). *(Rust port; Detect 2026-11-14)*
- Each dipole and pressure surface takes a panel range of the body mesh or a mesh of the library,
  and can be switched off (kept, not solved). A pressure surface has Chamber Coupling, the Response
  Convention (Variational or Numeric Compatibility) and one or more pressure modes, each with a
  shape (Uniform, Sign of X, Sign of Y), stiffness and damping.
- The IRR can also be a mesh of the library (Method Mesh, body coordinates).
- Additional Damping and Stiffness are given About the rotation centre, the centre of gravity or a
  body-local point (the engine moves them).
- A Generalized Mode has a name, the rigid mode whose parity it has, and pieces: each a record range
  (or every record) of the body mesh with the polynomial terms c x^px y^py z^pz of u, v and w,
  edited in the table. Mode Matrices are the (6 + G)² mass, damping and stiffness of the rigid and
  generalized coordinates, edited in the table one at a time.
- A tank can hold sheets: dipoles or dipole dampers, record ranges of the body mesh.
- An automatic control surface has a circular or polygon outer boundary and partition polylines,
  their (x, y) points edited in the table.
- A damping lid can be Placed With a body: then it is a record range of that body's mesh, placed by
  its position and yaw, and those records leave the body's wetted surface.
- Field Points can have Further Depths (the same grid at each) and lie In Tank k; the result views
  then choose the depth.
- Trim (Body Details): Elevation, Pitch and Roll; the engine moves the mesh and cuts it at the free
  surface (a hull with freeboard). The viewport draws what the engine keeps, and what the cut takes
  away translucent grey ("Cut Off by Trim"), by the engine's rule x = R(pitch, roll) ξ + (0, 0,
  elevation). Excitation → Excitation Method: the exciting force of the motions by Diffraction
  (default, pressure integration) or Haskind.
- A body's dipole damper is a record range of the body's mesh (the engine's rule) with its damping
  factor; a water damper is a library mesh in global coordinates, at rest. Both can be switched off.
- A tank's walls are a record range of the body's mesh, normals into the tank; it has a density ratio
  and a free surface at the top of its walls or at a given body-local z.
- A control surface is Cylinder, Ellipsoid, Automatic (generated by the engine with a panel size) or
  a library Mesh, with the waterline term on the control surface or on the body.
- Body points are body-local, edited in their table (Count in Details); they replace the panels as
  the points of the body outputs (for every body or none, the engine's rule).

## Environment, options and outputs

- Environment: Depth Type, Depth (Finite only), Density, Gravity, Domain, Channel Width and
  Channel Reflect (Channel only). Options: Logarithmic Integration, Log Tolerance, Frequency and
  Inner Threads, the memory estimate. First Order: Radiation, Diffraction, Diffraction Formulation
  (the engine's `first_order` item since HydroBEM 5c72659), Motion, Field Pressure and Field Velocity (both greyed out without field points;
  Field Pressure on by default), Body Pressure, Body Velocity. Second Order: Drift — Pressure
  Integration, Control Surface, Momentum Conservation with its Momentum Conservation Angles (Auto
  or a number), and Bidirectional, the pairs of two headings of every drift; QTF — Quadratic
  Pressure Integration, Quadratic Control Surface, Potential Direct, Potential Indirect, Newman
  with its Newman Average (Geometric, Arithmetic), Difference, Sum, Bidirectional; with a
  potential QTF, Free Surface (Potential QTF): the Free Surface Method (Panels or Cells with
  their Free Surface Panel Size and Inner Radius, Auto or a number; or a library Mesh in z = 0
  with its Free Surface Quadrature Points), the Quadrature Annuli, Radius Step, Radial and
  Azimuthal Nodes, the Outer Circle Segments and the Expansion Order (the engine's `"auto"` as an
  Auto check). *(Rust port: the former Analysis node, split as the engine's objects)*

- First Order: Radiation *(C++ port)*, Diffraction, Motion (the RAOs), Logarithmic
  Integration, Log Integration Tolerance (default `1e-4`) *(C++ port)*, Diffraction Formulation
  (Scattered, Total, or Auto: the engine's choice, total with a lid *(Rust port)*),
  Domain, Channel Width (Channel only), Frequency Threads and Inner Threads *(C++ port; engine API 16:
  frequencies solved at a time, which multiplies the memory, and threads inside each frequency)*.
- Run compares the engine's memory estimate with the free memory first: above it, a dialog asks
  (Memory: Solve / Cancel); Solve runs that solve with the engine's `options.memory_check` off (the
  project keeps it on), so the engine does not refuse it again. *(2026-11-13)*
- While solving, the status bar shows only the frequencies finished, `Frequency done / total`
  (the limits counted), with a bar and the elapsed time; no stage or stage percentage.
  *(Rust port)*
- While solving, a button left of Run (three lines, Frequency Progress) opens a table below Run
  by worker, with the columns Worker, Status and Progress: one row per frequency worker of the
  engine (Frequency Threads; its own fixed numbers). Each row shows what the worker is on, as the
  engine reports it (`hb_model_progress`, read every frame without waiting for the solve): a
  limit (ω = 0, ω = ∞) or a frequency with its stage and ω ("Factorizing frequency 12 of 40, ω =
  0.6000 rad/s"; Solving while no stage is reported yet, Assembling, Factorizing, Computing the
  outputs of), an item of a second-order step ("Quadratic QTF, pressure integration: row ω_a 3
  of 20"), the work all workers share ("Preparing the influence data (all workers)", "Potential
  QTF, far-field fits (all workers)"), Idle (grey, an empty bar: no item left for it in the phase
  while others still run, and while the results are written) or Done once the solve has ended.
  The bar is the share of its item: blue with its percentage while it runs, green at 100% once
  done. Once the second order begins, its steps follow in the order they began, each with its
  share (blue, green at 100%, red "failed", "cancelled" after Stop). Below: "Completed done of
  total frequencies" (limits counted) and the elapsed time. The status bar shows the phase:
  Preparing with the preparation's share, Frequency done / total, Second order: the running step
  with its share, Writing the results. The table closes when the solve ends. *(Rust port; the
  engine's progress since 2026-11-04; the engine's workers and second order since 2026-11-12)*
- After a solve, engine warnings are listed in the Results summary and counted in the status bar;
  a failed solve names where the engine located the error and selects that body. *(C++ port)*
- Depth Type is `Infinite` or `Finite`; Depth is shown only for Finite.
- Density defaults to `1025 kg/m³`; gravity defaults to `9.80665 m/s²`.
- Domain choices are Open Sea, Wall at X = 0, Wall at Y = 0, Orthogonal Walls, and Channel.
- The viewport shows conceptual walls for wall/channel domains and a bottom plane for finite depth, only
  on the fluid side (x > 0, y > 0, both, or between the channel walls); the field-point plane is clipped
  the same way. *(C++ port)*
- Field points outside the fluid (beyond a wall, outside a channel, above z = 0, on or below the sea bed)
  are not submitted; Details count them separately from points inside bodies. *(C++ port)*

## Wave cases

- Wave Heading defaults: Start `0 deg`, End `360 deg`, Count `1`.
- Wave Frequency defaults: Start `0.1 rad/s`, End `2 rad/s`, Count `20`.
- Frequency Input is Omegas or Periods.
- The first editable/generated column follows Input Type; the other form, wave number, and wavelength are derived.
  The wave number comes from the engine (`hb_wavenumber`). *(C++ port)*
- Table edit mode keeps existing rows, disables Start and End, and lets Count resize the table.
- Units belong in property labels or table headers rather than separate unit columns.

## Field points and viewport

- Field points (Input) form a Grid (rectangular X/Y), a Polar Grid (rings of radius First Radius
  to Last Radius about a centre, by angle), a Body-Fitted Grid (rings First Ring Offset to Last Ring
  Offset outside the chosen body's section at Z, by angle about the section's centre; Convex
  Outline: about the section's convex hull and its area centroid, one outline about several
  columns, on by itself when the Body-Fitted Grid is first chosen for a section in parts (as a
  star it covers under 70 % of its hull's area), and the Details say so in red when it is off;
  Inside Rings: rings dividing the outline's radius evenly between the centre and it, for
  the water between columns, whose points in the body the engine leaves out; 10 of them when
  Convex Outline turns on by itself and there were none) or a Point
  List; Z and Further Depths give the levels. Angles from Angle Start to Angle End: a whole turn
  closes without repeating its first angle. Fit to Body (and the first choice of a ring layout)
  sizes the rings to the body's section at Z: Polar from 1.1 to 3 times its largest radius about
  its centre, Body-Fitted from a tenth of that radius outside it to twice the radius. *(Rust port)*
- The heatmap and the animated surface are drawn between the field points (the nodes of the
  grid): from the first ring or row to the last, never beyond the outermost points or across a point
  the engine left out; a grid of one row or ring (no cells) shows a square in plan about each
  point, half the distance to its nearest neighbour wide. Without a result the rings show as a
  translucent plane. *(Rust port)*
- Points the engine rejects (inside a body, beyond a wall, …) are excluded before the solve.
- Heatmaps show complex-amplitude magnitude, not time animation. Details show the amplitude range. *(C++ port)*
- Interpolate (bilinear between the points) is on by default and may be turned off in the
  free-surface Details (one colour per cell, the mean of its four points).
- Interpolation is visualization only; increasing Field Point counts is required for greater solver sampling resolution.
- Switching between free-surface quantities must preserve the camera.
- Left drag rotates, right drag pans (also middle drag or Shift + left drag), and the mouse wheel zooms
  (no hint is drawn) *(2026-11-14)*. Fit, Iso, Front, Top and Right are on the viewport's right-click
  menu (a right click without a drag); the colour legend of what is drawn sits at the top left.
- Table selection: a click selects a cell, a drag selects a block, a click or drag on the row
  numbers selects rows, on a header columns, on the Number header everything; Shift extends the
  selection. Selected cells (and a header whose whole column is selected) turn blue.

## Solve feedback

- Run displays elapsed time, a progress bar, and `Frequency completed / total`.
- Do not duplicate the same progress message in the general status text.
- The UI stays responsive during a solve; inputs are snapshotted when Run is pressed.

## Results

- Hydrostatics shows the engine's hydrostatics of the solve per body (body axes about R): displaced
  volume, centre of buoyancy, waterplane area and centroid, mass and centre of gravity, the
  metacentric heights GM = C44 / (ρ g V), C55 / (ρ g V) when the body has a mass and G, and the
  6 × 6 hydrostatic stiffness (with the weight terms) as its table. *(Rust port)*
- Hydrostatics is first in Results (the engine lists it last), then Added Mass and Radiation
  Damping, the rest in the order of the engine's paths. *(2026-11-11)*
- Coefficient Details select force body/direction and motion body/direction; one curve is displayed.
- Excitation contains Diffraction (the excitation force by pressure integration), Froude Krylov
  and Scattering (its parts), and Haskind (the whole excitation force by the Haskind relation, of
  the radiated coordinates only: compare with Diffraction). *(2026-11-11)*
- Wave-load Details select body, force direction, actual heading, and graph components.
- Motion (the RAOs) uses the same selection and presentation as the excitation. *(C++ port)*
- Wave-load and motion tables include Magnitude, Phase, Re, and Im.
- Graph modes are Magnitude/Phase (default) and Real/Imaginary. Phase graphs show `-180..180 deg`.
  Graphs can be zoomed and panned; a double click resets them. *(Rust port)*
- Graphs have axis labels with coefficient name and unit, but no title.
- Graph ticks follow the graph's size: labels at least about 52 px apart on x and 26 px on y, at
  steps of 1, 2 or 5 times a power of ten, with as many decimals as the step; finer unlabelled lines
  between them. *(Rust port)*
- Point i of a graph is row i of its table. A click within 16 px of a point selects its row (and
  scrolls to it); a row clicked in the table picks its point. The picked point is drawn larger in
  orange with its values in a box beside it, `x = ...` and `y = ...` (the axes name the
  quantities). A click on a graph away from its points releases the point and the table's
  selection. *(Rust port)*
- Field Points contains Pressure Radiation, Pressure Incident, Pressure Scattering, Pressure
  Diffraction (the free-surface elevation at z = 0, p/ρg below), Pressure Total (the GUI's sum) and
  Velocity.
- A heatmap has a colour bar with the quantity, unit and the colour levels it uses; its range is
  the solver values', interpolated or not. *(Rust port)*
- The submitted field points are marked in the viewport (View → Show Field Points). Clicking one shows
  its coordinates; under a free-surface result it selects its table row, and a selected row marks its
  point. *(C++ port)*
- A heatmap is visible only while a free-surface child is selected. Free-surface results use table plus heatmap and no graph.
- A radiation mode (free-surface Radiation, Kochin radiation, radiation point outputs) is chosen by
  its Radiating Body (the solved bodies by name: a suppressed body is not among them) and that
  body's Radiation Mode (Surge … Yaw, Generalized k, Pressure k). The Total adds the radiation of
  All Bodies, of one body (All Modes of it) or of one mode of it. *(Rust port)*
- Animation (Details of Motion, of the field-point pressure views on a grid, of Body Points ·
  Pressure and Velocity): Animate shows
  Re{X e^{iωt}} of the solved amplitudes at the wave amplitude of Details → Waves (the incident
  wave too), with a speed relative to real time and a Display Scale (×) of the displacements (1,
  the default, the real size; the typed value otherwise, positive). A player at the
  bottom of the viewport repeats one period: play/pause and stop (back to the start) at its left,
  the period as a bar to click or drag to a phase, the time t / T and the phase at its right. Motions move
  the bodies rigidly about R in body axes with the incident wave on the free surface; a free-surface
  view raises the field-point surface by the elevation (below z = 0 it colours only), coloured about
  zero, and moves the bodies its radiation includes (Total: their motions; Radiation: the radiating
  mode). Body Points · Pressure and Velocity colour the hull by Re{p e^{iωt}} or the chosen velocity
  component (signed, on a scale fixed over the period; |V| from zero), on the same faces as the
  amplitude view; Radiation moves the radiating body by its mode and the colours with it. Move Bodies
  (Radiation, and Total when motions were solved; on by default) keeps the bodies still when off;
  the surface still includes their radiation. It is a display of the linear
  result, not a larger-amplitude solution. *(Rust port)*
- The Results node shows a solve summary and the table of available native quantities with their shapes. *(C++ port)*
- Drift selects the method the solve computed (Pressure Integration, Control Surface: six
  components per body in body axes, the moments about the rotation centre R; their moments about
  the moving R; Momentum Conservation: surge, sway, yaw about the z axis of all bodies in global
  axes), the heading and the graphed component; the table
  has every component over frequency, in N/m² and N·m/m² per unit amplitude squared (N and N·m at another Wave Amplitude):
  the drift of the heading with itself. Momentum, Heading Pairs (bidirectional) is a table of the
  first heading against the second at one frequency and component: magnitude, real or imaginary
  part (the engine gives the pairs with the first heading not after the second; the others are
  their complex conjugates). *(Rust port)*
- QTF (when the solve computed one) selects the method (Quadratic Pressure Integration or Control
  Surface, Potential Direct or Indirect, Total of each quadratic with each potential part solved,
  Newman of each drift), the difference or sum frequencies,
  the heading pair (Newman: the heading), the body and its coordinate (Newman's momentum
  conservation: surge, sway, yaw in global axes; the others in body axes, the moments about the
  rotation centre R, as HydroBEM gives them since API 51) and the part shown; the table is ω_a against ω_b,
  the graph a heatmap of the matrix with a colour bar (hover for a value, click for its row), or
  magnitude and phase over ω_b for a chosen row ω_a (`RESULTS.md`). *(Rust port)*
- Kochin (when the solve computed them, with the momentum drift): diffraction (scattered,
  per wave amplitude) or radiation (per unit velocity of a mode), and their angular derivatives, over
  the engine's angles at one frequency and heading or mode; table and magnitude/phase graph.
  *(Rust port)*
- Omegas (Periods) Details have Added Mass Limits: Zero Frequency (ω = 0) and Infinite Frequency
  (ω = ∞), solved besides the wave frequencies (the engine's omegas 0 and inf, or periods inf and 0).
  Added Mass shows the limits with the frequencies: the selected coefficient at ω = 0 and ω = ∞ in
  Details and as two rows A(0), A(∞) after the frequencies in its table, as two points on its
  graph (A(0) at ω = 0, A(∞) at the right edge, named in a legend), and the 6 × 6 blocks of the
  chosen force and motion bodies at either limit as the tabs ω = 0 Matrix and ω = ∞ Matrix; there
  is no node of their own. *(2026-11-11)*
- Pressure Surfaces · Pressure Mode Damping (after Elevation): the engine's damping of the
  pressure coordinates (the pressure modes' additional damping, D x D). The coefficient, load and motion views choose among every coordinate of a
  body: Surge … Yaw, Generalized k, Pressure k. *(Rust port)*
- Lids · Elevation and Pressure Surfaces · Elevation: |η| and phase at the engine's lid and
  pressure-surface points (Diffraction per wave amplitude, Radiation per unit displacement), table
  and viewport colours. The surfaces are horizontal and their points (in the higher order the
  Gauss points of the patches) unevenly spaced: the viewport draws, per height, a Delaunay
  triangulation of the points in plan, each point at its own value and the colours interpolated
  between them, only over the surfaces as solved (their panels, or their patches tessellated), so
  nothing bridges a hole; nothing is drawn beyond the outermost points. The colour bar says
  "interpolated between points". *(Rust port; triangulated since 2026-11-05: squares of the
  points' spacing overlapped where the Gauss points are dense and left gaps where sparse)*
- Field points below z = 0 give the pressure p/ρg there (the engine's rule), labelled so in the
  free-surface views. *(Rust port)*
- Body Pressure (p/ρg: Diffraction per wave amplitude, Radiation per unit displacement) and
  Field Velocities (Incident, Scattering, Diffraction, Radiation) and Body Velocities (Diffraction,
  Radiation) are
  tables at the output points (global coordinates) for one frequency and heading or mode: magnitude
  and phase, or |u|, |v|, |w|. The viewport colours them: body values on the panel whose centroid
  the point is (low order) or, in the higher order, on the tessellated patch: every quadrilateral of
  the solved patches' tessellation (`Model::tessellation`, finer than the output cells) takes the
  value of the nearest output point, the centre of its uniform cell in the patch parameters, so each
  cell is coloured whole on the curved surface (no value between points is made up); other points
  (specified body points) as a square of their area; field velocities on the field-point grid;
  velocities by |u|, |v|, |w| or |V| (Colour By). A constant field (the incident velocity amplitude)
  is one colour. *(Rust port)*
- The viewport shows the free surface z = 0 as a translucent plane (View → Show Free Surface, on by
  default): in the open sea a square 1.6 times the bodies' plan size (not the field-point grid's);
  with walls or a channel over the walls' plan, so that it reaches the walls. A field-point plane at
  z = 0 replaces it where it lies. Walls stand from the sea bed to a freeboard of a tenth of the
  water column (at most a tenth of the scene) whatever the field-point grid's size; their length
  covers the bodies and the grid. *(Rust port)*

See `RESULTS.md` for indexing, units, and dynamic Total behavior.
