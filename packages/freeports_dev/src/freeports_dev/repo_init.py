"""Initialize a new freeports format repository skeleton."""

import sys
from pathlib import Path
import shutil


CSV_HEADERS = {
    "metadata/formats.csv": "Name,Locale,Year,Country,Version\n",
    "metadata/url_mapping.csv": "Name,URL\n",
    "content/algorithms/structured/investments/args.csv": (
        "Format,Page type,Font,Font size,"
        "Subfund set col,Currency set col,Body set col,Market value col,"
        "Quantity col,% net assets col,Acquisition cost col,Acquisition cost currency col\n"
    ),
    "content/algorithms/structured/investments/additional_args.csv": (
        "Format,Page type,USE_RULER_AREA,Precision of tolerances,"
        "Interpret quantity as float,Geometrical indexing,Merge previous\n"
    ),
    "content/algorithms/structured/investments/deselection_lists.csv": (
        "Format,Page type,Reference string\n"
    ),
    "content/algorithms/structured/investments/partial_pipes.csv": (
        "Format,Page type,Skip pdf extract,Skip text filter,Skip deserialize\n"
    ),
    "content/algorithms/structured/page_classify/args.csv": (
        "Format,Page type,Font,Font size,Reference string\n"
    ),
    "content/algorithms/semistructured/formats_mapping.csv": "Format,Page type,Algorithm\n",
    "content/orchestration/algorithms_schedule.csv": (
        "Format,Page type,Filter next iteration\n"
    ),
    "content/orchestration/mapping.csv": "ID,Page type\n",
    "content/orchestration/pageclassify_overwrite.csv": "ID\n",
}

DIRS = [
    "metadata",
    "tests/formats",
    "validation",
    "devtools",
    "content/algorithms/structured/investments",
    "content/algorithms/structured/page_classify",
    "content/algorithms/semistructured/args",
    "content/algorithms/unstructured",
    "content/orchestration",
    "content/templates",
]

YAML_FILES = {
    "content/algorithms/semistructured/args/deserialize.yaml": "{}\n",
    "content/algorithms/semistructured/args/pdf_extract.yaml": "{}\n",
    "content/algorithms/semistructured/args/text_filter.yaml": "{}\n",
}

CONFTEST_CONTENT = """from pathlib import Path

ROOT = Path(__file__).parent.parent
"""

PYPROJECT_CONTENT = """[project]
name = "freeports-formats"
description = "Format definitions for the freeports PDF extraction framework"
requires-python = ">=3.8"
license = "GPL-3.0"
version = "0.0.1"
dependencies = [
    "freeports",
]

[build-system]
requires = ["setuptools >= 77.0.3"]
build-backend = "setuptools.build_meta"
"""

PACKAGE_YAML_CONTENT = (
    "schema: v0.0.0\n"
    "info:\n"
    '  description: ""\n'
    "  version: v0.0.0\n"
    "  validation_sha256: \n"
)

DEVTOOLS_GITIGNORE = "*\n!.gitignore\n!*.template.ipynb\n!*.py\n"


def init_format_repo(target: Path):
    if target.exists() and list(target.iterdir()):
        print(f"Error: {target} is not empty")
        sys.exit(1)

    for d in DIRS:
        (target / d).mkdir(parents=True, exist_ok=True)

    for rel_path, header in CSV_HEADERS.items():
        (target / rel_path).write_text(header)

    for rel_path, content in YAML_FILES.items():
        (target / rel_path).write_text(content)

    (target / "package.yaml").write_text(PACKAGE_YAML_CONTENT)
    (target / "tests" / "conftest.py").write_text(CONFTEST_CONTENT)
    (target / "pyproject.toml").write_text(PYPROJECT_CONTENT)

    devtools_dir = target / "devtools"
    devtools_dir.mkdir(parents=True, exist_ok=True)
    (devtools_dir / ".gitignore").write_text(DEVTOOLS_GITIGNORE)

    from freeports_dev.input_db import copy_default_input_db

    copy_default_input_db(target / "tests")

    print(f"Format repository created at {target}")
