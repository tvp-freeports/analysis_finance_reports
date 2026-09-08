"""``ci.yaml``: what a repository demands of a commit, in one file with one name everywhere.

Every freeports repository -- the engine, a formats repository, an input database -- is gated by a
file called ``ci.yaml`` at its root, written in one syntax. That is deliberate and it was a
reversal: an earlier round of this design put the settings inside each repository's own manifest,
``package.yaml`` here and ``metadata.yaml`` there, and the roles of the branches then read
differently in every repository. A manifest says *what the repository is*; this file says *how it is
gated*, which is a different question with a different audience, and the engine repository -- which
has no manifest at all -- would have needed one invented for the purpose.

::

    branches:
      prod: [main, master, "release/*"]
      dev:  [dev, generalization]
      off:  [experimental, "wip/*"]
      default: dev
    keyserver: https://keys.openpgp.org
    thresholds:
      tests.python.lines: 60
      lint.rust: 9.9

**Absent, everything falls to the defaults**: class ``dev``, no thresholds, the default key server --
which is to say a repository nobody has configured reports and never refuses. That is the right
behaviour for a repository nobody has configured yet, and it is why the file's absence is not an
error.

Two things in it *are* errors, and loudly:

* a branch matching two class lists -- resolving that silently would make the class of a commit
  depend on the order of a mapping nobody reads;
* a threshold naming a metric this repository cannot measure -- a threshold that can never be
  evaluated is a threshold somebody thinks they have.

The three tiers are the engine's own, resolved per setting: **command line, then environment, then
this file, then the default**. Thresholds are per metric all the way down, so ``--min`` on one
metric does not discard the file's minima for the others.
"""

import fnmatch
import os
import subprocess
from pathlib import Path

from freeports_dev.ci import metrics


#: The name of the file, in every repository kind. There is no second spelling.
CI_FILE = "ci.yaml"

#: A commit here is refused when a threshold is missed, a suite fails or a figure is unmeasured.
PROD = "prod"

#: A commit here reports everything and refuses nothing.
DEV = "dev"

#: The hook does nothing at all -- a scratch branch, a spike.
OFF = "off"

#: In the order a report lists them.
BRANCH_CLASSES = (PROD, DEV, OFF)

#: What an unlisted branch is, unless ``branches.default`` says otherwise.
DEFAULT_BRANCH_CLASS = DEV

#: Where a fingerprint is looked up when nothing names another server.
DEFAULT_KEYSERVER = "https://keys.openpgp.org"


class ConfigError(Exception):
    """A ``ci.yaml`` that cannot be obeyed. Always exit status 2, never a refused commit.

    The distinction matters: a refused commit means the repository's rules were applied and the
    commit failed them; this means the rules themselves do not parse, which is a different thing to
    tell somebody and a different thing to fix.
    """


def detect_repo_kind(root):
    """Which of the three kinds of repository ``root`` is, by what it actually contains.

    Not by what ``ci.yaml`` claims to be: the file is the thing being validated, and a repository
    that could declare its own kind could declare a kind whose metrics it cannot measure, which is
    the error this is here to catch.
    """
    root = Path(root)
    if (root / "metadata" / "formats.csv").exists():
        return metrics.FORMATS
    if (root / "metadata.yaml").exists() and (root / "companies").is_dir():
        return metrics.INPUT_DB
    if (root / "packages" / "freeports").is_dir():
        return metrics.ENGINE
    return None


def current_branch(root):
    """The branch checked out at ``root``, or ``None`` for a detached HEAD or no git at all.

    Derived at every run rather than recorded anywhere, so switching branch sets nothing and
    forgets nothing. ``None`` is not a failure: it falls to the default class, and the gate says
    which class and why.

    ``symbolic-ref`` and not ``rev-parse --abbrev-ref``, which is the obvious spelling and the
    wrong one twice over. It answers the literal string ``HEAD`` on a detached HEAD, which is
    indistinguishable from a branch actually called ``HEAD``; and it fails outright on a branch
    that has no commit yet, so a freshly initialised repository would be read as having no branch
    at all and gated by the default class for a reason that has nothing to do with its branch.
    ``symbolic-ref`` reads the ref rather than resolving it: it needs no commit, and its failure
    means detached and nothing else.
    """
    try:
        done = subprocess.run(
            ["git", "-C", str(root), "symbolic-ref", "--short", "-q", "HEAD"],
            capture_output=True,
            text=True,
            check=False,
        )
    except OSError:
        return None
    if done.returncode != 0:
        return None
    return done.stdout.strip() or None


class BranchClass:
    """The resolved class of a branch, together with the rule that produced it.

    The rule is carried rather than recomputed because it is the whole content of
    ``freeports-dev branch-class`` -- the first thing to run when a hook does something
    unexpected. "prod" answers nothing; "prod, because *main* matched the pattern *main* in
    branches.prod" answers everything.
    """

    def __init__(self, name, reason, branch=None):
        self.name = name
        self.reason = reason
        self.branch = branch

    @property
    def refuses(self):
        """Whether a failure on this branch stops the commit, rather than being reported."""
        return self.name == PROD

    @property
    def silent(self):
        """Whether the hook does nothing at all here."""
        return self.name == OFF

    def __str__(self):
        where = f"branch {self.branch}" if self.branch else "no branch"
        return f"{where} -> {self.name} ({self.reason})"


def _class_name(value):
    """A branch-class name as written, with YAML's opinion about ``off`` undone.

    ``off`` is one of the ten words YAML 1.1 spells a boolean with, so a file that reads

    ::

        branches:
          off: [experimental]

    arrives here with the key ``False`` and the class would appear to be misspelt. The class is
    called ``off`` because that is what it means and what the plan for this file writes; quoting it
    in every repository's ``ci.yaml`` to work around a parser would be a worse file to read. So the
    boolean is mapped back, and both spellings -- ``off`` and ``"off"`` -- name the same class.

    ``prod`` and ``dev`` are not words YAML has an opinion about, so nothing else needs this.
    """
    if value is False:
        return OFF
    if value is True:
        return "on"
    return value


def _as_patterns(value, key):
    if value is None:
        return []
    if isinstance(value, str):
        return [value]
    if isinstance(value, (list, tuple)):
        bad = [item for item in value if not isinstance(item, str)]
        if bad:
            raise ConfigError(f"branches.{key} must be a list of branch names: {bad!r}")
        return list(value)
    raise ConfigError(f"branches.{key} must be a branch name or a list of them")


class CiConfig:
    """The gating settings of one run, already resolved across the three tiers.

    Built from a repository root and the parsed command line, so every subcommand reads the same
    answers instead of each one re-implementing the precedence.
    """

    def __init__(self, root, args=None, repo_kind=None):
        self.root = Path(root)
        self._args = args
        self.repo_kind = (
            repo_kind if repo_kind is not None else detect_repo_kind(self.root)
        )
        self.path = self.root / CI_FILE
        self._file = self._load()
        self._validate()

    # -- the file ---------------------------------------------------------------------------

    def _load(self):
        """The parsed ``ci.yaml``, or an empty mapping when there is none.

        A file that exists and does not parse is an error, unlike one that is simply absent: the
        first is a mistake somebody made, the second is a repository nobody has configured.
        """
        if not self.path.exists():
            return {}
        import yaml

        try:
            doc = yaml.safe_load(self.path.read_text(encoding="utf-8"))
        except Exception as exc:
            raise ConfigError(f"{self.path} does not parse: {exc}") from exc
        if doc is None:
            return {}
        if not isinstance(doc, dict):
            raise ConfigError(
                f"{self.path} must hold a mapping, not {type(doc).__name__}"
            )
        return doc

    @property
    def exists(self):
        return self.path.exists()

    def _arg(self, name):
        return getattr(self._args, name, None) if self._args is not None else None

    # -- validation -------------------------------------------------------------------------

    def _validate(self):
        """Everything about the file that can be wrong independently of what is being measured."""
        branches = self._file.get("branches") or {}
        if not isinstance(branches, dict):
            raise ConfigError(
                "branches: must hold a mapping of class name to branch names"
            )
        branches = {_class_name(key): value for key, value in branches.items()}
        self._branches = branches
        unknown = set(branches) - set(BRANCH_CLASSES) - {"default"}
        if unknown:
            raise ConfigError(
                f"branches: names no such class: {', '.join(sorted(unknown))} "
                f"(the classes are {', '.join(BRANCH_CLASSES)})"
            )
        default = _class_name(branches.get("default", DEFAULT_BRANCH_CLASS))
        if default not in BRANCH_CLASSES:
            raise ConfigError(
                f"branches.default: {default!r} is no such class "
                f"(the classes are {', '.join(BRANCH_CLASSES)})"
            )
        for name in BRANCH_CLASSES:
            _as_patterns(branches.get(name), name)

        raw = self._file.get("thresholds") or {}
        if not isinstance(raw, dict):
            raise ConfigError(
                "thresholds: must hold a mapping of metric name to a number"
            )
        for name, value in raw.items():
            self._check_metric_name(str(name), f"thresholds: in {self.path.name}")
            _as_number(value, name)

    def _check_metric_name(self, name, where):
        """A threshold on a metric this repository cannot measure is a deliberate error.

        Silently ignoring it would leave the owner believing the repository is gated on something
        it never looks at -- exactly the failure this project already refuses for an unreachable
        methodology page, where a figure nobody could compute must never be reported as a pass.
        """
        metric = metrics.find(name)
        if metric is None:
            raise ConfigError(f"{where}: {name!r} is no metric this tool knows")
        if self.repo_kind is None:
            return
        if self.repo_kind not in metric.repo_kinds:
            answers = ", ".join(metric.repo_kinds)
            raise ConfigError(
                f"{where}: {name!r} is not measured in a {self.repo_kind} repository "
                f"(only in: {answers}) -- a threshold that can never be evaluated is a "
                f"threshold you only think you have"
            )

    # -- branch class -----------------------------------------------------------------------

    def branch_class(self):
        """The class of the branch checked out now, with the rule that produced it.

        Command line, then environment, then the lists in the file, then ``branches.default``,
        then ``dev``.
        """
        override = self._arg("branch_class") or os.environ.get(
            "FREEPORTS_CI_BRANCH_CLASS"
        )
        if override:
            override = override.strip()
            if override not in BRANCH_CLASSES:
                raise ConfigError(
                    f"{override!r} is no such branch class "
                    f"(the classes are {', '.join(BRANCH_CLASSES)})"
                )
            source = (
                "--branch-class"
                if self._arg("branch_class")
                else "FREEPORTS_CI_BRANCH_CLASS"
            )
            return BranchClass(
                override, f"forced by {source}", current_branch(self.root)
            )

        branches = self._branches
        default = _class_name(branches.get("default", DEFAULT_BRANCH_CLASS))
        branch = current_branch(self.root)
        if branch is None:
            return BranchClass(
                default,
                f"no branch is checked out (detached HEAD, or no git here), so the "
                f"default class {default} applies",
                None,
            )

        hits = []
        for name in BRANCH_CLASSES:
            for pattern in _as_patterns(branches.get(name), name):
                if fnmatch.fnmatchcase(branch, pattern):
                    hits.append((name, pattern))
        if len(hits) > 1:
            listed = "; ".join(f"{cls} lists {pat!r}" for cls, pat in hits)
            raise ConfigError(
                f"branch {branch!r} matches more than one class in {self.path.name}: {listed}. "
                f"Which one wins is not something this tool should decide quietly -- name the "
                f"branch in one class only."
            )
        if hits:
            name, pattern = hits[0]
            return BranchClass(name, f"{pattern!r} in branches.{name}", branch)
        return BranchClass(
            default,
            f"no pattern matches, so the default class {default} applies",
            branch,
        )

    # -- thresholds -------------------------------------------------------------------------

    def thresholds(self):
        """Every configured minimum, by metric name, across the three tiers.

        Resolved per metric rather than per source: ``--min lint.rust=9.9`` raises the bar for
        clippy and leaves every other minimum the file declares exactly where it was.
        """
        resolved = {}
        for name, value in (self._file.get("thresholds") or {}).items():
            resolved[str(name)] = _as_number(value, name)
        for name, value in self._env_thresholds().items():
            self._check_metric_name(name, "the environment")
            resolved[name] = value
        for name, value in self._arg_thresholds().items():
            self._check_metric_name(name, "--min")
            resolved[name] = value
        return resolved

    def _env_thresholds(self):
        """``FREEPORTS_CI_MIN_TESTS_RUST_LINES=70`` and its siblings.

        The variable is matched back to a metric name rather than parsed apart, because the mapping
        from dots to underscores is not invertible: ``tests.python.freeports_validate.lines``
        and a package called ``freeports.validate`` would produce the same variable. Comparing
        against the names the registry can produce keeps the ambiguity out.
        """
        found = {}
        prefix = "FREEPORTS_CI_MIN_"
        candidates = {}
        for name in list(self._file.get("thresholds") or {}) + [
            m.pattern for m in metrics.REGISTRY if not m.is_family
        ]:
            candidates[metrics.env_name(str(name))] = str(name)
        for variable, raw in os.environ.items():
            if not variable.startswith(prefix) or not raw.strip():
                continue
            name = candidates.get(variable)
            if name is None:
                name = _metric_name_from_env(variable)
                if name is None:
                    raise ConfigError(
                        f"{variable} names no metric this tool knows. Per-package minima are set "
                        f"in {CI_FILE} or with --min, where the package name keeps its dots."
                    )
            found[name] = _as_number(raw, variable)
        return found

    def _arg_thresholds(self):
        """``--min metric=value``, repeatable, so a growing set of metrics needs no new flags."""
        found = {}
        for item in self._arg("min") or []:
            if "=" not in item:
                raise ConfigError(f"--min wants metric=value, not {item!r}")
            name, _, raw = item.partition("=")
            found[name.strip()] = _as_number(raw.strip(), f"--min {name.strip()}")
        return found

    # -- key server -------------------------------------------------------------------------

    @property
    def keyserver(self):
        """Where a granter's fingerprint is looked up.

        A setting rather than a constant because ``keys.openpgp.org`` is the default and not a
        law: a repository may publish to another server, and this project could not confirm that
        every server it was pointed at is live.
        """
        for value in (
            self._arg("keyserver"),
            os.environ.get("FREEPORTS_VALIDATE_KEYSERVER"),
            self._file.get("keyserver"),
        ):
            if value:
                return str(value).strip()
        return DEFAULT_KEYSERVER


def _as_number(value, where):
    try:
        return float(value)
    except (TypeError, ValueError):
        raise ConfigError(f"{where}: {value!r} is not a number") from None


def _metric_name_from_env(variable):
    """The metric a per-package ``FREEPORTS_CI_MIN_*`` variable names, if exactly one family fits.

    Underscores are ambiguous coming back -- a package name may hold them -- so this only accepts a
    variable whose remainder round-trips through :func:`metrics.env_name` to the same string. It is
    a convenience for the common case; ``ci.yaml`` and ``--min`` remain the unambiguous spellings.
    """
    body = variable[len("FREEPORTS_CI_MIN_") :].lower()
    for prefix, suffix in (("tests_python_", "_lines"), ("docs_python_", "")):
        if not body.startswith(prefix) or (suffix and not body.endswith(suffix)):
            continue
        package = body[len(prefix) : len(body) - len(suffix) if suffix else None]
        if not package:
            continue
        family = prefix.replace("_", ".").rstrip(".") + "."
        candidate = f"{family}{package}{suffix.replace('_', '.')}"
        if metrics.env_name(candidate) == variable:
            return candidate
    return None
