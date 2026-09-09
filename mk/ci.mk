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

# ---------------------------------------------------------------------------
# The budget
# ---------------------------------------------------------------------------
# **`ci-fast` has a budget of five seconds, and it is a budget rather than an aspiration.** What is
# in it is decided by that number and by nothing else — not by how important a check is, which is a
# different question with a different answer.
#
# The argument is not that the things left out do not matter. It is that a gate is worth what it is
# *run*: the more often the tests, the coverage and the linters run, the more solid the tree, which
# is the whole case for checking at every commit instead of at every release. That case collapses
# the moment the gate costs real time — a minute of waiting turns into fewer, larger commits, then
# into `--no-verify`, and a gate nobody clears gates nothing. A cheap check that runs a hundred times
# a week beats a thorough one that runs on Fridays.
#
# What was in here on 2026-09-09, and what it cost:
#
#     lint                   1.0 s      coverage-python       24.2 s   re-runs both suites
#     test-rust-unit         0.7 s      test-rust-integration  2.6 s
#     test-python            3.9 s      test-rust-doc          2.7 s
#     doc-coverage-python    0.3 s      doc-coverage-rust      2.7 s   a second toolchain
#     ci-report              0.9 s      validation-report      5.9 s   network
#     ci-check               0.2 s      check-grants           3.2 s   network
#     ------------------------------    ---------------------------
#     kept                   7.0 s      moved out             41.3 s
#
# The right-hand column is now `make ci`, and **nothing in it is silently absent.** Each of those
# metrics is `SLOW` in `freeports_dev.ci.metrics` and each of those suites is `SLOW` in
# `freeports_dev.ci.suites`; both kinds of fact are recorded with the commit they were established
# at, so `ci-check` prints a figure from another commit as `STALE` and a suite nobody ran here as
# `NOT RUN`. On `dev` those are notifications and the commit stands. On `prod` every one of them
# refuses — a production commit is a deliberate merge, and taking the time is the point of it.
#
# So the two halves are not "the checks" and "the optional checks". They are "what this commit was
# actually checked against" and "what it was not", and the second half is written down.
#
# `ci-report` comes before `ci-check` and not after it. `ci-check` is the one step here that can
# refuse, and a run that refused before the report was written would leave the badges and the
# README describing the *previous* commit -- which is the one arrangement in which a published
# figure is wrong and nothing says so.
# **`.WAIT` is the whole reason this file is included rather than recursive.** The five measurements
# before the first barrier are independent — two linters, two suites, one docstring walk — and under
# `make -j` they run at once; the report may only be rendered once they have all landed, and the
# verdict may only be read once the report is written. One graph, one namespace, and the ordering
# expressed where the dependencies are rather than in a shell script that starts sub-makes.
#
# It takes the gate from 5.5 s to about 3.5 s, and `make ci-fast` without `-j` still runs it in
# exactly this order. `.WAIT` needs GNU Make 4.4; the empty target below is the compatibility
# spelling the GNU manual gives, under which an older make treats it as a no-op and simply runs
# everything in sequence.
.WAIT:

ci-fast: lint test-fast doc-coverage-python .WAIT ci-report .WAIT ci-check ## The commit gate: lint, the fast suites, the fast measurements, the report, the verdict

# Kept as a name people already type, and as the name `.githooks/pre-commit` calls.
pre-commit: ci-fast ## The commit gate (alias of ci-fast)

# The other commit gate: what a commit to a **prod** branch has to clear.
#
# Same three layers, same report, same verdict — only the set of measurements differs, and it
# differs by exactly the fast/slow line drawn above. Everything gated is established here, at this
# commit: both suites in full, the instrumented coverage, the second toolchain's documentation
# figure, and the grants re-resolved over the network. `ci-check` then has nothing to call stale,
# which is the point: on a production branch nothing about the commit may be unknown.
#
# It is minutes rather than seconds, and that is the trade. A commit to `main` is a deliberate act —
# usually a merge — made a few times a week; paying for certainty then, and not at every keystroke
# of a working day, is the whole shape of this arrangement.
#
# `.WAIT` for the same reason `ci-fast` uses it: under `-j` the measurements overlap, the report
# waits for all of them, and the verdict waits for the report.
ci-full: lint test-all coverage doc-coverage validation .WAIT ci-report .WAIT ci-check ## The prod commit gate: everything gated, measured at this commit

# What a pipeline would run: the prod gate, plus the documentation build, which is gated on nothing
# but has to compile.
ci: ci-full ci-docs ## Everything runnable locally — what a pipeline would run

ci-docs: docs docs-site-coverage ## The documentation build, and what the site covers

ci-rust: lint-rust test-rust coverage-rust doc-coverage-rust ## The whole Rust column

ci-python: lint-python test-python-all coverage-python doc-coverage-python ## The whole Python column

# Reads whatever is already in `reports/` and judges it. Measures nothing itself, which is why it
# is instant and why it can be run repeatedly while fixing something.
#
# No `--suite` argument any more, and its removal is the point of this work. It used to say
# `--suite fast:passed` — a claim made by whoever typed the command, about a run that may never have
# happened, and silent about every suite that had not run. Each suite now records its own outcome
# and the commit it ran at, so this reads them all: what ran here, what ran somewhere else, and what
# nobody has run at all.
ci-check: ## The verdict table over whatever is already in reports/
	$(FREEPORTS_DEV) ci-check

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

# **One process, six renderings, no temporary file.** This used to build the model into `mktemp` and
# start this command six more times with `--model`; the file existed so that six evaluations could
# not disagree with one another. `--render` gets the same guarantee from one evaluation, and gives
# back two thirds of a second — six interpreter starts out of a commit gate budgeted at five
# seconds. The `mktemp` and its `trap` go with them.
ci-report: ## Refresh the CI badges, the README block, the doc pages and the HTML
	@mkdir -p $(dir $(CI_REPORT_HTML))
	@$(CI_REPORT) \
	     --render badges:$(CI_BADGES_DIR)/ \
	     --render markdown:README.md \
	     --render rst:$(CI_REPORT_DIR)/index.rst \
	     $(foreach table,$(CI_REPORT_TABLES),--render rst@$(table):$(CI_REPORT_DIR)/$(table).rst) \
	     --render html:$(CI_REPORT_HTML) >/dev/null || { \
	         echo "freeports-dev: the measurements could not be read - nothing was rewritten" >&2; \
	         exit 1; }

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
