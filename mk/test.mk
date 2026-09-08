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

# What the commit hook runs. The crate's suite in full — its integration tests are the ones that
# would notice a regression in the whole flow, and that is where you want to notice it rather than
# after the push — plus the *fast* half of the tooling suite.
#
# The slow half is `test-python-slow`, and leaving it out here is deliberate rather than a
# concession: it is minutes of process starts, and a gate that costs minutes is a gate that gets
# disabled. Run `test-all` before calling a piece of work finished.
test: test-rust test-python ## The commit gate — the crate, and the fast tooling tests

test-all: test-rust test-python-all ## Everything, slow tests included — before you call it done

test-rust: ## The whole crate suite: unit + integration + doctests
	$(CARGO) test --manifest-path $(MANIFEST)

test-rust-unit: ## Unit tests only, without the crate's integration test files
	$(CARGO) test --manifest-path $(MANIFEST) --lib

test-rust-integration: ## Integration tests only (packages/freeports/tests/)
	$(CARGO) test --manifest-path $(MANIFEST) --test '*'

test-rust-doc: ## Only the examples inside the doc-comments
	$(CARGO) test --manifest-path $(MANIFEST) --doc

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
# the commit hook runs; `test-python-slow` is the rest and is run on request. A commit gate that
# costs seven minutes is a commit gate people turn off, and then it gates nothing.
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
test-python: ## The tooling packages' fast Python tests (pytest) — the commit gate
	$(PYTEST) $(PKG_DEV)
	$(PYTEST) $(PKG_VALID)

test-python-slow: ## The tooling tests that start the command as a process (minutes)
	$(PYTEST) $(PKG_VALID) -m 'slow and not network'

test-python-all: ## Every tooling test, fast and slow, except the ones needing the internet
	$(PYTEST) $(PKG_DEV)
	$(PYTEST) $(PKG_VALID) -m 'not network'
