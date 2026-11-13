"""hydrobem_gui._core: the engine, ``hydrobem.dll`` (its C ABI, ``hydrobem.h``), bound with ctypes.

The rest of the package builds its public API on this module:

- ``Model`` owns an engine model (``HbModel``); the solve runs with the GIL released (ctypes
  releases it during a foreign call), so the progress callback and other threads read the model
  and cancel the solve meanwhile.
- ``Object`` is an object of a model (the model and its id): its items, its lists of children,
  the functions of the engine on a mesh and the queries of a body.
- ``Results`` holds the results of a solve, of a results file or made from arrays; its
  quantities are NumPy arrays over the engine's data (read-only, not copied).
- The engine's errors raise the exceptions of ``hydrobem_gui.errors`` (by their kind, with the
  location); its diagnostics are ``Diagnostic``\\s.

The DLL is looked for in ``HYDROBEM_DLL`` (a path), then in the package's ``bin`` folder (the
wheel), then in ``engine/`` of the source tree (development).
"""

from __future__ import annotations

import ctypes as C
import os
from pathlib import Path

import numpy as np

from . import errors

# The version of the ABI this module is written for (HB_ABI_VERSION of hydrobem.h).
ABI_VERSION = 8

# ------------------------------------------------------------------ the library

_P = C.c_void_p
_S = C.c_int32  # HbStatus
_U64 = C.c_uint64
_I64 = C.c_int64
_I32 = C.c_int32
_F64 = C.c_double
_SZ = C.c_size_t
_STR = C.c_char_p
NO_OBJECT = 2**64 - 1


class _Location(C.Structure):
    _fields_ = [(n, _I64) for n in ("body", "mesh", "element", "field_point", "lid", "frequency")]


class _Column(C.Structure):
    _fields_ = [
        ("name", _STR),
        ("dtype", _I32),
        ("ndim", _SZ),
        ("shape", C.POINTER(_SZ)),
        ("count", _SZ),
        ("data", _P),
    ]


class _Progress(C.Structure):
    _fields_ = [
        ("stage", _I32),
        ("frequency", _I64),
        ("stage_done", _U64),
        ("stage_total", _U64),
        ("frequencies_done", _U64),
        ("frequencies_total", _U64),
        ("seconds", _F64),
        ("worker", _I32),
        ("phase", _I32),
        ("step", _I32),
        ("item", _I64),
    ]


_PROGRESS_FN = C.CFUNCTYPE(_I32, C.POINTER(_Progress), _P)


class _MeshOptions(C.Structure):
    _fields_ = [
        ("higher_order", _I32),
        ("zero_trailer", _I32),
        ("spl_path", _STR),
        ("discretization", C.POINTER(_I32)),
        ("discretization_rows", _SZ),
        ("panel_size", _F64),
        ("spline_order", _I32),
        ("outer_quadrature", _I32),
        ("inner_quadrature", _I32),
    ]


class _AnalyticShape(C.Structure):
    _fields_ = [
        ("shape", _STR),
        ("dimension_names", C.POINTER(_STR)),
        ("dimension_values", C.POINTER(_F64)),
        ("dimensions", _SZ),
        ("discs", C.POINTER(_F64)),
        ("disc_count", _SZ),
        ("parts", C.POINTER(_STR)),
        ("part_count", _SZ),
        ("mapping", _STR),
        ("discretization", C.POINTER(_I32)),
        ("discretization_rows", _SZ),
        ("reference_length", _F64),
        ("symmetry", _STR),
        ("label", _STR),
    ]


_PP = C.POINTER(_P)
_OUT_ID = C.POINTER(_U64)

# name: (result type, argument types)
_SIGNATURES = {
    "hb_abi_version": (C.c_uint32, []),
    "hb_string_free": (None, [_P]),
    "hb_last_error_message": (_STR, []),
    "hb_last_error_location": (_S, [C.POINTER(_Location)]),
    "hb_build_info": (_S, [_PP]),
    "hb_schema_items": (_S, [_PP]),
    "hb_schema_lists": (_S, [_PP]),
    "hb_quantities": (_S, [_PP]),
    "hb_result_views": (_S, [_PP]),
    "hb_result_groups": (_S, [_PP]),
    "hb_panel_roles": (_S, [_PP]),
    "hb_wavenumber": (_S, [_F64, _F64, _F64, C.POINTER(_F64)]),
    "hb_table_count": (_S, [_P, C.POINTER(_SZ)]),
    "hb_table_column": (_S, [_P, _SZ, C.POINTER(_Column)]),
    "hb_table_free": (None, [_P]),
    "hb_model_new": (_S, [_PP]),
    "hb_model_load": (_S, [_STR, _PP]),
    "hb_model_load_results": (_S, [_STR, _PP]),
    "hb_model_save": (_S, [_P, _STR, _I32]),
    "hb_model_save_results": (_S, [_P, _STR, _I32]),
    "hb_model_copy": (_S, [_P, _PP]),
    "hb_model_free": (_S, [_P]),
    "hb_model_state": (_S, [_P, C.POINTER(_I32)]),
    "hb_model_cancel": (_S, [_P]),
    "hb_model_progress": (_S, [_P, _PP]),
    "hb_model_reset": (_S, [_P]),
    "hb_model_clear": (_S, [_P]),
    "hb_model_check": (_S, [_P, _PP]),
    "hb_model_diagnostics": (_S, [_P, _PP]),
    "hb_model_solve": (_S, [_P, _PROGRESS_FN, _P, _PP]),
    "hb_model_results": (_S, [_P, _PP]),
    "hb_object": (_S, [_P, _STR, _OUT_ID]),
    "hb_object_info": (_S, [_P, _U64, _PP]),
    "hb_children": (_S, [_P, _U64, _STR, _PP]),
    "hb_add": (_S, [_P, _U64, _STR, _STR, _STR, _I64, _OUT_ID]),
    "hb_remove": (_S, [_P, _U64]),
    "hb_rename": (_S, [_P, _U64, _STR]),
    "hb_move": (_S, [_P, _U64, _I64]),
    "hb_clear": (_S, [_P, _U64, _STR]),
    "hb_item_info": (_S, [_P, _U64, _STR, _PP]),
    "hb_get": (_S, [_P, _U64, _STR, _PP]),
    "hb_set_bool": (_S, [_P, _U64, _STR, _I32]),
    "hb_set_int": (_S, [_P, _U64, _STR, _I64]),
    "hb_set_real": (_S, [_P, _U64, _STR, _F64]),
    "hb_set_text": (_S, [_P, _U64, _STR, _STR]),
    "hb_set_texts": (_S, [_P, _U64, _STR, C.POINTER(_STR), _SZ]),
    "hb_set_reals": (_S, [_P, _U64, _STR, C.POINTER(_F64), C.POINTER(_SZ), _SZ]),
    "hb_set_ints": (_S, [_P, _U64, _STR, C.POINTER(_I64), C.POINTER(_SZ), _SZ]),
    "hb_reset": (_S, [_P, _U64, _STR]),
    "hb_model_route": (_S, [_P, C.POINTER(_P)]),
    "hb_model_load_mesh": (_S, [_P, _STR, _STR, C.POINTER(_MeshOptions), _OUT_ID]),
    "hb_model_load_fdf": (_S, [_P, _STR, _STR, _STR, _OUT_ID]),
    "hb_model_load_csf": (_S, [_P, _STR, _STR, _I32, _U64, _OUT_ID]),
    "hb_model_mesh_from_hbg": (_S, [_P, _STR, _STR, _OUT_ID]),
    "hb_mesh_hbg": (_S, [_P, _U64, C.POINTER(_P)]),
    "hb_model_analytic_mesh": (_S, [_P, _STR, C.POINTER(_AnalyticShape), _OUT_ID]),
    "hb_body_trimmed_mesh": (_S, [_P, _U64, _STR, _STR, _OUT_ID]),
    "hb_mesh_write": (_S, [_P, _U64, _STR, _STR, _I32]),
    "hb_model_counts": (_S, [_P, _PP]),
    "hb_model_motion_equation": (_S, [_P, _PP]),
    "hb_model_panels": (_S, [_P, _STR, _I32, _PP]),
    "hb_model_free_surface_points": (_S, [_P, _STR, _PP]),
    "hb_model_tessellation": (_S, [_P, _STR, _I32, _I32, _I32, _PP]),
    "hb_model_mesh_quality": (_S, [_P, _PP]),
    "hb_model_mode_displacements": (_S, [_P, C.POINTER(_F64), C.POINTER(_I32), C.POINTER(_I32), _SZ, _PP]),
    "hb_model_memory_estimate": (_S, [_P, _PP]),
    "hb_model_classify_points": (_S, [_P, C.POINTER(_F64), _SZ, _U64, _PP]),
    "hb_body_hydrostatics": (_S, [_P, _U64, _PP]),
    "hb_body_panels": (_S, [_P, _U64, _STR, _I32, _PP]),
    "hb_body_mesh_quality": (_S, [_P, _U64, _PP]),
    "hb_body_tessellation": (_S, [_P, _U64, _STR, _I32, _I32, _I32, _PP]),
    "hb_mesh_geometry": (_S, [_P, _U64, _PP]),
    "hb_mesh_elements": (_S, [_P, _U64, _PP]),
    "hb_mesh_tessellation": (_S, [_P, _U64, _I32, _I32, _I32, _PP]),
    "hb_mesh_geometry_quality": (_S, [_P, _U64, _PP]),
    "hb_mesh_patch_points": (_S, [_P, _U64, _I32, C.POINTER(_F64), _SZ, _PP]),
    "hb_results_free": (None, [_P]),
    "hb_results_has": (_S, [_P, _STR, C.POINTER(_I32)]),
    "hb_results_quantity": (_S, [_P, _STR, C.POINTER(_Column)]),
    "hb_results_select": (_S, [_P, _STR, _I64, _I64, _PP]),
    "hb_results_body_columns": (_S, [_P, _I64, _PP]),
    "hb_results_selection": (_S, [_P, _STR, _I64, _I64, _PP]),
    "hb_results_check": (_S, [_P]),
    "hb_results_available": (_S, [_P, _PP]),
    "hb_results_route": (_S, [_P, C.POINTER(_P)]),
    "hb_results_diagnostics": (_S, [_P, _PP]),
    "hb_results_meta": (_S, [_P, _PP]),
    "hb_results_model": (_S, [_P, _PP]),
    "hb_results_solved_model": (_S, [_P, _PP]),
    "hb_results_normalized": (_S, [_P, _STR, _PP]),
    "hb_results_save": (_S, [_P, _STR, _I32]),
    "hb_results_write_wamit": (_S, [_P, _STR, _I32, _I32, _STR]),
    "hb_model_new_results": (_S, [_P, _PP]),
    "hb_results_put": (_S, [_P, _STR, _P, C.POINTER(_SZ), _SZ]),
}


def _candidates() -> list[Path]:
    here = Path(__file__).resolve().parent
    out = []
    if os.environ.get("HYDROBEM_DLL"):
        out.append(Path(os.environ["HYDROBEM_DLL"]))
    out.append(here / "bin" / "hydrobem.dll")  # the wheel
    out.append(here.parents[1] / "engine" / "hydrobem.dll")  # the source tree
    return out


def dll_path() -> Path:
    """The engine DLL this package loads."""
    for p in _candidates():
        if p.is_file():
            return p
    raise ImportError(
        "hydrobem.dll was not found (HYDROBEM_DLL, the package's bin folder, engine/ of the source tree)"
    )


def _load():
    path = dll_path()
    lib = C.CDLL(str(path))
    # the ABI first: a DLL of another ABI may lack some of the functions
    lib.hb_abi_version.restype = C.c_uint32
    lib.hb_abi_version.argtypes = []
    abi = lib.hb_abi_version()
    if abi != ABI_VERSION:
        raise ImportError(f"{path} has the C ABI {abi} of the engine; hydrobem_gui needs ABI {ABI_VERSION}")
    for name, (restype, argtypes) in _SIGNATURES.items():
        f = getattr(lib, name)
        f.restype = restype
        f.argtypes = argtypes
    return lib


_lib = _load()

# ------------------------------------------------------------------ errors and tables

_KINDS = {
    1: "value",
    2: "type",
    3: "unsupported",
    4: "state",
    5: "cancelled",
    6: "internal",
    7: "argument",
    8: "numerical",
    9: "io",
    10: "exists",
}
_FIELDS = ("body", "mesh", "element", "field_point", "lid", "frequency")


def _location(values) -> errors.Location:
    """A Location of the fields that apply (-1: none)."""
    return errors.Location(**{k: int(v) for k, v in zip(_FIELDS, values, strict=True) if int(v) >= 0})


def _check(status: int):
    if status == 0:
        return
    message = (_lib.hb_last_error_message() or b"").decode("utf-8", "replace")
    loc = _Location()
    _lib.hb_last_error_location(C.byref(loc))
    raise errors.exception_for(_KINDS.get(status, "internal"), message, _location([getattr(loc, f) for f in _FIELDS]))


def _text(s) -> bytes | None:
    if s is None:
        return None
    if not isinstance(s, str):
        s = os.fspath(s) if isinstance(s, os.PathLike) else str(s)
    return s.encode("utf-8")


def _owned_text(p: C.c_void_p) -> str:
    try:
        return C.cast(p, C.c_char_p).value.decode("utf-8") if p.value else ""
    finally:
        _lib.hb_string_free(p)


_ELEMENTS = {1: np.float64, 2: np.float32, 3: np.int32, 4: np.int64, 5: np.uint64, 6: np.complex64, 7: np.complex128}


def _frozen(a: np.ndarray) -> np.ndarray:
    a.flags.writeable = False
    return a


def _column_value(col: _Column, owner=None):
    """A column as a NumPy array of its shape (a copy, or a read-only view kept alive by ``owner``),
    a scalar for no dimensions, a list of texts for a text column."""
    shape = tuple(col.shape[k] for k in range(col.ndim))
    if col.dtype == 8:
        p = C.cast(col.data, C.POINTER(_STR))
        texts = [p[k].decode("utf-8") for k in range(col.count)]
        return texts[0] if not shape and texts else texts
    dtype = np.dtype(_ELEMENTS[col.dtype])
    if col.count == 0 or not col.data:
        a = np.zeros(shape, dtype)
    elif owner is not None:
        a = np.asarray(_View(col.data, shape, dtype, owner))
    else:
        buf = (C.c_char * (col.count * dtype.itemsize)).from_address(col.data)
        a = np.frombuffer(buf, dtype).copy().reshape(shape)
    if not shape and owner is None:
        return a.item()
    return _frozen(a)


class _View:
    """The array interface of elements the engine holds, kept alive by their owner."""

    def __init__(self, address, shape, dtype, owner):
        self._owner = owner
        self.__array_interface__ = {
            "data": (address, True),
            "shape": shape,
            "typestr": dtype.str,
            "version": 3,
        }


class _Table:
    def __init__(self):
        self.p = C.c_void_p()

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        _lib.hb_table_free(self.p)

    def dict(self) -> dict:
        n = _SZ()
        _check(_lib.hb_table_count(self.p, C.byref(n)))
        out = {}
        for k in range(n.value):
            col = _Column()
            _check(_lib.hb_table_column(self.p, k, C.byref(col)))
            out[col.name.decode("utf-8")] = _column_value(col)
        return out


def _table(fn, *args) -> dict:
    """The answer of a function that hands out a table, as a dict of its columns."""
    with _Table() as t:
        _check(fn(*args, C.byref(t.p)))
        return t.dict()


def _diagnostics(t: dict) -> list:
    return [
        errors.Diagnostic(
            "error" if t["severity"][k] else "warning",
            int(t["code"][k]),
            _location([t[f][k] for f in _FIELDS]),
            t["message"][k],
        )
        for k in range(len(t["message"]))
    ]


def _floats(a, n: int, what: str) -> np.ndarray:
    """A contiguous float64 array of rows of n."""
    a = np.ascontiguousarray(a, dtype=np.float64)
    if a.ndim != 2 or a.shape[1] != n:
        raise ValueError(f"{what} must have the shape (n, {n})")
    return a


def _ptr(a: np.ndarray, ctype):
    return a.ctypes.data_as(C.POINTER(ctype))


def _texts(v) -> tuple:
    """A C array of UTF-8 texts (and the bytes it points to)."""
    b = [_text(s) for s in v]
    return (_STR * len(b))(*b), b


def _rows6(d) -> np.ndarray:
    a = np.ascontiguousarray(np.asarray(d, dtype=np.int32).reshape(-1, 6) if d is not None and len(d) else
                             np.zeros((0, 6), np.int32))  # fmt: skip
    return a


# ------------------------------------------------------------------ the module's functions

_SCHEMA = None
_QUANTITIES = None
_VIEWS = None
_NUMPY_NAMES = {1: "float64", 2: "float32", 3: "int32", 4: "int64", 6: "complex64", 7: "complex128", 0: "str"}


def schema() -> dict:
    """Per object type its items (kind, dtype, shape, unit, default, choices, target, mesh kinds,
    the switch items that choose it and their values that take it, description) and its lists
    (types, named, optional_names, single, always, description)."""
    global _SCHEMA
    if _SCHEMA is None:
        items = _table(_lib.hb_schema_items)
        lists = _table(_lib.hb_schema_lists)
        out: dict = {}
        for k, t in enumerate(items["type"]):
            entry = out.setdefault(t, {"items": {}, "lists": {}})
            kind = items["kind"][k]
            default = items["default"][k]
            entry["items"][items["item"][k]] = {
                "kind": kind,
                # the element type of an array item; "real" for the others (as the extension module)
                "dtype": items["dtype"][k] if kind == "array" else "real",
                "shape": items["shape"][k],
                "unit": items["unit"][k],
                "default": default if default != "" else None,
                "choices": tuple(items["choices"][k].split()),
                "target": items["target"][k],
                "kinds": tuple(items["kinds"][k].split()),
                "switch": tuple(items["switch"][k].split()),
                "switch_values": tuple(items["switch_values"][k].split()),
                "description": items["description"][k],
            }
        for k, t in enumerate(lists["type"]):
            entry = out.setdefault(t, {"items": {}, "lists": {}})
            entry["lists"][lists["list"][k]] = {
                "types": tuple(lists["types"][k].split()),
                "named": bool(lists["named"][k]),
                "optional_names": bool(lists["optional_names"][k]),
                "single": bool(lists["single"][k]),
                "always": bool(lists["always"][k]),
                "description": lists["description"][k],
            }
        _SCHEMA = _ordered(out)
    return _SCHEMA


def _ordered(types: dict) -> dict:
    """The types in the engine's order: the tables list them by their items, then by their lists;
    a type without items (the model) goes before the first type it holds."""
    order = [t for t, v in types.items() if v["items"]]
    for t, v in types.items():
        if v["items"]:
            continue
        held = [h for lst in v["lists"].values() for h in lst["types"]]
        at = min((order.index(h) for h in held if h in order), default=len(order))
        order.insert(at, t)
    return {t: types[t] for t in order}


def _quantity_rows() -> list:
    global _QUANTITIES
    if _QUANTITIES is None:
        t = _table(_lib.hb_quantities)
        _QUANTITIES = [
            {
                "name": t["name"][k],
                "dtype": _NUMPY_NAMES[int(t["dtype"][k])],
                "dims": tuple(t["dims"][k].split(",")),
                "unit": t["unit"][k],
                "description": t["description"][k],
            }
            for k in range(len(t["name"]))
        ]
    return _QUANTITIES


def quantities() -> list:
    """The result quantities in the order of their codes: name, NumPy dtype, axes, unit, description."""
    return [dict(q) for q in _quantity_rows()]


def _quantity_name(code: int) -> str:
    rows = _quantity_rows()
    if not 0 <= code < len(rows):
        raise ValueError(f"no result quantity {code}")
    return rows[code]["name"]


def _view_rows() -> list:
    global _VIEWS
    if _VIEWS is None:
        t = _table(_lib.hb_result_views)
        codes = {q["name"]: k for k, q in enumerate(_quantity_rows())}
        _VIEWS = [
            {
                "path": t["path"][k],
                "quantity": t["quantity"][k],
                "code": codes[t["quantity"][k]],
                "owner": t["owner"][k],
                "width": int(t["width"][k]),
            }
            for k in range(len(t["path"]))
        ]
    return _VIEWS


def result_views() -> list:
    """The views of the result tree in the order of the quantity table: path, quantity (its name),
    code, owner (the rule's name) and width (the columns of a body of the rule "columns", else 0)."""
    return [dict(v) for v in _view_rows()]


def result_groups() -> list:
    """The groups of the result tree, parents first: path, description."""
    t = _table(_lib.hb_result_groups)
    return [{"path": p, "description": d} for p, d in zip(t["path"], t["description"], strict=True)]


def build_info() -> dict:
    """The build of the engine."""
    t = _table(_lib.hb_build_info)
    return {
        "version": t["version"],
        "commit": t["commit"],
        "compiler": t["compiler"],
        "linear_algebra": t["linear_algebra"],
        "build_type": t["build_type"],
        "api_version": int(t["api_version"]),
        "avx2": bool(t["avx2"]),
    }


def wavenumber(omega: float, depth: float, gravity: float) -> float:
    """The wave number of omega^2 = g k tanh(k h)."""
    k = _F64()
    _check(_lib.hb_wavenumber(omega, depth, gravity, C.byref(k)))
    return k.value


PANEL_ROLES = tuple(_table(_lib.hb_panel_roles)["names"])
QUALITY_FLAGS = {"orientation": 1, "free_edge": 2, "non_manifold": 4, "coarse": 8}

# ------------------------------------------------------------------ answers as dicts


def _hydrostatics(t: dict) -> dict:
    return {
        "volumes": list(t["volumes"]),
        "volume": t["volume"],
        "displaced_mass": t["displaced_mass"],
        "center_of_buoyancy": list(t["center_of_buoyancy"]),
        "waterplane_area": t["waterplane_area"],
        "waterplane_centroid": list(t["waterplane_centroid"]),
        "stiffness_rho_g": t["stiffness_rho_g"],
        "has_weight": bool(t["has_weight"]),
        "mass": t["mass"],
        "center_of_gravity": list(t["center_of_gravity"]),
        "stiffness": t["stiffness"],
        "metacentric_height": list(t["metacentric_height"]),
    }


def _named(t: dict, names: tuple) -> dict:
    return {k: t[k] for k in names}


_KEYS = ("body", "role", "tank", "owner", "element", "part", "reflection")
_PANELS = ("vertices", "distinct_vertices", *_KEYS, "centroids", "normals", "areas")
_TESSELLATION = ("vertices", "normals", *_KEYS)
_QUALITY = (
    "body", "element", "part", "size", "aspect", "per_wavelength", "flags", "volumes", "min_per_wavelength", "edges",
)  # fmt: skip
_MESH_ELEMENTS = ("element", "vertices", "distinct_vertices", "centroids", "normals", "areas")


def _role_text(roles):
    """The role argument of the C ABI: the names separated by commas, null for the default roles."""
    return _text(",".join(roles)) if roles else None

# ------------------------------------------------------------------ Model

_STATES = {0: "reset", 1: "calculating", 2: "calculated"}
_STAGES = ("statics", "assembly", "lu", "outputs", "frequency_done", "second_order", "phase", "worker_idle")
# HB_PHASE_ and HB_STEP_ of hydrobem.h, by their codes
_PHASES = ("preparation", "limits", "first_order", "second_order", "finishing", "done", "cancelled", "failed")
_STEPS = (
    "quadratic_pressure_integration", "quadratic_control_surface", "potential_first_order", "potential_radiation",
    "potential_fits", "potential_outer", "potential_pairs", "potential_direct",
)  # fmt: skip


def _index(i):
    """A zero-based index of the C ABI, None for -1 (none)."""
    return None if i < 0 else int(i)


class Model:
    """An engine model."""

    def __init__(self, _p=None):
        if _p is None:
            p = C.c_void_p()
            _check(_lib.hb_model_new(C.byref(p)))
            _p = p
        self._p = _p

    def __del__(self, _free=_lib.hb_model_free):
        # the free function bound at definition: the module's globals may be gone at shutdown
        p = getattr(self, "_p", None)
        if p is not None and p.value:
            try:
                _free(p)
            except Exception:  # noqa: BLE001 (nothing to report to at shutdown)
                pass

    @staticmethod
    def _made(fn, *args) -> Model:
        p = C.c_void_p()
        _check(fn(*args, C.byref(p)))
        return Model(p)

    def _h(self, closed="the model is closed"):
        if self._p is None:
            raise RuntimeError(closed)
        return self._p

    @staticmethod
    def load(path) -> Model:
        return Model._made(_lib.hb_model_load, _text(path))

    @staticmethod
    def load_results(path) -> Model:
        return Model._made(_lib.hb_model_load_results, _text(path))

    def save(self, path, overwrite=False):
        _check(_lib.hb_model_save(self._h(), _text(path), int(overwrite)))

    def save_results(self, path, overwrite=False):
        _check(_lib.hb_model_save_results(self._h(), _text(path), int(overwrite)))

    def copy(self) -> Model:
        return Model._made(_lib.hb_model_copy, self._h())

    def close(self):
        """Frees the engine model. Not while it solves (RuntimeError, nothing freed); the caller does
        not close a model another thread is still using (its calls would read a freed model)."""
        if self._p is None:
            return
        if self.state == "calculating":
            raise RuntimeError("the model is calculating: it can be closed once the solve ends (nothing was freed)")
        status = _lib.hb_model_free(self._p)
        if status == 4:
            raise RuntimeError("the model is calculating: it can be closed once the solve ends (nothing was freed)")
        _check(status)
        self._p = None

    @property
    def closed(self) -> bool:
        return self._p is None

    @property
    def state(self) -> str:
        s = _I32()
        _check(_lib.hb_model_state(self._h(), C.byref(s)))
        return _STATES[s.value]

    def cancel(self):
        if self._p is not None:
            _lib.hb_model_cancel(self._p)

    def progress(self):
        t = _table(_lib.hb_model_progress, self._h())

        def row(state, stage, fraction, worker):
            return (state, stage or None, float(fraction), _index(worker))

        limits = [
            (t["limit"][k], row(t["limit_state"][k], t["limit_stage"][k], t["limit_fraction"][k], t["limit_worker"][k]))
            for k in range(len(t["limit"]))
        ]  # fmt: skip
        frequencies = [row(t["state"][k], t["stage"][k], t["fraction"][k], t["worker"][k]) for k in range(len(t["state"]))]
        workers = [
            (t["worker_state"][k], t["worker_task"][k] or None, _index(t["worker_index"][k]),
             t["worker_stage"][k] or None, float(t["worker_fraction"][k]))
            for k in range(len(t["worker_state"]))
        ]  # fmt: skip
        items = [row(t["item_state"][k], t["item_stage"][k], t["item_fraction"][k], t["item_worker"][k])
                 for k in range(len(t["item_state"]))]  # fmt: skip
        steps, first = [], 0
        for k in range(len(t["step"])):
            n = int(t["step_items"][k])
            steps.append((t["step"][k], t["step_state"][k], float(t["step_fraction"][k]), items[first:first + n]))
            first += n
        return (
            bool(t["running"]), float(t["seconds"]), float(t["preparation"]), limits, frequencies,
            int(t["done"]), int(t["total"]), t["phase"][0] or None, workers, steps,
        )  # fmt: skip

    def check(self) -> list:
        with _Table() as t:
            _check(_lib.hb_model_check(self._h(), C.byref(t.p)))
            return _diagnostics(t.dict())

    def warnings(self) -> list:
        return _diagnostics(_table(_lib.hb_model_diagnostics, self._h()))

    def object(self, path="") -> Object:
        oid = _U64()
        _check(_lib.hb_object(self._h(), _text(path), C.byref(oid)))
        return Object(self, oid.value)

    def route(self) -> str:
        out = C.c_void_p()
        _check(_lib.hb_model_route(self._h(), C.byref(out)))
        return _owned_text(out)

    def counts(self) -> dict:
        t = _table(_lib.hb_model_counts, self._h())
        x = {
            k: int(t[k])
            for k in (
                "bodies", "meshes", "lids", "ordinary_frequencies", "headings", "coordinates", "radiation_columns",
                "field_points", "tank_points", "lid_elements", "pressure_elements",
            )
        }  # fmt: skip
        x["has_zero_frequency"] = bool(t["has_zero_frequency"])
        x["has_infinite_frequency"] = bool(t["has_infinite_frequency"])
        x["radiation_mode_ids"] = [int(v) for v in np.atleast_1d(t["radiation_mode_ids"])]
        x["per_lid_elements"] = [int(v) for v in np.atleast_1d(t["per_lid_elements"])]
        fields = (
            "stored", "wetted", "wetted_with_reflections", "dipole", "interior_surface", "pressure_modes", "symmetry_copies",
            "generalized_modes",
        )  # fmt: skip
        x["per_body"] = [{f: int(t[f][b]) for f in fields} for b in range(len(t["stored"]))]
        return x

    def motion_equation(self):
        t = _table(_lib.hb_model_motion_equation, self._h())
        return t["inertia"], t["stiffness"], t["damping"], bool(t["ready"])

    def panels(self, include_reflections, roles) -> dict:
        return _named(_table(_lib.hb_model_panels, self._h(), _role_text(roles), int(include_reflections)), _PANELS)

    def free_surface_points(self, zone):
        """The points (n, 3) and weights (n,) of one zone of the potential QTF's free surface."""
        t = _table(_lib.hb_model_free_surface_points, self._h(), _text(zone))
        return np.array(t["points"], dtype=np.float64).reshape(-1, 3), np.array(t["weights"], dtype=np.float64)

    def tessellation(self, roles, divisions, include_reflections) -> dict:
        du, dv = divisions
        t = _table(_lib.hb_model_tessellation, self._h(), _role_text(roles), int(du), int(dv), int(include_reflections))
        return _named(t, _TESSELLATION)

    def mesh_quality(self) -> dict:
        return _named(_table(_lib.hb_model_mesh_quality, self._h()), _QUALITY)

    def mode_displacements(self, points, bodies, elements):
        pts = _floats(points, 3, "points")
        n = len(pts)
        b = np.ascontiguousarray(bodies, dtype=np.int32)
        r = np.ascontiguousarray(elements, dtype=np.int32)
        if len(b) != n or len(r) != n:
            raise ValueError("one body and one element per point")
        return _table(_lib.hb_model_mode_displacements, self._h(), _ptr(pts, _F64), _ptr(b, _I32), _ptr(r, _I32), n)[
            "values"
        ]

    def memory_estimate(self) -> dict:
        t = _table(_lib.hb_model_memory_estimate, self._h())
        return {k: int(t[k]) for k in ("statics", "per_frequency", "outputs", "peak", "commit")}

    def classify_points(self, points, tank=None):
        """The verdict of the field-point rules on points in the sea (tank None) or in a tank (its
        object; body coordinates): (status names, the body, the lid and the tank of each, -1 none)."""
        pts = _floats(points, 3, "points")
        if tank is not None and not isinstance(tank, Object):
            raise TypeError("tank must be None or an object of the model")
        t = _table(_lib.hb_model_classify_points, self._h(), _ptr(pts, _F64), len(pts), 0 if tank is None else tank._id)
        status = t["status"]
        def column(name):
            return _frozen(np.atleast_1d(np.asarray(t[name], dtype=np.int32)))

        return ([status] if isinstance(status, str) else list(status)), column("body"), column("lid"), column("tank")

    def new_results(self, quantities) -> Results:
        r = C.c_void_p()
        _check(_lib.hb_model_new_results(self._h(), C.byref(r)))
        results = Results(r)
        names = [q["name"] for q in _quantity_rows()]
        for name, a in quantities:
            if name not in names:
                raise ValueError(f"no result quantity {name!r}")
            dtype = _quantity_rows()[names.index(name)]["dtype"]
            if dtype == "str":
                raise ValueError(f"{name} holds text")
            data = np.ascontiguousarray(a, dtype=dtype)
            shape = (_SZ * max(data.ndim, 1))(*data.shape)
            _check(_lib.hb_results_put(r, _text(name), data.ctypes.data_as(_P), shape, data.ndim))
        # the quantities given together, once all are put
        _check(_lib.hb_results_check(r))
        return results

    def solve(self, progress=None) -> Results:
        raised = []

        def report(p, _user):
            if raised:
                return 0
            r = p.contents
            # -1 a limit, -2 the statics, -3 a second-order item, -4 the solve as a whole
            frequency = -1 if r.frequency == -1 else _index(r.frequency)
            try:
                stage = _STAGES[r.stage] if 0 <= r.stage < len(_STAGES) else "frequency_done"
                phase = _PHASES[r.phase] if 0 <= r.phase < len(_PHASES) else "preparation"
                step = _STEPS[r.step] if 0 <= r.step < len(_STEPS) else None
                go_on = progress(
                    (stage, frequency, r.stage_done, r.stage_total, r.frequencies_done, r.frequencies_total, r.seconds,
                     _index(r.worker), phase, step, _index(r.item))
                )  # fmt: skip
            except BaseException as e:  # noqa: BLE001 (raised again once the solve ends)
                raised.append(e)
                return 0
            return 0 if go_on is False else 1

        callback = _PROGRESS_FN(report) if progress is not None else _PROGRESS_FN()
        out = C.c_void_p()
        status = _lib.hb_model_solve(self._h(), callback, None, C.byref(out))
        results = Results(out) if out.value else None
        if raised and status == 5:
            raise raised[0]
        _check(status)
        return results

    def results(self) -> Results:
        out = C.c_void_p()
        _check(_lib.hb_model_results(self._h(), C.byref(out)))
        return Results(out)

    def reset(self):
        _check(_lib.hb_model_reset(self._h()))

    def clear(self):
        _check(_lib.hb_model_clear(self._h()))

    def _object(self, fn, *args) -> Object:
        oid = _U64()
        _check(fn(self._h(), *args, C.byref(oid)))
        return Object(self, oid.value)

    def load_mesh(
        self,
        name,
        path,
        higher_order=False,
        zero_trailer=False,
        spl=None,
        discretization=None,
        panel_size=None,
        spline_order=None,
        outer_quadrature=None,
        inner_quadrature=None,
    ) -> Object:
        if panel_size is not None and not panel_size > 0:
            raise ValueError(f"panel_size must be > 0, got {panel_size}")
        if discretization is not None and len(discretization) == 0:
            raise ValueError("discretization: one row per patch, none given")
        rows = _rows6(discretization) if discretization is not None else None
        o = _MeshOptions(
            higher_order=int(bool(higher_order)),
            zero_trailer=int(bool(zero_trailer)),
            spl_path=_text(spl),
            discretization=_ptr(rows, _I32) if rows is not None and len(rows) else None,
            discretization_rows=len(rows) if rows is not None else 0,
            panel_size=float(panel_size) if panel_size is not None else 0.0,
            spline_order=int(spline_order) if spline_order is not None else 0,
            outer_quadrature=int(outer_quadrature) if outer_quadrature is not None else 0,
            inner_quadrature=int(inner_quadrature) if inner_quadrature is not None else 0,
        )
        return self._object(_lib.hb_model_load_mesh, _text(name), _text(path), C.byref(o))

    def load_fdf(self, name, path, symmetry="none") -> Object:
        """The panels of a free-surface file (FDF) as a panels mesh, with the model's symmetry."""
        return self._object(_lib.hb_model_load_fdf, _text(name), _text(path), _text(symmetry))

    def load_csf(self, name, path, zero_trailer=False, body=None) -> Object:
        if body is not None and (not isinstance(body, Object) or body.model is not self):
            raise ValueError("body: a Body of this model, its name or its index")
        b = body._id if body is not None else NO_OBJECT
        return self._object(_lib.hb_model_load_csf, _text(name), _text(path), int(bool(zero_trailer)), b)

    def hbg_mesh(self, name, text) -> Object:
        return self._object(_lib.hb_model_mesh_from_hbg, _text(name), _text(text))

    def analytic_mesh(
        self, name, shape, dimensions, discs, parts, mapping, discretization, reference_length, symmetry, label
    ) -> Object:
        dim_names, keep_names = _texts([d[0] for d in dimensions])
        dim_values = np.ascontiguousarray([float(d[1]) for d in dimensions], dtype=np.float64)
        disc = np.ascontiguousarray(np.asarray(discs, dtype=np.float64).reshape(-1, 3))
        part_names, keep_parts = _texts(list(parts))
        rows = _rows6(discretization)
        s = _AnalyticShape(
            shape=_text(shape),
            dimension_names=dim_names,
            dimension_values=_ptr(dim_values, _F64),
            dimensions=len(dim_values),
            discs=_ptr(disc, _F64),
            disc_count=len(disc),
            parts=part_names,
            part_count=len(keep_parts),
            mapping=_text(mapping),
            discretization=_ptr(rows, _I32),
            discretization_rows=len(rows),
            reference_length=float(reference_length),
            symmetry=_text(symmetry),
            label=_text(label) if label else None,
        )
        made = self._object(_lib.hb_model_analytic_mesh, _text(name), C.byref(s))
        del keep_names
        return made


# ------------------------------------------------------------------ Object

_CLOSED = "the model of this object is closed"


def _name_of(name) -> bytes:
    if name is None:
        return b""
    if not isinstance(name, str):
        raise TypeError(f"a name must be a string, got {type(name).__name__}")
    return name.encode("utf-8")


def _is_int(v) -> bool:
    return not isinstance(v, (bool, np.bool_)) and isinstance(v, (int, np.integer))


class Object:
    """An object of a model."""

    def __init__(self, model: Model, oid: int):
        self._model = model
        self._id = int(oid)

    def _h(self):
        return self._model._h(_CLOSED)

    @property
    def model(self) -> Model:
        return self._model

    def _info(self) -> dict:
        return _table(_lib.hb_object_info, self._h(), self._id)

    @property
    def type(self) -> str:
        return self._info()["type"]

    @property
    def name(self) -> str:
        return self._info()["name"]

    @property
    def path(self) -> str:
        return self._info()["path"]

    def children(self, list) -> list:  # noqa: A002 (the engine's word)
        ids = _table(_lib.hb_children, self._h(), self._id, _text(list))["ids"]
        return [Object(self._model, int(i)) for i in ids]

    def add(self, list, name=None, type=None, index=-1) -> Object:  # noqa: A002
        oid = _U64()
        # a negative place is the end (as the extension module; the C ABI takes -1 alone)
        at = -1 if int(index) < 0 else min(int(index), 2**63 - 1)
        _check(_lib.hb_add(self._h(), self._id, _text(list), _text(type), _name_of(name), at, C.byref(oid)))
        return Object(self._model, oid.value)

    def remove(self):
        _check(_lib.hb_remove(self._h(), self._id))

    def clear(self, list):  # noqa: A002
        """Removes every object of a list, all or nothing (refused while another object refers to
        one of them)."""
        _check(_lib.hb_clear(self._h(), self._id, _text(list)))

    def rename(self, name):
        _check(_lib.hb_rename(self._h(), self._id, _name_of(name)))

    def move(self, index):
        index = max(min(int(index), 2**63 - 1), -(2**63))
        _check(_lib.hb_move(self._h(), self._id, index))

    def describe(self, item) -> dict:
        t = _table(_lib.hb_item_info, self._h(), self._id, _text(item))
        return {
            "kind": t["kind"],
            "is_set": bool(t["is_set"]),
            "has_value": bool(t["has_value"]),
            "is_array": bool(t["is_array"]),
            "dtype": t["dtype"],
            "shape": tuple(int(d) for d in np.atleast_1d(t["shape"])) if np.size(t["shape"]) else (),
            "is_choice": bool(t["is_choice"]),
        }

    def get(self, item):
        with _Table() as tab:
            _check(_lib.hb_get(self._h(), self._id, _text(item), C.byref(tab.p)))
            t = tab.dict()
        kind = t["kind"]
        if kind == "none":
            return None
        v = t["value"]
        if kind == "bool":
            return bool(v)
        if kind == "int":
            return int(v)
        if kind == "real":
            return float(v)
        if kind == "text":
            return v
        if kind == "texts":
            return tuple(v if isinstance(v, list) else [v])
        if kind in ("reals", "ints"):
            return v
        raise RuntimeError(f"a value of kind {kind}")

    def set(self, item, value):
        h = self._h()
        if value is None:
            _check(_lib.hb_reset(h, self._id, _text(item)))
            return
        info = self._info()
        # an item the object does not have: the engine's message
        _table(_lib.hb_item_info, h, self._id, _text(item))
        d = schema()[info["type"]]["items"][item]
        at = info["path"] + ": " if info["path"] else ""
        self._set(h, d, at, item, value)

    def _set(self, h, d, at, item, value):
        kind = d["kind"]
        key = _text(item)

        def fail(what):
            return TypeError(f"{at}{item} must be {what}")

        names = d["choices"] if kind in ("int", "real") else ()
        if names and isinstance(value, str):
            _check(_lib.hb_set_text(h, self._id, key, _text(value)))
            return
        alt = f" or one of {', '.join(names)}" if names else ""
        is_bool = isinstance(value, (bool, np.bool_))
        is_int = _is_int(value)
        is_real = is_int or isinstance(value, (float, np.floating))
        if kind == "bool":
            if not is_bool:
                raise fail("a bool")
            _check(_lib.hb_set_bool(h, self._id, key, int(bool(value))))
        elif kind == "int":
            if not is_int:
                raise fail(f"an integer{alt}")
            if not -(2**63) <= int(value) < 2**63:
                raise OverflowError(f"{at}{item}: {value} does not fit in 64 bits")
            _check(_lib.hb_set_int(h, self._id, key, int(value)))
        elif kind == "real":
            if not is_real:
                raise fail(f"a real number{alt}")
            _check(_lib.hb_set_real(h, self._id, key, float(value)))
        elif kind in ("text", "choice", "reference") or (kind == "origin" and isinstance(value, str)):
            if not isinstance(value, str):
                raise fail("a string")
            _check(_lib.hb_set_text(h, self._id, key, _text(value)))
        else:
            self._set_array(h, d, at, item, value)

    def _set_array(self, h, d, at, item, value):
        key = _text(item)

        def fail(what):
            return TypeError(f"{at}{item} must be {what}")

        dtype = d["dtype"] if d["kind"] == "array" else "real"
        if dtype == "text":
            if isinstance(value, str):
                raise fail("a sequence of strings")
            try:
                texts = list(value)
            except TypeError:
                raise fail("a sequence of strings") from None
            if not all(isinstance(t, str) for t in texts):
                raise fail("a sequence of strings")
            arr, keep = _texts(texts)
            _check(_lib.hb_set_texts(h, self._id, key, arr, len(keep)))
            return
        a = np.asarray(value)
        if a.ndim == 0:
            raise fail("an array")
        if np.iscomplexobj(a):
            raise fail("real")
        shape = (_SZ * a.ndim)(*a.shape)
        if a.dtype.kind in "iu" and dtype == "int":
            c = np.ascontiguousarray(a, dtype=np.int64)
            _check(_lib.hb_set_ints(h, self._id, key, _ptr(c, _I64), shape, a.ndim))
            return
        if a.dtype.kind in "iuf":
            if dtype == "int":
                raise fail("an array of integers")
            c = np.ascontiguousarray(a, dtype=np.float64)
            _check(_lib.hb_set_reals(h, self._id, key, _ptr(c, _F64), shape, a.ndim))
            return
        raise fail("an array of numbers")

    def reset(self, item):
        _check(_lib.hb_reset(self._h(), self._id, _text(item)))

    def trimmed_mesh(self, name, role) -> Object:
        oid = _U64()
        _check(_lib.hb_body_trimmed_mesh(self._h(), self._id, _text(name), _text(role), C.byref(oid)))
        return Object(self._model, oid.value)

    def body_hydrostatics(self) -> dict:
        return _hydrostatics(_table(_lib.hb_body_hydrostatics, self._h(), self._id))

    def body_panels(self, include_reflections, roles) -> dict:
        t = _table(_lib.hb_body_panels, self._h(), self._id, _role_text(roles), int(include_reflections))
        return _named(t, _PANELS)

    def body_mesh_quality(self) -> dict:
        return _named(_table(_lib.hb_body_mesh_quality, self._h(), self._id), _QUALITY)

    def body_tessellation(self, roles, divisions, include_reflections) -> dict:
        du, dv = divisions
        t = _table(
            _lib.hb_body_tessellation, self._h(), self._id, _role_text(roles), int(du), int(dv), int(include_reflections)
        )
        return _named(t, _TESSELLATION)

    def hbg(self) -> str:
        out = C.c_void_p()
        _check(_lib.hb_mesh_hbg(self._h(), self._id, C.byref(out)))
        return _owned_text(out)

    def write(self, gdf, spl=None, overwrite=False):
        _check(_lib.hb_mesh_write(self._h(), self._id, _text(gdf), _text(spl), int(bool(overwrite))))

    def mesh_geometry(self) -> dict:
        t = _table(_lib.hb_mesh_geometry, self._h(), self._id)
        return {
            "volumes": list(t["volumes"]),
            "volume": t["volume"],
            "center_of_volume": list(t["center_of_volume"]),
            "area": t["area"],
            "bounds": t["bounds"],
            "waterplane_area": t["waterplane_area"],
            "waterplane_centroid": list(t["waterplane_centroid"]),
            "waterplane_moments": list(t["waterplane_moments"]),
        }

    def mesh_elements(self) -> dict:
        return _named(_table(_lib.hb_mesh_elements, self._h(), self._id), _MESH_ELEMENTS)

    def mesh_tessellation(self, divisions, include_reflections) -> dict:
        du, dv = divisions
        t = _table(_lib.hb_mesh_tessellation, self._h(), self._id, int(du), int(dv), int(include_reflections))
        return _named(t, _TESSELLATION)

    def mesh_geometry_quality(self) -> dict:
        return _named(_table(_lib.hb_mesh_geometry_quality, self._h(), self._id), _QUALITY)

    def patch_points(self, element, uv):
        uv = _floats(uv, 2, "uv")
        t = _table(_lib.hb_mesh_patch_points, self._h(), self._id, int(element), _ptr(uv, _F64), len(uv))
        return t["points"], t["normals"]

    def __eq__(self, other):
        return isinstance(other, Object) and other._id == self._id and other._model is self._model

    def __hash__(self):
        return hash((id(self._model), self._id))


# ------------------------------------------------------------------ Results


def _unsigned(v) -> int:
    """A zero-based index (as the extension module's usize: a negative one is refused)."""
    v = int(v.__index__())
    if v < 0:
        raise OverflowError("can't convert negative int to unsigned")
    return min(v, 2**63 - 1)


def _rows(v: list):
    """The rows of zero-based indices (ascending): a slice when they follow each other (and some),
    else an intp array."""
    if v and all(b == a + 1 for a, b in zip(v, v[1:], strict=False)):
        return slice(v[0], v[-1] + 1, 1)
    return np.asarray(v, dtype=np.intp)


class Results:
    """The results of a solve, of a results file, or made from arrays."""

    def __init__(self, p: C.c_void_p):
        self._p = p

    def __del__(self, _free=_lib.hb_results_free):
        p = getattr(self, "_p", None)
        if p is not None and p.value:
            try:
                _free(p)
            except Exception:  # noqa: BLE001 (nothing to report to at shutdown)
                pass

    def quantity(self, code: int):
        name = _text(_quantity_name(code))
        has = _I32()
        _check(_lib.hb_results_has(self._p, name, C.byref(has)))
        if not has.value:
            return None
        col = _Column()
        _check(_lib.hb_results_quantity(self._p, name, C.byref(col)))
        return _column_value(col, owner=self)

    def _has(self, name: str) -> bool:
        has = _I32()
        _check(_lib.hb_results_has(self._p, _text(name), C.byref(has)))
        return bool(has.value)

    def _ints(self, name: str):
        """A quantity of integers by name (a copy), None when the results do not have it."""
        if not self._has(name):
            return None
        col = _Column()
        _check(_lib.hb_results_quantity(self._p, _text(name), C.byref(col)))
        return np.asarray(_column_value(col))

    def body_columns(self, body: int):
        """The zero-based indices of a body's coordinates (D axis) and radiation columns (K axis)."""
        body = _unsigned(body)
        t = _table(_lib.hb_results_body_columns, self._p, body)
        return (np.atleast_1d(np.asarray(t["coordinates"], dtype=np.intp)),
                np.atleast_1d(np.asarray(t["radiation_columns"], dtype=np.intp)))  # fmt: skip

    def selection(self, path, body=None, owner=None):
        """What the view of a path selects for an owner (``hb_results_selection``): None, or the
        code of the quantity and per axis selected (axis, rows): a slice for contiguous rows, else
        an intp array of the zero-based rows; the axes not listed are taken whole. A refusal is
        the engine's."""
        body = -1 if body is None else _unsigned(body)
        owner = -1 if owner is None else _unsigned(owner)
        t = _table(_lib.hb_results_selection, self._p, _text(path), body, owner)
        if "quantity" not in t:
            return None
        code = [q["name"] for q in _quantity_rows()].index(t["quantity"])
        axis = np.atleast_1d(np.asarray(t.get("axis", []), dtype=np.intp))
        index = np.atleast_1d(np.asarray(t.get("index", []), dtype=np.intp))
        return code, [(int(a), _rows(index[axis == a].tolist())) for a in sorted(set(axis.tolist()))]

    def available(self) -> list:
        names = _table(_lib.hb_results_available, self._p)["names"]
        all_names = [q["name"] for q in _quantity_rows()]
        names = [names] if isinstance(names, str) else names
        return sorted(all_names.index(n) for n in names if n in all_names)

    @property
    def route(self) -> str:
        out = C.c_void_p()
        _check(_lib.hb_results_route(self._p, C.byref(out)))
        return _owned_text(out)

    def diagnostics(self) -> list:
        return _diagnostics(_table(_lib.hb_results_diagnostics, self._p))

    def meta(self):
        t = _table(_lib.hb_results_meta, self._p)
        if not t["present"]:
            return None
        return {
            "hydrobem_version": t["hydrobem_version"],
            "api_version": int(t["api_version"]),
            "commit": t["commit"],
            "compiler": t["compiler"],
            "linear_algebra": t["linear_algebra"],
            "build_type": t["build_type"],
            "time": t["time"],
            "route": t["route"],
            "frequency_threads": int(t["frequency_threads"]),
            "inner_threads": int(t["inner_threads"]),
            "engine_seconds": tuple(float(s) for s in np.atleast_1d(t["engine_seconds"])),
            "environment_hooks": tuple([h] if isinstance(h := t.get("environment_hooks", []), str) else h),
        }

    def model(self) -> Model:
        return Model._made(_lib.hb_results_model, self._p)

    def solved_model(self) -> Model:
        return Model._made(_lib.hb_results_solved_model, self._p)

    def normalized(self, code: int):
        v = _table(_lib.hb_results_normalized, self._p, _text(_quantity_name(code)))["values"]
        return v if isinstance(v, np.ndarray) else _frozen(np.asarray(v))

    def save(self, path, overwrite=False):
        _check(_lib.hb_results_save(self._p, _text(path), int(bool(overwrite))))

    def write_wamit(self, stem, overwrite=False, omega=False, stamp=None):
        _check(_lib.hb_results_write_wamit(self._p, _text(stem), int(bool(overwrite)), int(bool(omega)), _text(stamp)))
