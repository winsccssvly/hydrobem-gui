"""The graphical interface: ``HydroBEM.exe`` beside the engine DLL in the package's ``bin`` folder.

    import hydrobem_gui as hb

    hb.show()                     # the GUI, empty
    hb.show("barge.hbm")          # a model file or a results file (.hbr)
    m = hb.Model.load("barge.hbm")
    m.environment.omegas = [0.5, 1.0]
    hb.show(m)                    # this model, as a new unsaved project

The ``hydrobem-gui`` command (``main``) opens it from a terminal. The GUI runs in a process of its
own.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def exe_path() -> Path:
    """The GUI's program: ``bin/HydroBEM.exe`` of the package, else the release build of the source
    tree (development)."""
    here = Path(__file__).resolve().parent
    for p in (here / "bin" / "HydroBEM.exe", here.parents[1] / "target" / "release" / "HydroBEM.exe"):
        if p.is_file():
            return p
    raise FileNotFoundError("HydroBEM.exe was not found in the package (bin) nor in target/release")


def _runtime_dirs() -> list[str]:
    """The folders of the Visual C++ runtime this Python has loaded (``vcruntime140.dll`` and
    ``vcruntime140_1.dll``): HydroBEM.exe and the engine DLL are linked with the C runtime as a DLL
    and take the one Python installs, as the engine does inside Python; the wheel carries none."""
    import ctypes
    from ctypes import wintypes

    k = ctypes.WinDLL("kernel32", use_last_error=True)
    k.GetModuleHandleW.argtypes = [wintypes.LPCWSTR]
    k.GetModuleHandleW.restype = wintypes.HMODULE
    k.GetModuleFileNameW.argtypes = [wintypes.HMODULE, wintypes.LPWSTR, wintypes.DWORD]
    k.GetModuleFileNameW.restype = wintypes.DWORD
    out = []
    for name in ("vcruntime140.dll", "vcruntime140_1.dll"):
        h = k.GetModuleHandleW(name)
        if not h:
            continue
        buf = ctypes.create_unicode_buffer(32768)
        n = k.GetModuleFileNameW(h, buf, len(buf))
        folder = os.path.dirname(buf.value[:n]) if n else ""
        if folder and folder not in out:
            out.append(folder)
    return out


def _environment() -> dict[str, str]:
    """The environment of the GUI's process: this Python's prefix, and its C runtime first on the
    PATH (found there by Windows before HydroBEM.exe starts)."""
    env = dict(os.environ, HYDROBEM_PYTHON_PREFIX=sys.prefix)
    env["PATH"] = os.pathsep.join([*_runtime_dirs(), env.get("PATH", "")])
    return env


def _start(args: list[str], wait: bool) -> subprocess.Popen:
    env = _environment()
    p = subprocess.Popen([str(exe_path()), *args], env=env, close_fds=True)
    if wait:
        p.wait()
    return p


def show(target=None, wait: bool = False) -> subprocess.Popen:
    """Opens the GUI: empty, with a model file or results file (a path), or with a ``Model`` (saved
    to a temporary model file and opened as a new unsaved project: the GUI does not change the
    model). With ``wait`` it returns once the GUI is closed."""
    from .engine import Model

    if target is None:
        return _start([], wait)
    if isinstance(target, Model):
        folder = Path(tempfile.mkdtemp(prefix="hydrobem-show-"))
        path = folder / "model.hbm"
        target.save(path)
        p = _start(["--untitled", str(path)], wait)
        if wait:
            # the GUI has read it long ago; without wait the file stays in the temporary folder
            shutil.rmtree(folder, ignore_errors=True)
        return p
    path = Path(target)
    if not path.is_file():
        raise FileNotFoundError(path)
    return _start([str(path.resolve())], wait)


def main():
    """The ``hydrobem-gui`` command: ``hydrobem-gui [file.hbm | file.hbr]``."""
    _start(sys.argv[1:], wait=False)
