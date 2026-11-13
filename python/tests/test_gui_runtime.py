"""The GUI's process takes the Visual C++ runtime of this Python (HydroBEM GUI's own test, not
copied from HydroBEM): HydroBEM.exe and hydrobem.dll are linked with the C runtime as a DLL, and the
wheel carries none."""

import os
import sys
from pathlib import Path

from hydrobem_gui import gui


def test_the_runtime_folders_are_those_this_python_has_loaded():
    dirs = gui._runtime_dirs()
    assert dirs, "Python for Windows loads vcruntime140.dll"
    assert all((Path(d) / "vcruntime140.dll").is_file() or (Path(d) / "vcruntime140_1.dll").is_file() for d in dirs)


def test_the_gui_finds_them_first_on_its_path():
    env = gui._environment()
    first = env["PATH"].split(os.pathsep)[: len(gui._runtime_dirs())]
    assert first == gui._runtime_dirs()
    assert env["HYDROBEM_PYTHON_PREFIX"] == sys.prefix
