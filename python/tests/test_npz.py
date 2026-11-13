"""``hb.save_npz``: the GUI's NumPy archive from Python (HydroBEM GUI's own test, not copied from
HydroBEM). The members and texts are those of the GUI's ``export_npz``; the Rust test
``npz_python_texts_match`` checks that the two keep the same texts."""

import json

import numpy as np

import hydrobem_gui as hb

from bodies import lo_cosine_cylinder, lo_free_cylinder
from problems import G, model

W = np.sqrt(np.array([0.5, 1.2]) * G)
EXTRAS = ["bodies", "motion_equation_inertia", "motion_equation_stiffness", "motion_equation_damping",
          "body_position", "body_yaw", "body_rotation_center", "body_trim", "meta"]  # fmt: skip


def test_a_saved_results_file_writes_every_quantity_the_motion_equation_and_the_placement(tmp_path):
    body = lo_free_cylinder(position=[1.0, -2.0, 0.0], yaw=30.0, rotation_center=[0.0, 0.0, -0.1])
    m = model(body, W, [0.0, 45.0], options={"motion": True})
    m.solve()
    m.save_results(tmp_path / "run.hbr")
    r = hb.load_results(tmp_path / "run.hbr")

    out = hb.save_npz(r, tmp_path / "run.npz")
    hb.save_npz(r, out)  # again over it
    assert sorted(p.name for p in tmp_path.iterdir()) == ["run.hbr", "run.npz"]  # no temporary left
    with np.load(out) as z:
        assert list(z.files) == [*r.available, *EXTRAS]
        for name in r.available:
            a = r.quantity(name)
            assert z[name].dtype == a.dtype and z[name].shape == a.shape, name
            np.testing.assert_array_equal(z[name], a, err_msg=name)
        assert z["added_mass"].shape == (2, 6, 6)
        assert list(z["bodies"]) == ["body"]

        e = r.model.motion_equation()
        for key in ("inertia", "stiffness", "damping"):
            np.testing.assert_array_equal(z[f"motion_equation_{key}"], getattr(e, key))
        assert z["motion_equation_inertia"][0, 0] == r.hydrostatics.mass[0]
        np.testing.assert_array_equal(z["body_position"], [[1.0, -2.0, 0.0]])
        np.testing.assert_array_equal(z["body_yaw"], [30.0])
        np.testing.assert_array_equal(z["body_rotation_center"], [[0.0, 0.0, -0.1]])
        np.testing.assert_array_equal(z["body_trim"], [[0.0, 0.0, 0.0]])

        meta = json.loads(str(z["meta"]))
        assert z["meta"].shape == ()
    assert meta["format"] == "hydrobem-gui npz" and meta["version"] == 1
    assert meta["route"] == r.route
    assert meta["run"]["hydrobem_version"] == r.meta.hydrobem_version
    assert meta["quantities"]["added_mass"] == {
        "dims": ",".join(hb.quantities()["added_mass"].dims),
        "unit": hb.quantities()["added_mass"].unit,
        "description": hb.quantities()["added_mass"].description,
    }
    assert set(meta["quantities"]) == set(r.available)
    assert meta["motion_equation_ready"] is True
    assert "motion_equation" in meta["second_order_motions"]
    assert set(meta["model"]) == set(EXTRAS[1:-1])
    assert meta["model"]["body_yaw"]["unit"] == "deg"


def test_without_a_motion_equation_its_keys_are_left_out(tmp_path):
    # a fixed body: no mass, no modes
    r = model(lo_cosine_cylinder(fixed=True), W).solve()
    with np.load(hb.save_npz(r, tmp_path / "fixed.npz")) as z:
        assert not any(k.startswith("motion_equation_") for k in z.files)
        meta = json.loads(str(z["meta"]))
        assert z["body_position"].shape == (1, 3)
    assert meta["motion_equation_ready"] is False
    assert not any(k.startswith("motion_equation_") for k in meta["model"])
