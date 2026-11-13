"""Writes the test models of the Rust tests from HydroBEM's example cases (crates/*/tests/data).

Each model is the model an example script builds (its `build()`, imported from examples/), with
the outputs, frequencies and headings the tests need set on it here; a few meshes are written
beside them as files (GDF/SPL) for the tests that read a mesh file. `cargo xtask testdata` then
opens each model in the GUI and saves it with the GUI's state into the crates' tests/data
(crates/gui-core/tests/data/README.md).

    python scripts/make_testdata.py
    cargo xtask testdata

Run it with the hydrobem_gui package of this repository installed (docs/DEVELOPMENT.md). Tests
never read examples/; this script does, to make the files the tests read.
"""

import importlib.util
import math
import sys
from pathlib import Path

import numpy as np

import hydrobem_gui as hb

ROOT = Path(__file__).resolve().parent.parent
EXAMPLES = ROOT / "examples"
OUT = ROOT / "target" / "testdata"  # the models before the GUI saves them (cargo xtask testdata)


def example(folder, script="run.py"):
    """The module of an example script (its constants and build())."""
    path = EXAMPLES / folder / script
    spec = importlib.util.spec_from_file_location(f"example_{folder}_{path.stem}", path)
    module = importlib.util.module_from_spec(spec)
    sys.path.insert(0, str(path.parent))  # an example may import a helper beside it
    try:
        spec.loader.exec_module(module)
    finally:
        sys.path.pop(0)
    return module


def frequencies(m, omegas, headings):
    m.environment.update(frequency_input="omegas", omegas=list(omegas), headings=list(headings))


def grid(half, count, z=0.0):
    """A square grid of field points in the plane z, centred on the origin."""
    s = np.linspace(-half, half, count)
    x, y = np.meshgrid(s, s)
    return np.column_stack([x.ravel(), y.ravel(), np.full(x.size, z)])


def outside(m, points):
    """The points the engine keeps in the fluid (the GUI's rule for its grids)."""
    status = m.classify_points(points).status
    return np.asarray(points)[[s == "fluid" for s in status]]


def save(m, name):
    OUT.mkdir(parents=True, exist_ok=True)
    m.save(OUT / name, overwrite=True)
    print(f"wrote {name}")


def text(path, lines):
    path.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")  # LF on every OS


def write_mesh(m, mesh, name, spl=False):
    """A mesh of the model as a file beside the models (for the tests that read a mesh file)."""
    gdf = OUT / f"{name}.gdf"
    m.meshes[mesh].write(gdf, OUT / f"{name}.spl" if spl else None, overwrite=True)
    print(f"wrote {gdf.name}" + (f" and {name}.spl" if spl else ""))


def cylinder():
    """examples/cylinder: the floating cylinder R = 1 m, T = 0.5 m in deep water, a quadrant mesh;
    9 frequencies, 3 headings, motions and a field grid for the tests; beside a wall and in a
    channel."""
    ex = example("cylinder")
    m = ex.build()
    frequencies(m, np.linspace(0.4, 2.0, 9), [0.0, 45.0, 90.0])
    m.first_order.update(field_pressure=True, field_points=outside(m, grid(6.0, 25)))
    save(m, "cylinder.hbm")
    write_mesh(m, "cylinder", "cylinder")

    w = ex.build()
    w.bodies["cylinder"].update(position=[0.0, 2.0, 0.0])
    w.environment.update(domain="y_wall")
    w.first_order.update(motion=False)
    frequencies(w, np.linspace(0.5, 3.0, 11), [0.0, 45.0])
    save(w, "cylinder_wall.hbm")

    c = ex.build()
    c.environment.update(depth=5.0, domain="channel", channel_width=8.0)
    c.first_order.update(motion=False)
    frequencies(c, np.linspace(0.8, 3.0, 12), [0.0])
    save(c, "cylinder_channel.hbm")


def two_truncated_cylinders():
    """examples/two_truncated_cylinders: two cylinders D = 4 m side by side in 8 m of water, with
    the motions and a field grid between them."""
    ex = example("two_truncated_cylinders")
    m = ex.build()
    frequencies(m, m.environment.omegas, [0.0, 90.0])
    m.first_order.update(motion=True, field_pressure=True, field_points=outside(m, grid(8.0, 33)))
    save(m, "two_truncated_cylinders.hbm")
    write_mesh(m, "cylinder", "two_truncated_cylinders")


def truncated_cylinder_patches():
    """examples/truncated_cylinder_finite_depth/run_higher_order: the analytic cylinder R = 1 m,
    T = 1 m in 2 m of water; also as an analytic GDF file (IGDEF -1, the side and the bottom of a
    quadrant) with the spans of the example in its SPL file (the engine keeps an analytic mesh in
    the model file and does not write it as GDF)."""
    ex = example("truncated_cylinder_finite_depth", "run_higher_order.py")
    m = ex.build(ex.REFERENCE[:, 0])
    frequencies(m, m.environment.omegas, [0.0])
    save(m, "truncated_cylinder_patches.hbm")
    lines = [f"Truncated cylinder R = {ex.RADIUS:g} m, T = {ex.DRAFT:g} m: analytic patches (IGDEF -1)",
             f"1.0 {ex.GRAVITY}  ULEN GRAV", "1 1  ISX ISY", "2 -1  NPATCH IGDEF", "2  NLINES",
             f"{ex.RADIUS:g} {ex.DRAFT:g}", "0"]  # fmt: skip
    text(OUT / "truncated_cylinder_patches.gdf", lines)
    text(OUT / "truncated_cylinder_patches.spl", ["Truncated cylinder: B-spline spans of the potential per patch (NU NV)",
                                                  f"{ex.SIDE[0]} {ex.SIDE[1]}", f"{ex.BOTTOM[0]} {ex.BOTTOM[1]}"])
    print("wrote truncated_cylinder_patches.gdf and .spl")


def floating_owc_chamber():
    """examples/floating_owc_chamber: the chamber free in heave with its pressure surface (the
    chamber mode) at ka = 0.4, 1 and 2; and with a damping lid on the chamber instead."""
    ex = example("floating_owc_chamber")
    omegas = [math.sqrt(ka * ex.GRAVITY * math.tanh(ka / ex.A_OUT * ex.DEPTH) / ex.A_OUT) for ka in (0.4, 1.0, 2.0)]
    m = ex.build(omegas[0], True)
    frequencies(m, omegas, [0.0])
    save(m, "floating_owc_chamber.hbm")

    # the same tube with a damping lid (damping factor 0.1) on its chamber instead of the pressure
    # surface: the elements "chamber" of the example's mesh
    vertices, records = ex.tube()
    lid = hb.Model()
    lid.meshes.add(name="owc", kind="panels", vertices=vertices, reference_length=ex.A_OUT, symmetry="xz_and_yz",
                   label=m.meshes["owc"].label)  # fmt: skip
    lid.bodies.add(name="owc", mesh="owc", ranges=[records["hull"]], modes=["heave"], mass="displaced",
                   center_of_gravity=[0.0, 0.0, -0.5 * ex.DRAFT], interior_surface="mesh",
                   interior_surface_mesh="owc", interior_surface_ranges=[records["waterplane"]])  # fmt: skip
    lid.lids.add(name="chamber", mesh="owc", ranges=[records["chamber"]], placed_with="owc", damping_factor=0.1)
    lid.environment.update(depth=ex.DEPTH, density=ex.DENSITY, gravity=ex.GRAVITY)
    lid.first_order.update(motion=True)
    frequencies(lid, omegas, [0.0])
    save(lid, "floating_owc_chamber_lid.hbm")


def three_truncated_cylinders():
    """examples/three_truncated_cylinders: three cylinders on a triangle in 120 m of water, low order
    and higher order."""
    ex = example("three_truncated_cylinders")
    m = ex.build()
    frequencies(m, m.environment.omegas, [0.0])
    save(m, "three_truncated_cylinders.hbm")
    hx = example("three_truncated_cylinders", "run_higher_order.py")
    h = hx.build()
    frequencies(h, h.environment.omegas, [0.0])
    save(h, "three_truncated_cylinders_patches.hbm")
    # the three columns as one body: one mesh of the three cylinders where the example places them,
    # a section in parts (the body-fitted grid's convex outline)
    one = ex.build()
    frequencies(one, one.environment.omegas, [0.0])
    cylinder = one.meshes["cylinder"].vertices
    columns = np.concatenate([cylinder + np.asarray(b.position) for b in one.bodies])
    one.meshes.add(name="columns", kind="panels", vertices=columns, reference_length=1.0)
    names = [b.name for b in one.bodies]
    for b in names[1:]:
        del one.bodies[b]
    one.bodies[names[0]].update(mesh="columns", position=[0.0, 0.0, 0.0])
    one.bodies.rename(names[0], "columns")
    del one.meshes["cylinder"]
    save(one, "three_columns.hbm")


def heave_plate():
    """examples/cylinder_with_heave_plate/run_thin_plate: the cylinder with its thin heave plate, a
    dipole sheet, on a mesh three times coarser in each direction than the example's (its intervals
    and the panels around), for the tests' time (the example's mesh takes half a minute a
    frequency)."""
    ex = example("cylinder_with_heave_plate", "run_thin_plate.py")
    ex.HULL_SIZE, ex.SHEET_SIZE, ex.N_THETA = 3 * ex.HULL_SIZE, 3 * ex.SHEET_SIZE, ex.N_THETA // 3
    save(ex.build(), "cylinder_with_heave_plate.hbm")


def covered_tank():
    """examples/fish_tank_cylinders/run_covered_tank: the tank half covered, a free-surface plate
    of the tank (a mesh of its own) from the wall to the radius a / sqrt(2)."""
    ex = example("fish_tank_cylinders", "run_covered_tank.py")
    save(ex.build(inner=ex.COVERS["half covered"]), "fish_tank_covered.hbm")


def single(folder, name, script="run.py"):
    """An example's model as it builds it."""
    m = example(folder, script).build()
    save(m, name)


if __name__ == "__main__":
    cylinder()
    two_truncated_cylinders()
    truncated_cylinder_patches()
    floating_owc_chamber()
    three_truncated_cylinders()
    heave_plate()
    single("dock", "dock.hbm")
    single("fish_tank_cylinders", "fish_tank_cylinders.hbm")
    single("trimmed_moonpool_cover", "trimmed_moonpool_cover.hbm")
    covered_tank()
