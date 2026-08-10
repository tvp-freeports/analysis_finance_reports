"""Initialize a new freeports format repository skeleton.

Reads template files from the adjacent lib/ directory so content can be
modified without touching Python code.
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path


_TEMPLATES_DIR = Path(__file__).resolve().parent / "lib"


def _read_template(filename: str) -> str:
    """Read a template file from the lib/ directory."""
    path = _TEMPLATES_DIR / filename
    if not path.exists():
        print(f"Error: template file not found: {path}")
        sys.exit(1)
    return path.read_text(encoding="utf-8")


def _read_json(filename: str):
    """Read a JSON data file from the lib/ directory."""
    path = _TEMPLATES_DIR / filename
    if not path.exists():
        print(f"Error: data file not found: {path}")
        sys.exit(1)
    return json.loads(path.read_text(encoding="utf-8"))


def _validate_package_yaml(target: Path) -> None:
    """Validate the generated package.yaml against the JSON Schema."""
    import yaml

    schema_path = _TEMPLATES_DIR / "package.schema.json"
    if not schema_path.exists():
        return  # schema is optional for now

    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    pkg_path = target / "package.yaml"
    with open(pkg_path, encoding="utf-8") as f:
        doc = yaml.safe_load(f)

    errors = []
    for key in schema.get("required", []):
        if key not in doc:
            errors.append(f"missing required key: {key}")

    if "info" in schema.get("properties", {}):
        info_props = schema["properties"]["info"].get("properties", {})
        info = doc.get("info", {})
        for key, props in info_props.items():
            if key in schema["properties"]["info"].get("required", []):
                if key not in info:
                    errors.append(f"missing required key: info.{key}")
                elif props.get("type") == "string" and not isinstance(info[key], str):
                    errors.append(
                        f"info.{key} must be a string, got {type(info[key]).__name__}"
                    )

    if errors:
        print("Warning: package.yaml validation issues:")
        for e in errors:
            print(f"  - {e}")
    else:
        print("  package.yaml validates OK")


def _user_confirm(question: str, default: bool = True) -> bool:
    """Ask the user a yes/no question."""
    y_text = "Y" if default else "y"
    n_text = "n" if default else "N"
    c = input(f"{question} [{y_text}/{n_text}]: ").strip().lower()
    if c in ("y", "yes"):
        return True
    if c in ("n", "no"):
        return False
    if c == "":
        return default
    print("Please answer yes or no.")
    return _user_confirm(question, default)


def _setup_git(target: Path, quiet: bool = False) -> None:
    """Configure git hooks for the target directory."""
    is_git_repo = (target / ".git").exists()

    if not is_git_repo:
        if quiet or _user_confirm("Initialize as a git repository?", default=True):
            subprocess.run(
                ["git", "-C", str(target), "init"],
                check=True,
                capture_output=True,
            )
            print("  git repository initialized")
        else:
            print("  skipping git setup (not a git repository)")
            return

    subprocess.run(
        ["git", "-C", str(target), "config", "--local", "core.hooksPath", ".githooks"],
        check=True,
        capture_output=True,
    )
    print("  git hooks configured (core.hooksPath = .githooks)")


def _is_empty_dir(target: Path) -> bool:
    """Check if a directory is empty, ignoring .git."""
    entries = [e for e in target.iterdir() if e.name != ".git"]
    return len(entries) == 0


def init_format_repo(target: Path, quiet: bool = False) -> None:
    """Create a new format repository skeleton at `target`.

    Parameters
    ----------
    target : Path
        Path to the new repository directory. Must be empty or non-existent
        (a .git directory is tolerated).
    quiet : bool
        If True, skip interactive prompts (defaults to yes for git init).
    """
    if target.exists() and not _is_empty_dir(target):
        print(f"Error: {target} is not empty")
        sys.exit(1)

    target.mkdir(parents=True, exist_ok=True)

    dirs: list = _read_json("dirs.json")
    csv_headers: dict = _read_json("csv_headers.json")
    yaml_seeds: dict = _read_json("yaml_seeds.json")

    # Create directory structure
    for d in dirs:
        (target / d).mkdir(parents=True, exist_ok=True)
    print(f"  created {len(dirs)} directories")

    # Write CSV header files
    for rel_path, header in csv_headers.items():
        (target / rel_path).write_text(header, encoding="utf-8")
    print(f"  wrote {len(csv_headers)} CSV files")

    # Write YAML seed files
    for rel_path, content in yaml_seeds.items():
        (target / rel_path).write_text(content, encoding="utf-8")
    print(f"  wrote {len(yaml_seeds)} YAML seed files")

    # Write template-based files
    (target / "package.yaml").write_text(
        _read_template("package.template.yaml"), encoding="utf-8"
    )
    (target / "pyproject.toml").write_text(
        _read_template("pyproject.template.toml"), encoding="utf-8"
    )
    (target / "tests" / "conftest.py").write_text(
        _read_template("conftest.template.py"), encoding="utf-8"
    )
    print("  wrote package.yaml, pyproject.toml, tests/conftest.py")

    # Validate package.yaml
    _validate_package_yaml(target)

    # Create .githooks directory and pre-commit hook
    githooks_dir = target / ".githooks"
    githooks_dir.mkdir(exist_ok=True)
    pre_commit = githooks_dir / "pre-commit"
    pre_commit.write_text(_read_template("pre-commit.template"), encoding="utf-8")
    pre_commit.chmod(0o755)
    print("  created .githooks/pre-commit")

    # Copy default input DB
    from freeports_dev.input_db import copy_default_input_db

    copy_default_input_db(target / "tests")
    print("  copied default input DB to tests/input_db/")

    # Git setup
    _setup_git(target, quiet=quiet)

    print(f"\nFormat repository created at {target}")
