"""The NumPy export of the GUI (File → Export → NumPy, docs/FILES.md) from Python: the same
archive the GUI writes, for results solved or loaded in Python.

    import hydrobem_gui as hb

    r = hb.load_results("run.hbr")          # or m.solve()
    hb.save_npz(r, "run.npz")
    a = np.load("run.npz")                  # a["added_mass"], a["motion_equation_inertia"], ...

This module is HydroBEM GUI's own (not HydroBEM's package, which the rest of ``hydrobem_gui``
copies); it follows ``export_npz`` of ``crates/gui-core/src/files.rs``, and a test of each side
checks that the two write the same members and texts.
"""

from __future__ import annotations

import json
import os
from pathlib import Path

import numpy as np

from ._core import quantities as _quantities

#: The version of the ``meta`` array (``NPZ_VERSION`` of the GUI).
VERSION = 1

_AXES = "rigid coordinate (6 per body: surge, sway, heave, roll, pitch, yaw), rigid coordinate"

#: The axes, unit and description of each key that is not a quantity, as the GUI's ``meta``.
MODEL_KEYS = {
    "motion_equation_inertia": (
        _AXES,
        "kg, kg m, kg m^2",
        "the rigid-body mass and inertia matrix of the motion equation, body axes about each rotation centre R",
    ),
    "motion_equation_stiffness": (
        _AXES,
        "N/m, N, N m",
        "the stiffness of the motion equation: hydrostatic (rho g), weight and additional, body axes about each R",
    ),
    "motion_equation_damping": (
        _AXES,
        "N s/m, N s, N m s",
        "the additional (external) damping of the motion equation, body axes about each R",
    ),
    "body_position": ("body,xyz", "m", "the global position of the body origin"),
    "body_yaw": ("body", "deg", "the rotation of the body about the global z axis, applied before the translation"),
    "body_rotation_center": (
        "body,xyz",
        "m",
        "the motion reference point R, body-local: forces, matrices and motions are about R",
    ),
    "body_trim": ("body,elevation_pitch_roll", "m, deg, deg", "the trim of the hull in body coordinates (zero: none)"),
}

SECOND_ORDER_MOTIONS = (
    "the drifts and QTFs of a moving body are of its first-order motions `motion`, the RAOs the engine "
    "solved with motion_equation_* (and, for pressure and generalized coordinates, mode_restoring and "
    "pressure_mode_damping); a calculation with other stiffness or damping moves differently from what "
    "they assume. A fixed body has no motions in them"
)


def _text(value: str) -> np.ndarray:
    """A text as NumPy's Unicode type, at least one character wide (as the GUI writes it)."""
    return np.array(value, dtype=f"<U{max(len(value), 1)}")


def _texts(values) -> np.ndarray:
    width = max((len(v) for v in values), default=0)
    return np.array(list(values), dtype=f"<U{max(width, 1)}").reshape(len(values))


def arrays(results) -> dict[str, np.ndarray]:
    """The members of the archive, in order: every quantity the results have under its engine name,
    in the engine's type and dimensions (the F/H/D/K/P order); ``bodies``, the solved bodies'
    names (the B axis); ``motion_equation_inertia``, ``_stiffness``, ``_damping`` (6N x 6N, only
    when the motion equation is ready); ``body_position``, ``body_yaw``, ``body_rotation_center``,
    ``body_trim`` of the model as it was solved; ``meta``, a JSON text of the run, the diagnostics,
    each quantity's and key's axes, unit and description."""
    out: dict[str, np.ndarray] = {}
    info = {q["name"]: q for q in _quantities()}
    described = {}
    for name in results.available:
        a = results.quantity(name)
        if a is None:
            continue
        q = info[name]
        out[name] = np.ascontiguousarray(a, dtype=np.dtype(q["dtype"]).newbyteorder("<"))
        described[name] = {"dims": ",".join(q["dims"]), "unit": q["unit"], "description": q["description"]}

    solved = results._inputs()  # the model as it was solved: its bodies are the B axis
    bodies = list(solved.bodies)
    out["bodies"] = _texts([b.name for b in bodies])
    motion = solved.motion_equation()
    if motion.ready:
        for key in ("inertia", "stiffness", "damping"):
            out[f"motion_equation_{key}"] = np.array(getattr(motion, key), dtype="<f8")

    def reals(value, n: int, name: str) -> np.ndarray:
        if value is None and name == "trim":
            return np.zeros(n)
        v = np.atleast_1d(np.asarray(value, dtype=float))
        if v.shape != (n,):
            raise ValueError(f"the solved model's {name} of a body is {value!r}, not {n} numbers")
        return v

    count = len(bodies)
    out["body_position"] = np.array([reals(b.position, 3, "position") for b in bodies], dtype="<f8").reshape(count, 3)
    out["body_yaw"] = np.array([reals(b.yaw, 1, "yaw")[0] for b in bodies], dtype="<f8").reshape(count)
    out["body_rotation_center"] = np.array(
        [reals(b.rotation_center, 3, "rotation_center") for b in bodies], dtype="<f8"
    ).reshape(count, 3)
    out["body_trim"] = np.array([reals(b.trim, 3, "trim") for b in bodies], dtype="<f8").reshape(count, 3)

    keys = {k: v for k, v in MODEL_KEYS.items() if motion.ready or not k.startswith("motion_equation_")}
    run = results._r.meta()
    meta = {
        "format": "hydrobem-gui npz",
        "version": VERSION,
        "route": results.route,
        "run": None
        if run is None
        else {
            k: run[k]
            for k in (
                "hydrobem_version",
                "commit",
                "compiler",
                "linear_algebra",
                "build_type",
                "time",
                "api_version",
                "frequency_threads",
                "inner_threads",
                "engine_seconds",
            )
        },
        "warnings": [{"severity": d.severity, "code": d.code, "message": d.message} for d in results.diagnostics],
        "quantities": described,
        "motion_equation_ready": bool(motion.ready),
        "second_order_motions": SECOND_ORDER_MOTIONS,
        "model": {k: {"dims": d, "unit": u, "description": t} for k, (d, u, t) in keys.items()},
    }
    if meta["run"] is not None:
        meta["run"]["engine_seconds"] = list(meta["run"]["engine_seconds"])
    out["meta"] = _text(json.dumps(meta, sort_keys=True, separators=(",", ":"), ensure_ascii=False))
    return out


def save_npz(results, path) -> Path:
    """Writes the results as the GUI's NumPy archive (``.npz``, uncompressed; docs/FILES.md):
    every quantity under its engine name, ``bodies``, the motion equation, the bodies' placement
    and ``meta`` (see ``arrays``). Written to a temporary name beside ``path`` and renamed, so a
    failed write leaves a file there as it was. Returns the path."""
    path = Path(path)
    members = arrays(results)
    temp = path.with_name(f".{path.name}.saving-{os.getpid()}")
    try:
        with open(temp, "wb") as f:
            np.savez(f, **members)
        os.replace(temp, path)
    except BaseException:
        temp.unlink(missing_ok=True)
        raise
    return path


__all__ = ["MODEL_KEYS", "VERSION", "arrays", "save_npz"]
