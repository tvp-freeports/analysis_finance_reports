##@ Tests

# The suites, on two axes: **`<what>` alone is everything, `<what>-<which>` is one slice.** Nobody
# has to memorise a list of target names; they compose one and it exists.
#
#     test              both suites          test-rust          the crate
#                                            test-python        the two tooling packages
#                                            test-formats       a formats repository
#
# and the sub-selections keep going down the same path — `test-rust-unit`,
# `test-rust-integration`, `test-rust-doc`, `test-python-slow`.
#
# These names are a **rename**, and no aliases are kept: `test-full` is `test-rust`, `test-tools`
# is `test-python`, and their variants follow. Carrying an old name with a deprecation warning is
# not how this workspace renames things, and a rename with a survivor is worse than no rename —
# the hook, `dev/build.rst`, `dev/tests.rst` and `AGENTS.md` were updated in the same commit.
#
# Most of the coverage lives in the `packages/freeports` crate — both the unit tests (in the
# `mod tests` blocks inside `src/`) and the integration ones (`tests/`, one file per flow). The
# tooling packages are Python and run under pytest instead.
#
# The tests that cross into Python (the `python_boundary` modules: the ones that really open a PDF
# with PyMuPDF) only run inside the `freeports-dev` environment; outside it they fail with a
# message saying so. That is also why `make coverage-rust` needs the environment active — without
# it those six tests panic and llvm-cov writes no report at all.

check: test ## Run the test suite (canonical GNU name, alias of test)

# ---------------------------------------------------------------------------
# Fast and slow, and why the line is where it is
# ---------------------------------------------------------------------------
# **A commit gate is worth what it is run.** Every one of these suites makes the tree more solid the
# more often it runs — that is the whole argument for testing at every commit rather than at every
# release. But the argument turns on itself the moment the gate costs real time: a suite that takes
# a minute is a suite people commit around, in bigger and rarer commits, and a gate that is skipped
# gates nothing. The cheap gate that runs every time beats the thorough one that runs on Fridays.
#
# So the line is drawn by **cost, not by importance**. `test-fast` is what a person can afford to
# pay at every commit — seconds, locally, no network. Everything else is run deliberately, and the
# gate says so: each suite records its outcome *and the commit it ran at*, so `make ci-check` lists
# a suite nobody ran here as `NOT RUN` and one that last ran elsewhere as `STALE`. Neither can be
# mistaken for a pass. That is what makes running a subset honest instead of merely convenient.
#
# On a `prod` branch it inverts: nothing may be stale, so `make ci` — which runs all of it — is the
# price of the merge, and paying minutes once for a deliberate merge is the right trade.
#
# Measured on 2026-09-09, warm caches:
#
#     rust.unit                  0.7 s      rust.integration      2.6 s
#     python.fast                3.9 s      rust.doc              2.7 s
#                                           python.slow           minutes
#                                           python.online         somebody else's server
#                                           formats.integration  95 s
#
# **Online counts as slow.** `python.online` is not slow on this machine at all; what it is, is
# dependent on a host nobody here controls, so it can fail for a reason that has nothing to do with
# the commit being made. That is the same disqualification as costing minutes, and it earns the same
# treatment — out of the gate, recorded, current before a production commit.
#
# **The outcome is recorded, not announced.** `ci-record --suite <name>:<outcome>` writes one small
# file into `reports/`; `ci-check` reads them. A suite that says on a command line "I passed" can
# say nothing at all about the four that did not run, and that silence is exactly what the old
# arrangement — a single `--suite fast:passed` — turned into a green tick.
#
# `$(SUITE)` runs a recipe and records what happened either way, then re-raises the failure. Written
# as a function so that a suite added later cannot forget the recording half: there is one spelling
# of "run this and write down how it went", and it is here.
SUITE = @outcome=passed; $(1) || outcome=failed; \
        $(FREEPORTS_DEV) ci-record --suite $(2):$$outcome >/dev/null; \
        test "$$outcome" = passed

# What the commit hook runs, and nothing in it may cost more than seconds.
test: test-fast ## The commit gate — the crate's unit tests and the fast tooling tests

test-fast: test-rust-unit test-python ## Everything that costs seconds: what runs at every commit

test-slow: test-rust-integration test-rust-doc test-python-slow test-python-online ## The rest, run deliberately

test-all: test-fast test-slow ## Everything, slow and online included — before you call it done

# The crate's own suite, on the same two axes everything else uses. `test-rust` is all three, and
# each third is a target because each has a different cost and a different thing to say: the unit
# tests are what a keystroke-to-keystroke loop runs, the integration tests are what notices a
# regression in the whole flow, the doctests are what notices documentation that has stopped
# compiling.
test-rust: test-rust-unit test-rust-integration test-rust-doc ## The whole crate suite: unit + integration + doctests

test-rust-unit: ## Unit tests only, without the crate's integration test files — in the commit gate
	$(call SUITE,$(CARGO) test --manifest-path $(MANIFEST) --lib,rust.unit)

test-rust-integration: ## Integration tests only (packages/freeports/tests/) — run deliberately
	$(call SUITE,$(CARGO) test --manifest-path $(MANIFEST) --test '*',rust.integration)

test-rust-doc: ## Only the examples inside the doc-comments — run deliberately
	$(call SUITE,$(CARGO) test --manifest-path $(MANIFEST) --doc,rust.doc)

# A format's tests live in *its* repository, not here. FORMAT narrows the run to a single format.
test-formats: ## Test a formats repository: make test-formats REPO=<path> [FORMAT=NAME]
	@test -n "$(REPO)" || { \
	    echo "REPO is not set — for example:" >&2; \
	    echo "  make test-formats REPO=../analysis_finance_reports_formats" >&2; \
	    exit 1; }
	$(FREEPORTS_DEV) test --repo $(REPO) $(if $(FORMAT),--format $(FORMAT),)

# `freeports-validate` is a Python entry point in front of a set of shell scripts, so its suite runs
# the command as a *process*: a throwaway GPG keyring, a skeleton repository and a local HTTP server
# in a temporary directory, and no internet at any point. The programs the scripts call — gpg, yq,
# jq, check-jsonschema, curl — cannot be declared as Python dependencies, so a missing one skips the
# suite with a message naming it rather than failing it.
#
# That process-per-test architecture is also what makes most of the suite cost minutes, so it is
# split. Tests that start the command are marked `slow` — automatically, by the fixtures they ask
# for, not by hand — and deselected by default. `test-python` is therefore the fast set and is what
# the commit hook runs; `test-python-slow` is the rest and is run on request.
#
# The `network` marker keeps its own target and its own suite, `test-python-online`, and is gated
# exactly like `slow`: out of the commit gate, recorded with the commit it ran at, current before a
# production commit. Two categories, one policy — they are separate because they break for different
# reasons, and treated alike because the question "can this be paid for at every commit" has the
# same answer for both.
#
# What did change is that `network` is no longer deselected from *every* target. `test-python-all`
# used to say `-m 'not network'`, so an online test would never have run anywhere: a marker that
# excludes a test from every run is not a marker, it is a delete.
#
# Both sets have to pass before a piece of work is finished. `test-python-all` runs them together.
#
# The two packages are two invocations rather than one over both directories, because each carries
# its own `[tool.pytest.ini_options]` — the markers, the deselection that keeps the internet out of
# a normal run, and `pythonpath = ["src"]` — and pytest reads those from the rootdir it settles on,
# which over two packages at once would be neither of them. Running them together really does fail:
# `freeports_validate` is then imported from the *installed* copy, and the two tests that locate the
# package through `cli.__file__` check the venv instead of the working tree.
#
# `freeports_dev`'s own suite is fast throughout: it initialises repositories in a temporary
# directory and reads what came out, so it belongs in the commit gate whole.
# **Two pytest processes, started at once.** They have to be two — each package carries its own
# `[tool.pytest.ini_options]`, and pytest reads those from the rootdir it settles on, which over two
# packages at once would be neither of them — but they need not be one after the other. They share
# nothing: two source trees, two rootdirs, two temporary directories. Running them concurrently
# takes the fast suite from 6.5 s to 3.9 s, which is a third of the whole commit gate's budget
# recovered for nothing but a `&`.
#
# `wait` per process rather than a bare `wait`, because a bare one returns the status of the last
# job and a failure in the first package would then be reported as a pass. Both statuses are
# collected before either is acted on, so a run always prints both suites' output.
test-python: ## The tooling packages' fast Python tests (pytest) — in the commit gate
	$(call SUITE,{ $(PYTEST) $(PKG_DEV) & dev=$$!; $(PYTEST) $(PKG_VALID) & val=$$!; \
	              wait $$dev; d=$$?; wait $$val; v=$$?; test $$d -eq 0 -a $$v -eq 0; },python.fast)

# What makes these expensive is the command's own architecture, not this suite: an invocation starts
# a Python interpreter, hands over to `bash`, and each subcommand shells out to `yq`, to
# `check-jsonschema` and to `gpg` several times over. Tens of process starts per test.
# Both packages, because both have a process-starting half now. `freeports_dev`'s is the two
# fixtures that bootstrap a repository — twenty-odd process starts each — and `freeports_validate`'s
# is most of what it does.
test-python-slow: ## The tooling tests that start a process (minutes)
	$(call SUITE,{ $(PYTEST) $(PKG_DEV) -m 'slow' && \
	               $(PYTEST) $(PKG_VALID) -m 'slow and not network'; },python.slow)

# **A separate suite from `test-python-slow`, and gated identically.** Kept apart because the two
# fail for different reasons and a person fixes them differently — one is this command's process
# architecture, the other is somebody else's server being down — and a single suite would throw
# that away. Gated alike because the policy question is the same one: not at every commit, recorded
# with the commit it ran at, current before a production commit.
#
# `-m network` currently selects nothing, and pytest exits 5 for that. Read as a failure it would
# make `python.online` permanently red, which is the one reading that is certainly wrong: no online
# test has failed. It is passed instead — vacuously, the way an empty directory of tests passes —
# and the day the first one is written this needs no change.
test-python-online: ## The tooling tests that reach the real internet — run deliberately
	$(call SUITE,{ $(PYTEST) $(PKG_VALID) -m network; test $$? -eq 0 -o $$? -eq 5; },python.online)

test-python-all: test-python test-python-slow test-python-online ## Every tooling test, fast, slow and online
