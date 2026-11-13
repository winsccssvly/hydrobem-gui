# Result semantics and presentation

Indexing helpers live in `crates/gui-core/src/results.rs` (`SolveResult`) and are tested through
real solves in `crates/gui-core/tests/solver.rs`. Shapes always come from `Results::shape`.

## Complex convention

```text
magnitude = |z|
phase_deg = atan2(Im z, Re z) * 180 / pi
```

Phase graphs show `-180..180 deg` with grid lines every 45, 90 or 180° as the height allows; phase
table cells show three decimals. Graphs can be zoomed and panned (egui_plot); a double click
resets them.

## Hydrodynamic coefficients

Added Mass and Radiation Damping use `[F][D][K]`. The row is the force body/direction
(`coordinate_row`), the column is the K position of the motion body/direction's coordinate id
(`radiation_column`). If that mode was not solved, Details say so and no table is produced. The
added mass limits (`added_mass_zero_frequency`, `added_mass_infinite_frequency`, `[D][K]`) of the
same row and column follow the frequencies as rows A(0) and A(∞) and are two points of the graph;
ω = ∞ has no place on a frequency axis, so its point stands at the right edge (the legend says
so). Their whole 6 × 6 blocks of the two bodies are the tabs of Added Mass.

| Quantity | Translation/translation | One rotational side | Rotation/rotation |
|---|---|---|---|
| Added mass | `kg` | `kg·m` | `kg·m²` |
| Radiation damping | `N·s/m` | `N·s` | `N·m·s` |

The selected coefficient is labelled `A<row>,<id>` or `B<row>,<id>` (one-based), e.g. `A3,9 [kg·m]`.

## Wave loads and motions

Froude-Krylov, Scattering, Excitation and Motion use `[F][H][D]`. Haskind (`excitation.haskind`)
is `[F][H][K]`: its last axis the radiation columns, read for a coordinate through its column
(`SolveResult::haskind_series`, `radiation_mode_ids`); a coordinate not radiated has none.

- Force units: `N` (translations), `N·m` (rotations). Motion units: `m/m`, `rad/m` (per unit wave amplitude; at another Wave Amplitude `m`, `rad`: see Wave amplitude).
- The heading selector shows actual solved headings, not indices; it is shared by wave loads,
  motions, and free-surface results.
- Tables contain Magnitude, Phase, Re, and Im. Graph mode: Magnitude/Phase (default) or Real/Imaginary.

## QTF

The QTFs are `[F][F][P][D]` (complex; P the heading pairs `QtfHeadingPairs`, D the coordinates),
Newman's approximation `[F][F][H][D]` (real; `[F][F][H][3]` of the momentum conservation, global
surge, sway, yaw). The QTF view (`SolveResult::qtf_matrix`) shows one method, the difference or the
sum frequencies, one heading pair (a heading for Newman's) and one component (a body's coordinate,
by `body_coordinates`) as the table ω_a × ω_b (Magnitude, Real, Imaginary or Phase), and as a
heatmap of the same part (the viewport's colours, rows ω_a, columns ω_b, each cell reaching half
way to its neighbours; the cell under the pointer shows its value, a click picks its row in the
table) or, Graph → Row ω_a, magnitude and phase over ω_b for one row. Units `N/m²` or `N·m/m²` per unit wave amplitude
squared (A² times at another Wave Amplitude). The difference QTF of a frequency with itself is the
drift of its method (the diagonal); the potential part is zero there. Total (Method "Total,
Pressure Integration + Direct" and the other pairs solved) is the quadratic part plus the potential
part of the same frequency kind, heading pair and component, element by element
(`SolveResult::qtf_sum_of`): HydroBEM defines the total as that sum and gives the parts only, so
the sum is the GUI's.

## Wave amplitude

The engine's results are per metre of incident wave amplitude A (the mean drifts per square
metre); the radiation quantities are per unit motion (1 m, or 1 rad for a rotation). The views of
what the waves drive (wave loads, motions, free surface, drifts, Kochin functions, point values)
have Details → Waves → Wave Amplitude [m] (1 m at first): their values are shown for that A. The
rule is the engine's quantity table (`SolveResult::wave_scale`): a quantity over frequency and
heading scales with A, one whose unit is per m² with A²; the others (added mass, damping,
hydrostatics, the radiation per unit motion) do not change. The Total is incident + scattering +
Σ radiation × motion with the motions scaled, so A times as well. Units per amplitude become those of
the waves (`m/m` → `m`, `N/m²` → `N`, `m³/s per m` → `m³/s`) and the title adds "A = … m". Tables,
graphs, the heatmap, the animation and CSV export show the same values. The animation moves by them
(the incident wave of amplitude A as well) times its Display Scale, 1 (the real size) by
default. The amplitude is the
session's: it applies to every result shown, is not saved, and results files keep the engine's
per-unit values.

## Free-surface quantities

| GUI item | Source |
|---|---|
| Radiation | `FieldPressureRadiation` `[F][P][K]`, per unit displacement of the selected mode |
| Incident | `FieldPressureIncident` `[F][P][H]` |
| Scattering | `FieldPressureScattering` `[F][P][H]` |
| Pressure Diffraction | `FieldPressureDiffraction` `[F][P][H]`, the engine's fixed-body elevation (incident + scattering) |
| Total | dynamic composition below |

The engine computes no total; the GUI's Total is the sum HydroBEM states as the total of a moving
body (physics §10, over every radiation column, with the motions of the solve, which depend on
the mass through M and the weight terms of C):

```text
eta_total(f,p,h) = eta_incident(f,p,h) + eta_scattering(f,p,h)
                 + sum_k eta_radiation(f,p,k) * motion(f,h,id(k) - 1)
```

A Total of the body pressure would follow the same sum at the mean position; the pressure at the
point that moves with the body adds −Σ_k ξ_k u_z,k (physics §10). Neither is shown yet.

Total Details choose the radiating bodies: All Bodies, one body (every mode of it) or one mode of
that body. Without motions the GUI shows
fixed-body excitation and says that RAO was not solved.

## Field-point rendering

The solver receives only the grid points the engine accepts (`Model::classify_points`).
The heatmap is built in `crates/gui-core/src/scene/overlays.rs`. The heatmap normalizes
displayed values to their min–max (Details show the raw amplitude range) and uses a jet palette.
The field points are the nodes of the grid's lattice (Cartesian rows and columns, or rings and
angles); each cell between four solved nodes is filled between them. With Interpolate (on by
default) the cell is bilinear in its four values, drawn in 6 × 6 pieces each coloured by its own
value (64 colour bins), so a point shows its own value; off, the cell is one colour, the mean of its
corners (16 bins). Nothing is drawn beyond the outermost points or in a cell one of whose points the
engine left out (inside a body); a grid of one row shows a square of each point. The colour range
is the solver values' either way. This changes appearance only; it does not add BEM samples. The heatmap uses the grid stored with the result, so editing
Field Points after a solve does not misplace it.
