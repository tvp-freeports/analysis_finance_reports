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
#
# `ci-report` comes before `ci-check` and not after it. `ci-check` is the one step here that can
# refuse, and a run that refused before the report was written would leave the badges and the
# README describing the *previous* commit -- which is the one arrangement in which a published
# figure is wrong and nothing says so.
ci-fast: lint test coverage-python doc-coverage validation-report check-grants ci-report ci-check ## The commit gate: lint, the fast suites, the fast measurements, the report, the verdict

# Kept as a name people already type, and as the name `.githooks/pre-commit` calls.
pre-commit: ci-fast ## The commit gate (alias of ci-fast)

ci: ci-fast coverage-rust check-keys docs docs-site-coverage ## Everything runnable locally — what a pipeline would run

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


# ---------------------------------------------------------------------------
# The report
# ---------------------------------------------------------------------------
# The verdict, written down where somebody who is not running `make` will meet it.
#
# `ci-check` prints a table on a terminal and nothing keeps it. This is the same run rendered into
# the four shapes the grants report already has — badges, a block in `README.md`, pages in the
# documentation, one HTML page — and it is the same arrangement, deliberately: `validation-report`
# in `mk/validation.mk` is its twin, and a reader who has found one has found the other.
#
# **One evaluation, rendered several times.** `ci-report --format json` writes the model and every
# other rendering is `--model` against it, so the badges cannot disagree with the README about the
# same run. Evaluating six times would be cheap here — it reads JSON files, it does not measure —
# but two renderings of a repository that disagree are not a cost anybody notices until they do.
#
# **Nothing here can refuse a commit**, on any branch, exactly like the validation report. What may
# refuse is `ci-check`, below, and it is the last thing `ci-fast` runs for that reason.
CI_BADGES_DIR = ci/report/badges
CI_REPORT_DIR = docs/source/dev/ci-report
# Generated into `_extra/`, which `html_extra_path` copies verbatim into the root of the site — the
# same arrangement as rustdoc and the validation coverage page. It is written here and not by the
# documentation builder because `reports/` is gitignored: a builder has no measurements at all, and
# a page generated there would be a page of dashes.
CI_REPORT_HTML = docs/source/_extra/ci/report.html
CI_REPORT_TABLES = thresholds breakdown

CI_REPORT = $(FREEPORTS_DEV) ci-report --repo "$(CURDIR)"

ci-report: ## Refresh the CI badges, the README block, the doc pages and the HTML
	@model=$$(mktemp) || exit 1; \
	 trap 'rm -f "$$model"' EXIT; \
	 $(CI_REPORT) --format json --out "$$model" >/dev/null || { \
	     echo "freeports-dev: the measurements could not be read - nothing was rewritten" >&2; \
	     exit 1; }; \
	 $(CI_REPORT) --model "$$model" --format badges   --out $(CI_BADGES_DIR)/ >/dev/null; \
	 $(CI_REPORT) --model "$$model" --format markdown --out README.md >/dev/null; \
	 $(CI_REPORT) --model "$$model" --format rst      --out $(CI_REPORT_DIR)/index.rst >/dev/null; \
	 for table in $(CI_REPORT_TABLES); do \
	     $(CI_REPORT) --model "$$model" --format rst --table "$$table" \
	         --out "$(CI_REPORT_DIR)/$$table.rst" >/dev/null || exit 1; \
	 done; \
	 mkdir -p $(dir $(CI_REPORT_HTML)); \
	 $(CI_REPORT) --model "$$model" --format html --out $(CI_REPORT_HTML).new >/dev/null \
	     && mv -f $(CI_REPORT_HTML).new $(CI_REPORT_HTML) \
	     || { rm -f $(CI_REPORT_HTML).new; exit 1; }

ci-report-json: ## The model every other rendering is drawn from, on standard output
	@$(CI_REPORT) --format json

ci-report-badges: ## Only the badges
	$(CI_REPORT) --format badges --out $(CI_BADGES_DIR)/

ci-report-readme: ## Only the block between the markers in README.md
	$(CI_REPORT) --format markdown --out README.md

ci-report-docs: ## Only the pages under docs/source/dev/ci-report/
	@$(CI_REPORT) --format rst --out $(CI_REPORT_DIR)/index.rst
	@for table in $(CI_REPORT_TABLES); do \
	     $(CI_REPORT) --format rst --table "$$table" --out "$(CI_REPORT_DIR)/$$table.rst" \
	         || exit 1; \
	 done

ci-report-html: ## Only the single-page HTML rendering
	@mkdir -p $(dir $(CI_REPORT_HTML))
	$(CI_REPORT) --format html --out $(CI_REPORT_HTML)
