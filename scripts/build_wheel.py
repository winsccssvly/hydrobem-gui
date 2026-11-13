"""Builds the wheel of hydrobem-gui: the Python package (python/hydrobem_gui), the GUI and the engine
DLL in its bin folder, the examples (HydroBEM's scripts), and the licence notices.

    cargo build --release
    cargo xtask dist                  # dist/: HydroBEM.exe, hydrobem.dll, examples, licences
    python scripts/build_wheel.py     # dist/hydrobem_gui-<version>-py3-none-win_amd64.whl

The wheel holds no compiled Python: one wheel serves every Python >= 3.11 on Windows x64. Only the
standard library is used.
"""

from __future__ import annotations

import base64
import hashlib
import json
import sys
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
PACKAGE = ROOT / "python" / "hydrobem_gui"
TAG = "py3-none-win_amd64"


def project() -> dict:
    """The package's [project] table, after checking that the three versions agree."""
    p = tomllib.loads((ROOT / "python" / "pyproject.toml").read_text(encoding="utf-8"))["project"]
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    init = (PACKAGE / "__init__.py").read_text(encoding="utf-8")
    versions = {"python/pyproject.toml": p["version"], "Cargo.toml": cargo}
    versions["hydrobem_gui.__version__"] = next(
        (line.split('"')[1] for line in init.splitlines() if line.startswith("__version__")), "?"
    )
    if len(set(versions.values())) != 1:
        sys.exit(f"the versions disagree: {versions}")
    return p


def files(p: dict, dist_info: str) -> list[tuple[str, bytes]]:
    """The files of the wheel: (path in the wheel, content), RECORD apart."""
    out: list[tuple[str, bytes]] = []
    # the sources and the typing marker (editors read the docstrings and hints from them)
    for f in sorted([*PACKAGE.rglob("*.py"), PACKAGE / "py.typed"]):
        out.append((f"hydrobem_gui/{f.relative_to(PACKAGE).as_posix()}", f.read_bytes()))
    for name in ("HydroBEM.exe", "hydrobem.dll"):
        f = DIST / name
        if not f.is_file():
            sys.exit(f"{f} is missing: cargo build --release, then cargo xtask dist")
        out.append((f"hydrobem_gui/bin/{name}", f.read_bytes()))
    # the DLL is the one imported (engine/IMPORT.json)
    record = json.loads((ROOT / "engine" / "IMPORT.json").read_text(encoding="utf-8"))
    if hashlib.sha256((DIST / "hydrobem.dll").read_bytes()).hexdigest() != record["sha256"]["hydrobem.dll"]:
        sys.exit("dist/hydrobem.dll is not engine/hydrobem.dll of engine/IMPORT.json: run cargo xtask dist")
    for f in sorted(f for f in (DIST / "examples").rglob("*") if f.is_file()):
        out.append((f"hydrobem_gui/examples/{f.relative_to(DIST / 'examples').as_posix()}", f.read_bytes()))
    licences = [DIST / "LICENSE", DIST / "NOTICE", DIST / "THIRD_PARTY_NOTICES.md"]
    licences += sorted(f for f in (DIST / "licenses").rglob("*") if f.is_file())
    licence_names = []
    for f in licences:
        name = f.relative_to(DIST).as_posix()
        licence_names.append(name)
        out.append((f"{dist_info}/licenses/{name}", f.read_bytes()))
    deps = "".join(f"Requires-Dist: {d}\n" for d in p.get("dependencies", []))
    lics = "".join(f"License-File: {n}\n" for n in licence_names)
    metadata = (
        "Metadata-Version: 2.4\n"
        f"Name: {p['name']}\n"
        f"Version: {p['version']}\n"
        f"Summary: {p['description']}\n"
        f"Requires-Python: {p['requires-python']}\n"
        f"License-Expression: {p['license']}\n"
        f"{deps}{lics}"
    )
    out.append((f"{dist_info}/METADATA", metadata.encode("utf-8")))
    wheel = f"Wheel-Version: 1.0\nGenerator: hydrobem-gui scripts/build_wheel.py\nRoot-Is-Purelib: false\nTag: {TAG}\n"
    out.append((f"{dist_info}/WHEEL", wheel.encode("utf-8")))
    scripts = "".join(f"{k} = {v}\n" for k, v in p.get("gui-scripts", {}).items())
    out.append((f"{dist_info}/entry_points.txt", f"[gui_scripts]\n{scripts}".encode()))
    return out


def digest(data: bytes) -> str:
    return "sha256=" + base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode()


def main() -> Path:
    p = project()
    name = p["name"].replace("-", "_")
    dist_info = f"{name}-{p['version']}.dist-info"
    content = files(p, dist_info)
    record = "".join(f"{path},{digest(data)},{len(data)}\n" for path, data in content)
    record += f"{dist_info}/RECORD,,\n"
    wheel = DIST / f"{name}-{p['version']}-{TAG}.whl"
    with zipfile.ZipFile(wheel, "w", zipfile.ZIP_DEFLATED) as z:
        for path, data in content:
            z.writestr(zipfile.ZipInfo(path, date_time=(2026, 1, 1, 0, 0, 0)), data, zipfile.ZIP_DEFLATED)
        z.writestr(zipfile.ZipInfo(f"{dist_info}/RECORD", date_time=(2026, 1, 1, 0, 0, 0)), record)
    print(f"wrote {wheel} ({wheel.stat().st_size / 1e6:.1f} MB)")
    return wheel


if __name__ == "__main__":
    main()
