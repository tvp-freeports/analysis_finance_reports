"""The test suites this workspace gates on, what each one costs, and how to run it.

This is :mod:`freeports_dev.ci.metrics` for the other half of what a commit is judged on. A metric
is a number with a minimum; a suite is a set of tests with an outcome. Both are facts about one
commit, both are recorded into ``reports/`` with the commit they were established at, and both are
judged by :mod:`freeports_dev.ci.gate` — which is what lets the commit hook run part of the tests
now and *read* the rest from the last full run without anybody being able to mistake the second
kind for the first.

**Why a suite is recorded at all.** Before this registry the outcome of a run was a string on a
command line — ``ci-check --suite fast:passed`` — which said what had just happened and nothing
about what had *not*. A gate that runs a quarter of the tests and reports "tests: ok" is worse than
one that runs none, because it is a gate that lies. Recording each suite under its own name, with
the commit it ran at, turns the silence into a verdict: a suite nobody has run at this commit reads
``not run``, and one whose last run was at another commit reads ``stale``.

**Cost is a property of the suite, not of the caller**, exactly as it is for a metric. And *slow*
here means "not something to pay at every commit", which covers two things the workspace treats as
one:

* it takes seconds or minutes — the crate's integration tests, the doctests, the process-per-test
  half of the tooling suite, a formats repository's whole-document runs;
* it needs the network — and then it depends on somebody else's host being up, which is not a
  property of the commit being made.

The second is flagged by :attr:`Suite.online` so that a message can say *which* kind of expense it
is, but both are ``SLOW``: they run when a person asks for them, and on a production branch they
must have run at HEAD before the commit is allowed through.

**Kept apart, treated alike.** ``python.slow`` and ``python.online`` are two suites and not one:
they fail for different reasons and a person fixes them differently — one is the command's own
architecture, the other is somebody else's server — so collapsing them would throw away the only
information a failure carries. What they share is the *policy*: neither runs at every commit,
both are recorded with the commit they ran at, and both must be current before a production commit
goes through. Which is to say the split is in the table and not in the rules.

``command`` is the whole point of the table being a table. A verdict that says a suite is stale and
does not say how to un-stale it is a verdict people route around, so every entry carries the
command that answers it.
"""

from freeports_dev.ci.metrics import ENGINE, FAST, FORMATS, SLOW  # noqa: F401


#: The suite ran and every test in it passed.
PASSED = "passed"

#: The suite ran and something in it did not pass.
FAILED = "failed"


class Suite:
    """One set of tests: what it is called, what it costs, who has one, and how it is run."""

    def __init__(self, name, cost, repo_kinds, description, command, online=False):
        self.name = name
        self.cost = cost
        self.repo_kinds = tuple(repo_kinds)
        self.description = description
        self.command = command
        self.online = online

    @property
    def expense(self):
        """Why this suite is not in the commit gate, in the words a message should use."""
        if self.cost == FAST:
            return None
        return "it needs the network" if self.online else "it costs more than seconds"

    def __repr__(self):
        return f"<Suite {self.name}>"


#: Every suite the gate knows, in the order a report lists them: fast before slow, and within each
#: the order somebody would run them.
#:
#: The names are ``<language or repository>.<which>``, so they sort into their own groups and so a
#: reader who has met ``tests.rust.lines`` in the metric table recognises ``rust.unit`` here.
REGISTRY = (
    Suite(
        "rust.unit",
        FAST,
        (ENGINE,),
        "The crate's unit tests, in the `mod tests` blocks inside `src/`.",
        "make test-rust-unit",
    ),
    Suite(
        "python.fast",
        FAST,
        (ENGINE,),
        "The tooling packages' tests that do not start the command as a process.",
        "make test-python",
    ),
    Suite(
        "formats.single_page",
        FAST,
        (FORMATS,),
        "A formats repository's per-page tests: one page of one document at a time.",
        "freeports-dev test -- -m 'not integration_tests'",
    ),
    Suite(
        "rust.integration",
        SLOW,
        (ENGINE,),
        "The crate's integration tests, one file per flow, in `packages/freeports/tests/`.",
        "make test-rust-integration",
    ),
    Suite(
        "rust.doc",
        SLOW,
        (ENGINE,),
        "The examples inside the crate's doc-comments.",
        "make test-rust-doc",
    ),
    Suite(
        "python.slow",
        SLOW,
        (ENGINE,),
        "The tooling tests that start the command as a process, tens of process starts each.",
        "make test-python-slow",
    ),
    Suite(
        "python.online",
        SLOW,
        (ENGINE,),
        "The tooling tests that reach the real documentation site.",
        "make test-python-online",
        online=True,
    ),
    Suite(
        "formats.integration",
        SLOW,
        (FORMATS,),
        "A formats repository's whole-document tests, each one a full extraction run.",
        "freeports-dev test -- -m integration_tests",
    ),
)

#: Grant integrity is **not** here, and its absence is a decision rather than an oversight.
#:
#: `freeports-validate check-grants` is the same shape as a suite — it runs, it passes or it does
#: not — but it must never refuse a commit on any branch, and everything in this table refuses on
#: `prod`. A granted reference output regenerated by `make-tests` is ordinary work, and a
#: contributor who does not hold the granting key could not clear such a gate at all. What says
#: whether the grants have been looked at recently is the `grants.coverage` *metric*, which is slow
#: for the same reasons the suites above are and goes stale in exactly the same way.


def find(name):
    """The suite a name belongs to, or ``None``. Suites have no families, so this is a lookup."""
    for suite in REGISTRY:
        if suite.name == name:
            return suite
    return None


def known_in(repo_kind):
    """The suites a repository of this kind has.

    The same rule the metric table follows: a repository is judged on what it can actually answer
    for, so a formats repository is never asked about the crate's doctests and never reports them
    as missing.
    """
    return tuple(s for s in REGISTRY if repo_kind in s.repo_kinds)


def parse_outcome(text):
    """``passed`` and its synonyms into :data:`PASSED`, and **anything else** into :data:`FAILED`.

    Deliberately asymmetric. This string is written by a shell variable in a commit hook, and the
    one way this must not fail is by reading a word nobody intended — an empty variable, a typo, a
    localised ``ok`` — as success. Only the spellings listed here mean the suite passed.
    """
    return (
        PASSED if str(text).strip().lower() in ("passed", "pass", "ok", "0") else FAILED
    )


def parse(text):
    """``"rust.unit:passed"`` into ``("rust.unit", "passed")``.

    An outcome that is absent is a failure for the reason above: ``--suite rust.unit`` with the
    colon left off says nothing about how the run went, and the safe reading of nothing is not
    "fine".
    """
    name, _, outcome = str(text).partition(":")
    return name.strip(), parse_outcome(outcome)
