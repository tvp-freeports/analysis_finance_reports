"""The metrics this workspace gates on, and which repository can answer for which.

A metric is a dotted name, a unit and a **cost class**, and the name is the same string everywhere
it appears -- in ``ci.yaml``, in the environment variable, on the command line and in the report.
One name written four ways is the whole reason this registry is a table and not a set of constants
scattered through the commands that produce the numbers.

**Cost is a property of the metric, not of the caller.** ``tests.rust.lines`` recompiles the crate
instrumented and costs minutes; it is slow whether a hook, a person or a pipeline asks for it. That
is what lets the commit hook run the fast set at every commit and *read* the slow ones from the last
full run, instead of each caller guessing what it can afford.

``FAST`` means **seconds, locally, always**. A metric is ``SLOW`` if it costs more than that *or* if
it needs the network, and the two are one class here rather than two: a figure that depends on
somebody else's host being up is not a figure about the commit being made, so it belongs with the
instrumented recompilation and not in a gate somebody has to clear on a train. ``grants.coverage``
and ``grants.keys_online`` are slow for that reason and not for their wall-clock cost; the whole
budget for a commit gate is a few seconds, and it is spent on the code being committed.

Two metric families are **per package**: a repository may gate ``tests.python.lines`` as a whole and
still hold one package to a different figure. The rule is that the most specific configured name
wins for that package and the aggregate applies to whatever no specific name claims -- a package
with its own threshold leaves the aggregate's denominator, so the two can never contradict each
other. See :func:`split_package_key`.

Not everything gated here is a metric. The outcome of a test suite and the fingerprint/version rule
are conditions, not numbers: they have no unit, no baseline and no threshold, and they live in the
gate rather than in this table.
"""

import fnmatch


#: A repository whose ``packages/`` hold the crate and the tooling -- ``analysis_finance_reports``.
ENGINE = "engine"

#: A formats repository: ``metadata/formats.csv``, ``content/``, one ``package.yaml``.
FORMATS = "formats"

#: An input database: ``companies/``, ``lists/``, one ``metadata.yaml``.
INPUT_DB = "input_db"

#: Every repository kind, in the order a report lists them.
REPO_KINDS = (ENGINE, FORMATS, INPUT_DB)

#: Measured at every commit, in seconds.
FAST = "fast"

#: Measured by ``make ci`` and read from the last one at commit time. Minutes, not seconds.
SLOW = "slow"

#: A ratio rendered as a percentage.
PERCENT = "percent"

#: Pylint's score out of ten -- see :mod:`freeports_dev.ci.lint`.
SCORE = "score"


class Metric:
    """One gated figure: what it is called, what it is in, what it costs, and who can answer.

    ``pattern`` is the name as a shell glob, so that the per-package families -- which cannot be
    enumerated, since the set of packages is whatever the repository holds -- are matched rather
    than listed. A metric with no wildcard matches only itself.
    """

    def __init__(self, pattern, unit, cost, repo_kinds, description):
        self.pattern = pattern
        self.unit = unit
        self.cost = cost
        self.repo_kinds = tuple(repo_kinds)
        self.description = description

    @property
    def is_family(self):
        """True when the name carries a wildcard, i.e. it names a family and not one figure."""
        return "*" in self.pattern

    def matches(self, name):
        return fnmatch.fnmatchcase(name, self.pattern)

    def __repr__(self):
        return f"<Metric {self.pattern}>"


#: Every metric the gate knows, in the order a report prints them.
#:
#: The baselines this table was seeded from were measured, never wished for: the pipeline that
#: preceded this one asked for 90 % of a documentation figure that read 25.93 %, which is how a
#: threshold nobody can clear ends up being the threshold everybody disables.
REGISTRY = (
    Metric(
        "tests.rust.lines",
        PERCENT,
        SLOW,
        (ENGINE,),
        "Line coverage of the crate, from cargo llvm-cov.",
    ),
    Metric(
        "tests.python.lines",
        PERCENT,
        SLOW,
        (ENGINE,),
        "Line coverage of the Python packages, from pytest --cov.",
    ),
    Metric(
        "tests.python.*.lines",
        PERCENT,
        SLOW,
        (ENGINE,),
        "Line coverage of one named Python package.",
    ),
    Metric(
        "tests.formats.integration",
        PERCENT,
        FAST,
        (FORMATS,),
        "Documents whose whole-document test exists at all.",
    ),
    Metric(
        "tests.formats.single_page",
        PERCENT,
        FAST,
        (FORMATS,),
        "Documents carrying at least one page with the whole fixture triple.",
    ),
    Metric(
        "docs.rust",
        PERCENT,
        SLOW,
        (ENGINE,),
        "Documented items of the crate, from rustdoc --show-coverage.",
    ),
    Metric(
        "docs.python",
        PERCENT,
        FAST,
        (ENGINE, FORMATS),
        "Public Python objects carrying a docstring.",
    ),
    Metric(
        "docs.python.*",
        PERCENT,
        FAST,
        (ENGINE,),
        "Public objects carrying a docstring, in one named package.",
    ),
    Metric(
        "lint.rust",
        SCORE,
        FAST,
        (ENGINE,),
        "Clippy diagnostics scored out of ten.",
    ),
    Metric(
        "lint.python",
        SCORE,
        FAST,
        (ENGINE, FORMATS),
        "Ruff violations scored out of ten.",
    ),
    Metric(
        "grants.coverage",
        PERCENT,
        SLOW,
        (ENGINE, FORMATS, INPUT_DB),
        "Files covered by a methodology grant.",
    ),
    Metric(
        "grants.keys_online",
        PERCENT,
        SLOW,
        (ENGINE, FORMATS, INPUT_DB),
        "Granters' keys published on the configured key server.",
    ),
)


def find(name):
    """The metric a name belongs to, or ``None`` if no family claims it.

    Exact names are tried before families, so that ``docs.python`` is itself and not an oddly
    spelled member of ``docs.python.*``. Without that order the aggregate and the per-package
    family would fight over the same string, and which one won would depend on table order.
    """
    for metric in REGISTRY:
        if not metric.is_family and metric.pattern == name:
            return metric
    for metric in REGISTRY:
        if metric.is_family and metric.matches(name):
            return metric
    return None


def known_in(repo_kind):
    """The metrics a repository of this kind can answer for.

    This is what makes a threshold on a metric the repository cannot measure a *configuration
    error* rather than a silent no-op: a formats repository naming ``tests.rust.lines`` has written
    down a demand nothing will ever evaluate, and a threshold that can never be evaluated is a
    threshold somebody thinks they have.
    """
    return tuple(m for m in REGISTRY if repo_kind in m.repo_kinds)


def is_known_in(name, repo_kind):
    metric = find(name)
    return metric is not None and repo_kind in metric.repo_kinds


#: The two families whose middle segment names a package rather than being part of the metric.
_PACKAGE_FAMILIES = (("tests.python.", ".lines"), ("docs.python.", ""))


def split_package_key(name):
    """``("tests.python.lines", "freeports_validate")`` for a per-package name, else ``None``.

    The aggregate a per-package key overrides is the same name with the package taken out, which is
    what lets the gate remove a specifically named package from the aggregate's denominator without
    a second table saying which aggregate each family belongs to.
    """
    for prefix, suffix in _PACKAGE_FAMILIES:
        if not name.startswith(prefix) or not name.endswith(suffix):
            continue
        package = name[len(prefix) : len(name) - len(suffix) if suffix else None]
        if not package or "." in package:
            continue
        return prefix.rstrip(".") + suffix, package
    return None


def env_name(metric_name):
    """The environment variable that sets this metric's minimum.

    The metric upper-cased, dots and dashes as underscores, prefixed ``FREEPORTS_CI_MIN_`` --
    mechanical, so that a person who knows the metric knows the variable without a lookup table.
    """
    return "FREEPORTS_CI_MIN_" + metric_name.upper().replace(".", "_").replace("-", "_")
