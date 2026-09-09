"""Initialize a new freeports repository skeleton — a formats repository or an input database.

The two are different things and are used by different people, but they are bootstrapped the same
way: a list of directories, a set of files that are nothing but their header row, a manifest, and an
optional git repository around them. So they share the readers below, and each entry point differs
only in which data files under lib/ it names.

Reads template files from the adjacent lib/ directory so content can be modified without touching
Python code.
"""

import json
import subprocess
import sys
import tempfile
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


def _write_hook(target: Path, template: str) -> None:
    """Write `.githooks/pre-commit` from a template, executable.

    One helper for both repository kinds, because the two hooks differ only in what they check.
    """
    githooks = target / ".githooks"
    githooks.mkdir(exist_ok=True)
    hook = githooks / "pre-commit"
    hook.write_text(_read_template(template), encoding="utf-8")
    hook.chmod(0o755)
    print("  created .githooks/pre-commit")


def _write_build_system(target: Path) -> None:
    """Write the `Makefile` and its Windows shim: the repository's single entry point.

    A format repository is worked on with `make` and interrogated with `freeports-dev`, and the
    two are deliberately the same vocabulary — `make test-fast` is `freeports-dev test --fast`,
    `make ci-check` is `freeports-dev ci-check`. What the Makefile adds is that it acts on *this*
    repository: it writes into `reports/`, refreshes the committed report artefacts and rewrites
    the manifest when it is entitled to. The command acts on any repository, including one that is
    not yours, and only where you point it with `--out`.

    Generated here rather than left to the author for the same reason the hook is: the hook runs
    `make pre-commit`, so a repository created without a Makefile would be one whose commit gate
    fails at the first commit, naming a file nobody told them to write.

    `make.bat` is a shim of a dozen lines that starts a POSIX shell and hands it the same Makefile.
    There is one build system in a format repository, not one per platform.
    """
    (target / "Makefile").write_text(
        _read_template("Makefile.template"), encoding="utf-8"
    )
    (target / "make.bat").write_text(
        _read_template("make.bat.template"), encoding="utf-8"
    )
    # And the list of what those targets leave behind. Without it the first commit made in a new
    # repository carries `reports/` into the history and every commit after it shows the same files
    # modified again -- which is the state the hook's staging of the report artefacts exists to
    # avoid, arrived at from the other side.
    (target / ".gitignore").write_text(
        _read_template("gitignore.template"), encoding="utf-8"
    )
    print("  wrote Makefile, make.bat and .gitignore")


def _write_ci_yaml(target: Path) -> None:
    """Write `ci.yaml`, which says how this repository is gated.

    The same file, the same name and the same syntax in every freeports repository, so that the
    roles of the branches read the same everywhere. `package.yaml` and `metadata.yaml` say what a
    repository *is*; this says how it is *gated*, which is a different question with a different
    audience.
    """
    (target / "ci.yaml").write_text(
        _read_template("ci.template.yaml"), encoding="utf-8"
    )
    print("  wrote ci.yaml")


def _setup_git(target: Path, quiet: bool = False, hooks: bool = True) -> None:
    """Initialize the target as a git repository, and point it at .githooks when asked."""
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

    if not hooks:
        return

    subprocess.run(
        ["git", "-C", str(target), "config", "--local", "core.hooksPath", ".githooks"],
        check=True,
        capture_output=True,
    )
    print("  git hooks configured (core.hooksPath = .githooks)")


def _write_validation_report(target: Path) -> list:
    """Write the README and the six pages that host the validation report.

    The report is a function of what `validation/` claims, and `freeports-validate report` writes it
    between two markers rather than over a whole file — so the files have to exist, with their
    markers in them, before anything can be written into them. Creating them here is what makes the
    repository's `pre-commit` hook a refresh rather than a first draft.

    Six pages because the repository has three dimensions — file, contributor, methodology — and
    each of the three lookup subcommands groups by one and lists another, with and without its
    flag. They are the same six the command offers as `--table`, and each page is named after the
    value that fills it, so the hook line and the file it writes read the same.

    Returns the paths written, relative to `target`, in the order they were written.
    """
    entries = _read_json("report_tables.json")
    template = _read_template("report_table.template.md")

    written = ["README.md"]
    (target / "README.md").write_text(
        _read_template("readme.template.md").replace("{name}", target.name),
        encoding="utf-8",
    )
    for entry in entries:
        page = f"validation/report/{entry['table']}.md"
        body = template
        for key in ("table", "title", "mirrors"):
            body = body.replace("{" + key + "}", entry[key])
        (target / page).write_text(body, encoding="utf-8")
        written.append(page)
    return written


def _fill_validation_report(target: Path) -> bool:
    """Run the command once so the badges exist and every marked block is filled.

    Best-effort, and deliberately so. A skeleton whose creation needed a reachable documentation
    server would be a worse tool than one that leaves nine files to fill: the failure here is a note
    naming the command to run, never a half-created repository. What it costs when it does work is
    one walk of an empty repository, which resolves the general methodology and nothing else.

    One `collect`, rendered nine times, for the reason the hook does the same: nine collections
    would re-resolve the same pages nine times for an answer that cannot have changed in between.
    """
    entries = _read_json("report_tables.json")

    def validate(*arguments, **kwargs):
        return subprocess.run(
            ["freeports-validate", "--repo", str(target), *arguments],
            check=True,
            capture_output=True,
            **kwargs,
        )

    with tempfile.NamedTemporaryFile("w+", suffix=".json") as model:
        try:
            collected = validate("collect")
        except FileNotFoundError:
            print("  note: freeports-validate is not installed, so the report is empty")
            print(
                "        install it, then run: freeports-validate report --format badges "
                "--out validation/report/badges/"
            )
            return False
        except subprocess.CalledProcessError:
            print(
                "  note: the methodologies could not be resolved, so the report is empty"
            )
            print(
                "        run `freeports-validate sources` to see why, then re-run the hook"
            )
            return False

        model.write(collected.stdout.decode("utf-8"))
        model.flush()

        renderings = [
            ("--format", "badges", "--out", str(target / "validation/report/badges")),
            ("--format", "markdown", "--out", str(target / "README.md")),
        ]
        renderings += [
            (
                "--format",
                "markdown",
                "--table",
                entry["table"],
                "--out",
                str(target / f"validation/report/{entry['table']}.md"),
            )
            for entry in entries
        ]
        for rendering in renderings:
            try:
                validate("report", "--model", model.name, *rendering)
            except subprocess.CalledProcessError:
                print(f"  note: could not write {rendering[-1]}")
                return False

    print("  wrote the badges and filled the report")
    return True


def _write_ci_report(target: Path) -> list:
    """Write the two pages that host the CI report, with their markers in them.

    The twin of :func:`_write_validation_report`, for the other half of what a repository publishes
    about itself. `freeports-dev ci-report` writes between two markers rather than over a whole
    file, so the files have to exist before anything can be written into them — creating them here
    is what makes the repository's `pre-commit` hook a refresh rather than a first draft.

    Two pages and not three: the summary lives in the README, where a reader meets it, and the two
    beside it answer the questions it provokes — what is being demanded, and where a figure that
    fell short comes from.

    Returns the paths written, relative to `target`.
    """
    entries = _read_json("ci_report_tables.json")
    template = _read_template("ci_report_table.template.md")

    written = []
    for entry in entries:
        page = f"ci/report/{entry['table']}.md"
        body = template
        for key in ("table", "title", "about"):
            body = body.replace("{" + key + "}", entry[key])
        (target / page).write_text(body, encoding="utf-8")
        written.append(page)
    return written


def _fill_ci_report(target: Path) -> bool:
    """Run the command once so the badges exist and every marked block is filled.

    Best-effort, like its validation twin, and for a smaller reason: a new repository has taken no
    measurements at all, so what this writes is a report saying exactly that — every metric not
    measured, the status `inconclusive`, and the commands to run. That is the honest first state and
    a more useful one than an empty file: it tells the author what the repository will be asked for
    before they have anything to be asked about.

    One evaluation rendered several times, so the badges cannot disagree with the pages.
    """
    entries = _read_json("ci_report_tables.json")

    def dev(*arguments, **kwargs):
        return subprocess.run(
            ["freeports-dev", "ci-report", "--repo", str(target), *arguments],
            check=True,
            capture_output=True,
            **kwargs,
        )

    with tempfile.NamedTemporaryFile("w+", suffix=".json") as model:
        try:
            dev("--format", "json", "--out", model.name)
        except FileNotFoundError:
            print("  note: freeports-dev is not installed, so the CI report is empty")
            return False
        except subprocess.CalledProcessError:
            print(
                "  note: the CI report could not be evaluated, so its pages are empty"
            )
            print("        run `freeports-dev ci-check` to see why")
            return False

        # A markdown rendering is written *between markers* in a file that has to exist already, so
        # one whose page is absent is skipped rather than attempted. An input database has no
        # README at all, and a repository kind acquiring one later should not need this list edited.
        renderings = [("--format", "badges", "--out", str(target / "ci/report/badges"))]
        pages = [("README.md", None)] + [
            (f"ci/report/{entry['table']}.md", entry["table"]) for entry in entries
        ]
        for page, table in pages:
            if not (target / page).exists():
                continue
            arguments = ["--format", "markdown"]
            if table:
                arguments += ["--table", table]
            renderings.append(tuple(arguments + ["--out", str(target / page)]))
        renderings.append(
            ("--format", "html", "--out", str(target / "ci/report/index.html"))
        )
        for rendering in renderings:
            try:
                dev("--model", model.name, *rendering)
            except subprocess.CalledProcessError:
                print(f"  note: could not write {rendering[-1]}")
                return False

    print("  wrote the CI badges and filled the CI report")
    return True


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

    _write_build_system(target)
    _write_hook(target, "pre-commit.template")
    _write_ci_yaml(target)

    # The README, the six report pages, and the first fill of all of them
    written = _write_validation_report(target)
    print(
        f"  wrote README.md and {len(written) - 1} report pages under validation/report/"
    )
    _fill_validation_report(target)

    # The two CI report pages, and their first fill. After the validation report and not before it:
    # both rewrite README.md between their own markers, and doing them in the order the README lists
    # them keeps a diff of a fresh repository readable.
    written = _write_ci_report(target)
    print(f"  wrote {len(written)} report pages under ci/report/")
    _fill_ci_report(target)

    # Copy default input DB
    from freeports_dev.input_db import copy_default_input_db

    copy_default_input_db(target / "tests")
    print("  copied default input DB to tests/input_db/")

    # Git setup
    _setup_git(target, quiet=quiet)

    print(f"\nFormat repository created at {target}")


def init_input_db(target: Path, sample: bool = False, quiet: bool = False) -> None:
    """Create a new input database skeleton at `target`.

    What comes out is an empty but *complete* database: all seven tables the engine reads exist and
    carry their header row. That completeness is the point — the loader requires every one of them,
    so a skeleton missing the file you have nothing to put in yet would fail on the first run for a
    reason that has nothing to do with what you were doing.

    It gets a `.githooks/pre-commit` of its own, which it did not use to. A database has no test
    suite, so the hook checks the one thing a database can be wrong about on its own terms: that
    the fingerprints in `metadata.yaml` and the version it declares moved together.

    Parameters
    ----------
    target : Path
        Path to the new database directory. Must be empty or non-existent (a .git directory is
        tolerated).
    sample : bool
        If True, overwrite the empty tables with the packaged example database — the single list
        `TEST` and the few hundred companies in it — as a starting point to edit down.
    quiet : bool
        If True, skip interactive prompts (defaults to yes for git init).
    """
    if target.exists() and not _is_empty_dir(target):
        print(f"Error: {target} is not empty")
        sys.exit(1)

    target.mkdir(parents=True, exist_ok=True)

    dirs: list = _read_json("input_db_dirs.json")
    csv_headers: dict = _read_json("input_db_csv_headers.json")

    for d in dirs:
        (target / d).mkdir(parents=True, exist_ok=True)
    print(f"  created {len(dirs)} directories")

    for rel_path, header in csv_headers.items():
        (target / rel_path).write_text(header, encoding="utf-8")
    print(f"  wrote {len(csv_headers)} CSV files")

    (target / "metadata.yaml").write_text(
        _read_template("input_db_metadata.template.yaml"), encoding="utf-8"
    )
    (target / ".gitignore").write_text(
        _read_template("input_db_gitignore.template"), encoding="utf-8"
    )
    print("  wrote metadata.yaml, .gitignore")

    if sample:
        from freeports_dev.input_db import copy_default_input_db_into

        copy_default_input_db_into(target)
        print("  filled the tables with the packaged example database (list TEST)")

    # An input database now gets a hook, where it used to get none.
    #
    # The old reasoning was that a database has no test suite of its own, so there was nothing for a
    # hook to run. That was true and it was not the whole question: `metadata.yaml` declares a
    # fingerprint of `companies/` and of `lists/`, and when the contents move the version has to
    # move with them or the manifest holds a false statement about itself. Nothing computed those
    # fingerprints, so nothing noticed. That is what this hook is for, and it is all it does.
    _write_hook(target, "input_db_pre-commit.template")
    _write_ci_yaml(target)

    # **No CI report here**, and that is a decision rather than an omission. An input database takes
    # no measurements at all -- its hook checks the fingerprint rule and nothing else -- so every
    # figure a report could show would read "not measured", for ever, in a committed file nothing
    # refreshes. An artefact nobody updates is worse than no artefact: it is a page of dashes that
    # looks like a status. When a database gains a measurement, this is where the report starts.
    _setup_git(target, quiet=quiet, hooks=True)

    print(f"\nInput database created at {target}")
