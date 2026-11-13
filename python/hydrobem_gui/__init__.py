"""HydroBEM: frequency-domain panel-method (BEM) hydrodynamics, with its graphical interface.

The engine is ``hydrobem.dll`` (the engine's C ABI), bound with ctypes (``hydrobem_gui._core``);
``hb.Model`` is its model and ``hb.show`` opens the GUI, which runs on the same DLL. A typical
solve::

    import hydrobem_gui as hb

    m = hb.Model()
    m.meshes.load("cylinder", "cylinder.gdf")
    m.bodies.add("cylinder", mesh="cylinder", mass="displaced", center_of_gravity=[0, 0, 0],
                 radii_of_gyration=[1, 1, 1])
    m.environment.omegas = [0.5, 1.0, 2.0]
    m.environment.headings = [0.0]
    m.first_order.motion = True
    results = m.solve()
    results.write_wamit("out/cylinder")

A model file holds a model: ``m.save("case.hbm")``, ``hb.Model.load("case.hbm")``;
``m.save_results("run.hbr")`` adds the results; ``hb.show(m)`` or ``hb.show("case.hbm")`` opens
it in the GUI.
"""

__version__ = "0.3.0"

from . import _cpu, engine  # noqa: F401 (_cpu: the processor checked before the DLL loads)
from ._core import dll_path
from .engine import (
    FrequencyProgress,
    LimitProgress,
    Model,
    Progress,
    SecondOrderProgress,
    SolveProgress,
    WorkerProgress,
)
from .errors import Diagnostic, HydroBEMWarning, Location, SolveCancelledError, UnsupportedProblemError
from .gui import exe_path, show
from .npz import save_npz
from .queries import (
    BodyCounts,
    BuildInfo,
    Counts,
    FreeSurfacePoints,
    Hydrostatics,
    ItemInfo,
    MemoryEstimate,
    MeshEdges,
    MeshElements,
    MeshGeometry,
    MeshQuality,
    Meta,
    MotionEquation,
    Panels,
    PatchPoints,
    PointClasses,
    QuantityInfo,
    Tessellation,
    build_info,
    quantities,
    wavenumber,
)
from .results import (
    BodyPointResults,
    BodyResults,
    Coordinate,
    ExcitationResults,
    FieldPointResults,
    LidResults,
    PressureSurfaceResults,
    Results,
    TankResults,
    load_results,
)
from .selection import ranges

__all__ = [
    "BodyCounts",
    "BodyPointResults",
    "BodyResults",
    "BuildInfo",
    "Coordinate",
    "Counts",
    "Diagnostic",
    "ExcitationResults",
    "FieldPointResults",
    "FreeSurfacePoints",
    "FrequencyProgress",
    "HydroBEMWarning",
    "Hydrostatics",
    "ItemInfo",
    "LidResults",
    "LimitProgress",
    "Location",
    "MemoryEstimate",
    "Meta",
    "MeshEdges",
    "MeshElements",
    "MeshGeometry",
    "MeshQuality",
    "Model",
    "MotionEquation",
    "Panels",
    "PatchPoints",
    "PointClasses",
    "PressureSurfaceResults",
    "Progress",
    "QuantityInfo",
    "Results",
    "SecondOrderProgress",
    "SolveCancelledError",
    "SolveProgress",
    "TankResults",
    "Tessellation",
    "UnsupportedProblemError",
    "WorkerProgress",
    "build_info",
    "dll_path",
    "engine",
    "exe_path",
    "load_results",
    "quantities",
    "ranges",
    "save_npz",
    "show",
    "wavenumber",
]
