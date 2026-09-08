##@ Continuous integration

# What a pipeline would run, runnable here. There is no Jenkins at the moment, and this is the
# answer to "what did it do": `make ci`. When it comes back it adds two things and no more —
# publishing to PyPI on a tagged release, and building the documentation — because everything else
# it would do already has a name here.
#
# **The gate is a name, not a list.** `pre-commit` is `ci-fast`, and `ci-fast` is a list of
# prerequisites in this file. That is what lets the set of checks grow without `.githooks/pre-commit`
# ever being edited again.
#
# Three layers, and nothing in one layer knows the next:
#
#   measurement   each recipe in `mk/quality.mk` answers one question and writes one JSON file
#                 into `reports/`. None of them decides anything.
#   the gate      `ci-check` reads those files, `ci.yaml`, and the class of the current branch,
#                 prints one table and chooses an exit status.
#   the wiring    these aggregates, and a thin hook in each repository.
#
# The split is the one `freeports-validate` already follows — `collect` establishes facts, `report`
# renders them, `check-grants` judges them — and it is what keeps every figure reproducible by
# hand, which is the only way a threshold survives its first failure.

# What the commit hook runs, and nothing in it may cost more than seconds. If `ci-fast` ever costs
# more than the suite it wraps, the fast/slow line has moved and the measurement that moved it
# belongs in `ci` instead.
#
# `coverage-rust` is absent on purpose: it recompiles the crate instrumented. `ci-check` reads that
# figure from the last `make ci` and, on a prod branch, refuses when it was taken at another commit.
ci-fast: lint test coverage-python doc-coverage validation-report check-grants ci-check ## The commit gate: lint, the fast suites, the fast measurements, the verdict

# Kept as a name people already type, and as the name `.githooks/pre-commit` calls.
pre-commit: ci-fast ## The commit gate (alias of ci-fast)

ci: ci-fast coverage-rust docs docs-site-coverage ## Everything runnable locally — what a pipeline would run

ci-rust: lint-rust test-rust coverage-rust doc-coverage-rust ## The whole Rust column

ci-python: lint-python test-python coverage-python doc-coverage-python ## The whole Python column

# Reads whatever is already in `reports/` and judges it. Measures nothing itself, which is why it
# is instant and why it can be run repeatedly while fixing something.
ci-check: ## The verdict table over whatever is already in reports/
	$(FREEPORTS_DEV) ci-check --suite fast:passed

branch-class: ## Which class this branch is in, and the rule that put it there
	$(FREEPORTS_DEV) branch-class

# Refused unless HEAD carries a release tag and the branch is a prod one. Publishing is the one
# action here that cannot be undone, so it is the one that checks twice.
release: dist ## Upload the distributions to PyPI (tagged release, prod branch)
	@git describe --exact-match --tags HEAD >/dev/null 2>&1 || { \
	    echo "HEAD is not a release tag. Tag the release first." >&2; exit 1; }
	@test "$$($(FREEPORTS_DEV) branch-class --format json | $(PY) -c \
	    'import json,sys; print(json.load(sys.stdin)["class"])')" = "prod" || { \
	    echo "Not on a prod branch. Releasing from a dev branch is how a pre-release ships." >&2; \
	    exit 1; }
	$(PYTHON) -m twine upload $(DISTDIR)/*
