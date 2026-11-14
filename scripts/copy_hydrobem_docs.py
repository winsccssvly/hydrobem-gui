"""Copies HydroBEM's user documentation of the Python API into docs/python/.

The guide (docs/guide/*.md), the reference of the inputs, queries, results and output files
(docs/reference/), and the list of literature (docs/references.md), as they are at a commit of a
HydroBEM checkout, with the changes this package needs:

- the package is `hydrobem_gui` (`import hydrobem_gui as hb`), as in the copied Python code;
- getting-started's installation section is this package's (the wheel `hydrobem-gui`);
- a link to a document that is not copied (the theory, the validation report, the development
  documents, the Rust and C references) points into HydroBEM's documentation at `--url` when given,
  else it becomes its text alone ("theory §12.3"; docs/python/README.md says where the theory is).

Copy again with the Python package at each engine import (docs/DEVELOPMENT.md); never edit the
copies by hand.

    python scripts/copy_hydrobem_docs.py ..\\HydroBEM 2def9bb [--url https://.../docs/]
"""

import argparse
import posixpath
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "python"

GUIDE = [
    "getting-started",
    "working-with-the-model",
    "meshes-and-bodies",
    "environment-and-options",
    "results",
    "special-surfaces",
    "tanks-and-generalized-modes",
    "second-order-loads",
    "performance",
]
REFERENCE = [
    "inputs-model",
    "inputs-meshes",
    "inputs-bodies",
    "inputs-model-objects",
    "inputs-orders",
    "inputs-rules",
    "queries",
    "results",
    "file-formats",
]
FILES = [f"guide/{n}.md" for n in GUIDE] + [f"reference/{n}.md" for n in REFERENCE] + ["references.md"]

INSTALL = """## 1. Installation

The Python API is the package `hydrobem_gui` of the wheel `hydrobem-gui`, which also holds the
graphical interface (`HydroBEM.exe`) and HydroBEM's engine (`hydrobem.dll`). It needs Windows on
x86-64, a processor with the AVX2 and FMA instructions (most x86-64 processors made since 2013 to
2015), Python 3.11 or later and NumPy. Install the wheel into a Python environment (conda's from
conda-forge, or a virtual environment):

```bat
pip install hydrobem_gui-0.3.0-py3-none-win_amd64.whl
```

To check the installation, start Python and ask for the build information of the engine:

```python
import hydrobem_gui as hb

print(hb.build_info())
```

`hb.show(m)` opens a model in the graphical interface, and the command `hydrobem-gui` starts it
(the GUI's manual: `docs/manual/`).

### 1.1 Troubleshooting

| Message | Cause and remedy |
|---|---|
| `ImportError` or a closed Python process when NumPy loads | NumPy's own libraries were not found: activate the conda environment (`conda activate <name>`) before starting Python, rather than running its `python.exe` directly. |
| `ImportError: hydrobem.dll was not found` | The package was copied without its `bin` folder: install the wheel again. |
| `ImportError: ... has the C ABI n of the engine; hydrobem_gui needs ABI m` | A `hydrobem.dll` of another version was found first (the environment variable `HYDROBEM_DLL`): unset it, or install the wheel again. |

"""


def show(checkout, commit, path):
    """A file of the checkout at the commit (the committed text, not the working tree)."""
    r = subprocess.run(["git", "-C", str(checkout), "show", f"{commit}:docs/{path}"], capture_output=True, check=True)
    return r.stdout.decode("utf-8")


def rename(text):
    """The package of this wheel in place of HydroBEM's."""
    text = text.replace("import hydrobem as hb", "import hydrobem_gui as hb")
    text = re.sub(r"\bfrom hydrobem\b(?!_gui)", "from hydrobem_gui", text)
    text = re.sub(r"\bimport hydrobem\b(?!_gui)", "import hydrobem_gui", text)
    text = re.sub(r"`hydrobem`", "`hydrobem_gui`", text)
    text = re.sub(r"\bhydrobem\.(?=[a-z_]+\b)(?!dll|exe|h\b)", "hydrobem_gui.", text)
    return text


def links(text, path, url):
    """Links into the copied documents stay relative; others point into HydroBEM's documentation
    (`url`) or become a citation."""
    here = posixpath.dirname(path)

    def one(m):
        label, target = m.group(1), m.group(2)
        if re.match(r"[a-z]+:", target) or target.startswith("#"):
            return m.group(0)
        file, _, anchor = target.partition("#")
        resolved = posixpath.normpath(posixpath.join(here, file))
        if resolved in FILES:
            return m.group(0)
        if resolved.startswith(".."):  # outside docs/: HydroBEM's CHANGELOG, CONTRIBUTING, ...
            where = resolved.removeprefix("../")
        else:
            where = f"docs/{resolved}"
        if url:
            return f"[{label}]({url.rstrip('/')}/{where.removeprefix('docs/')}{'#' + anchor if anchor else ''})"
        return label

    return re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", one, text)


def note(path, commit):
    return (f"<!-- Copied from HydroBEM docs/{path} at {commit} by scripts/copy_hydrobem_docs.py; "
            "copy again rather than edit. -->\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("checkout", type=Path)
    ap.add_argument("commit")
    ap.add_argument("--url", default="", help="the address of HydroBEM's docs/ folder, for the documents not copied")
    a = ap.parse_args()
    for path in FILES:
        text = rename(show(a.checkout, a.commit, path))
        if path == "guide/getting-started.md":
            start, end = text.index("## 1. Installation"), text.index("## 2. ")
            text = text[:start] + INSTALL + text[end:]
        text = links(text, path, a.url)
        target = OUT / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(note(path, a.commit) + text, encoding="utf-8", newline="\n")
        print("wrote", target.relative_to(ROOT))


if __name__ == "__main__":
    main()
