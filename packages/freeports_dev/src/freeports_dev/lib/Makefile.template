# Makefile of a freeports **format repository**: the single entry point.
#
# This repository holds format plugins — the YAML, the Python under `content/`, the reference
# fixtures under `tests/formats/` — and the grants that say who vouched for which of them. It has
# no crate, no packages and no site to build, so everything here is one invocation of `freeports-dev`
# or `freeports-validate` that you could equally type by hand. That is deliberate: the documentation
# quotes these recipes in full, and two formulations of the same thing always diverge.
#
#     make init        first time, from a fresh clone: point git at .githooks/
#     make test        the per-page suite — the loop format development runs in
#     make ci-fast     what the commit hook runs on a dev branch
#     make ci-full     what it runs on a prod branch: everything, measured here
#     make doctor      what is installed, what is missing, and what supplies it
#
# `make help` lists the rest and is generated from the `##` comments below, so a new target
# documents itself and a removed one disappears from the help.
#
# On Windows the same commands are typed `make.bat <target>` from `cmd` or PowerShell, or `make
# <target>` from Git Bash. `make.bat` is a shim of a dozen lines that starts a POSIX shell and calls
# this file: there is one build system here, not one per platform.
#
# ---------------------------------------------------------------------------
# Two command surfaces, one vocabulary
# ---------------------------------------------------------------------------
# **`make` changes the repository you are standing in. `freeports-dev` interrogates a repository,
# including one that is not yours.** That is the whole division, and every name below follows it.
#
# So `make coverage` measures *this* repository and writes the figure into `reports/`, while
# `freeports-dev coverage --repo ../somebody-elses-formats` prints theirs and touches nothing. A
# person who has learnt the engine's `Makefile` can work here without reading anything; a person
# who has learnt `freeports-dev` can answer questions about a repository they have just cloned.
#
#     make test-fast          freeports-dev test --fast
#     make test-slow          freeports-dev test --slow
#     make lint               freeports-dev lint-score
#     make coverage           freeports-dev coverage
#     make doc-coverage       freeports-dev doc-coverage
#     make fingerprint        freeports-dev fingerprint
#     make branch-class       freeports-dev branch-class
#     make ci-report          freeports-dev ci-report
#     make ci-check           freeports-dev ci-check
#     make validation-report  freeports-validate report
#     make check-grants       freeports-validate check-grants
#     make check-keys         freeports-validate check-keys
#
# The renderings of the report have **no** target of their own — `make ci-report` writes all of
# them at once. Asking for one shape is a question rather than a change, and questions are
# `freeports-dev ci-report --format badges --out …`, which works against any repository.
#
# ---------------------------------------------------------------------------
# Why there are fewer targets here than in the engine
# ---------------------------------------------------------------------------
# The engine's surface is laid out on two axes — `test`/`lint`/`coverage` × `rust`/`python` — so
# that `test-rust-unit` and `lint-python` exist without anybody having to look them up. **A format
# repository has no second axis.** There is one test command, and its two halves are told apart by
# a marker rather than by a language. There is nothing to install, nothing to compile and no site
# to build. What is left fits in one file, which is why there is no `mk/` directory here.
#
# The names that do exist are the engine's names, with the same meanings, so that the two
# repositories can be worked on in the same afternoon.
#
# ---------------------------------------------------------------------------
# What may refuse a commit, and what may only report
# ---------------------------------------------------------------------------
# **`ci-check` is the verdict, and with `fingerprint` it is the only thing here that refuses.**
# Everything else measures, records and renders. That is not a detail of this file, it is the
# arrangement the whole gate rests on: a suite writes its outcome into `reports/` with the commit
# it ran at, and `ci-check` — reading those files, `ci.yaml`, and the class of the branch — decides.
# A recipe that refused on its own would be making that decision twice, in a place that cannot see
# which branch you are on.
#
# Which is why the suites and the grant checks behave differently depending on how they were
# reached. `ci-fast` and `ci-full` set the target-specific variable `GATE`, and GNU make passes a
# target-specific variable down to every prerequisite; under it a failing suite is recorded and the
# run continues to the verdict. Typed on their own — `make test-fast`, `make check-grants` — they
# fail, because a command that answers a question must answer it in its exit status.
#
# The effect is exactly what `ci.yaml` says it should be: on a `dev` branch nothing refuses a
# commit, on a `prod` branch a failing suite, a missed threshold, a figure nobody could measure or a
# fingerprint that moved without its version all do.

.DEFAULT_GOAL := help

# Output kept whole per target, so that `make -j` stays readable: without it two recipes running at
# once interleave line by line and a failure is a jigsaw. It costs nothing on a sequential run,
# which is why it is set here rather than passed by whoever remembers to. `.githooks/pre-commit`
# asks for `-j` itself, where it applies.
MAKEFLAGS += -Otarget

# ---------------------------------------------------------------------------
# The commands
# ---------------------------------------------------------------------------
# Taken from `PATH`, and not from a virtualenv this file guesses at. A format repository installs
# nothing: the tools come from the `freeports-dev` environment created in the engine repository, and
# the hook refuses to run outside it. Naming them as variables is what lets a checkout be tested
# against a build of the engine that is not the installed one:
#
#     make test FREEPORTS_DEV=../analysis_finance_reports/venv/freeports-dev/bin/freeports-dev
FREEPORTS_DEV      ?= freeports-dev
FREEPORTS_VALIDATE ?= freeports-validate
RUFF               ?= ruff

# The repository being acted on, named outright rather than inherited.
#
# Left to resolve it themselves, both commands consult their own tiers — `--repo`, then
# `FREEPORTS_FORMATS_REPO_PATH`, then `formats_repo` in whatever configuration file is found from
# here — and any of those can name a *different* repository than the one you are standing in. A
# relative value carried in from another directory is the usual way it happens, and it fails
# confusingly: the path comes out doubled, and the command says the repository is not one.
REPO     = --repo "$(CURDIR)"
VALIDATE = $(FREEPORTS_VALIDATE) $(REPO) $(VALIDATE_SOURCE_ARGS)

# Where the measurements land. Gitignored here and in every other freeports repository: these are
# facts about one commit on one machine, not something to carry in the history.
REPORTS  = reports

# Narrows a run to one format: `make test FORMAT=EURIZON-EN23`. Empty means all of them.
FORMAT ?=
FORMAT_ARG = $(if $(FORMAT),--format $(FORMAT),)

# What ruff is given. `content/` is the Python of this repository; `tests/formats/` is generated
# fixture data rather than code somebody wrote, and linting it would score this repository on the
# output of `make-tests`.
LINT_PATHS ?= content

# Where the methodology pages are fetched from — **empty, and that is the setting.**
#
# The published `stable` channel is `freeports-validate`'s own default, and it is the right one:
# `stable` is the last release, it is what a reader of this repository can fetch, and a grant is a
# claim addressed to that reader. So this file names no source at all and lets the command's three
# tiers decide.
#
# **Naming one here would take the decision away from you**, and silently: a `--source` on the
# command line outranks a configuration file, so a value written down in this Makefile would
# override the `validate.sources` you set in your own `freeports-conf.yaml` — the one tier where a
# personal choice belongs. That is the whole reason this is empty rather than helpful.
#
# Which matters when your grants were made under `latest`, the channel built from the engine's
# working branch: `latest` and `stable` do not carry the same text, so a grant hashed against one is
# reported as a mismatch when resolved from the other. That is a fact about which publication you
# are reading, not about this repository, and the answer is to say so where your other settings
# live:
#
#     validate:                                    # in your freeports-conf.yaml
#       sources:
#         - https://docs.freeports.org/en/latest/_sources/validation/*.rst.txt
#         - https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt
#
# `freeports-validate sources` prints the list in force and what each name resolves to; run it when
# a grant is reported as unresolvable. For a one-off, this variable is still here:
#
#     make validation-report VALIDATE_SOURCES="https://docs.example.org/_sources/validation/*.rst.txt"
#
# and because the Makefile runs in this directory, the configuration file it finds is the one *here*
# — `--config` or `FREEPORTS_CONFIG_FILE` names one kept elsewhere, such as a workspace root's.
VALIDATE_SOURCES ?=
VALIDATE_SOURCE_ARGS = $(foreach source,$(VALIDATE_SOURCES),--source "$(source)")

# The committed artefacts of the two reports. Named here because the hook stages exactly these
# after a run rewrites them, and a list that lived in two files would come apart.
# **A page is rewritten only when it asks to be.** Both reports write *between two markers* rather
# than over a whole file, so a page that carries no marker pair is not a page this can fill: it is
# either somebody's own prose, or one whose markers were deliberately taken out. Testing for the
# file and not for the markers is how a run ends with "the measurements could not be read" and
# rewrites none of the other eight renderings, because one file in the list was not what it looked
# like.
CI_MARKER    = <!-- freeports-dev:begin -->
GRANT_MARKER = <!-- freeports-validate:begin -->

CI_BADGES_DIR     = ci/report/badges
CI_REPORT_HTML    = ci/report/index.html
CI_REPORT_TABLES  = thresholds breakdown

GRANT_BADGES_DIR  = validation/report/badges
GRANT_REPORT_DIR  = validation/report
GRANT_TABLES      = file-contributor file-methodology contributor-methodology \
                    contributor-file methodology-file methodology-contributor

.PHONY: help doctor init githooks \
        check test test-fast test-slow test-all \
        lint fmt fmt-check coverage doc-coverage \
        validation validation-report check-grants check-keys \
        ci ci-fast ci-full pre-commit ci-report ci-check branch-class fingerprint \
        clean

##@ Help

help: ## List the available targets
	@awk 'BEGIN {FS = ":.*##"} \
	     /^##@/ { printf "\n%s\n", substr($$0, 5); next } \
	     /^[a-zA-Z0-9_.-]+:.*##/ { printf "  %-18s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@echo
	@echo "Repository: $(CURDIR)"
	@echo "Details and diagnosis: make doctor"

# What is installed, what is missing, and what supplies it. The counterpart of the engine's target
# of the same name, and the first thing to run when a recipe here fails for a reason that has
# nothing to do with the formats in this repository.
doctor: ## Diagnosis: what is installed, what is missing, and where it comes from
	@echo "Environment"
	@if [ -n "$$VIRTUAL_ENV" ]; then echo "  virtualenv     $$VIRTUAL_ENV"; \
	 elif [ -n "$$CONDA_DEFAULT_ENV" ]; then echo "  conda          $$CONDA_DEFAULT_ENV"; \
	 else echo "  none active    ->  source <engine>/venv/freeports-dev/bin/activate"; fi
	@echo
	@echo "Commands this file calls"
	@for cmd in $(FREEPORTS_DEV) $(FREEPORTS_VALIDATE) freeports $(RUFF); do \
	    if command -v "$$cmd" >/dev/null 2>&1; then \
	        echo "  $$cmd: $$(command -v $$cmd)"; \
	    else \
	        echo "  $$cmd: MISSING  ->  install the engine's tooling: make dev-formats there"; \
	    fi; \
	 done
	@echo
	@echo "System programs freeports-validate shells out to"
	@for cmd in gpg jq sha256sum realpath; do \
	    if command -v "$$cmd" >/dev/null 2>&1; then \
	        echo "  $$cmd: $$(command -v $$cmd)"; \
	    else \
	        echo "  $$cmd: MISSING  ->  install it from your system package manager"; \
	    fi; \
	 done
	@echo
	@echo "This repository"
	@echo "  formats        $$(ls tests/formats 2>/dev/null | wc -l)"
	@echo "  branch class   $$($(FREEPORTS_DEV) branch-class $(REPO) 2>/dev/null | head -1 || echo unknown)"
	@if [ ! -d "$(REPORTS)" ]; then \
	    echo "  reports/       nothing measured yet  ->  make ci-fast"; \
	 else \
	    echo "  reports/       $$(ls $(REPORTS) 2>/dev/null | wc -l) measurements  ->  make ci-check"; \
	 fi

##@ Setup

init: githooks ## First time, from a fresh clone: everything a checkout needs
	@echo
	@echo "Ready. The tools come from the engine's environment:"
	@echo "  source <engine>/venv/freeports-dev/bin/activate"
	@echo "Then: make test"

githooks: ## Point git at .githooks/, so the commit gate runs
	git config --local core.hooksPath "$(CURDIR)/.githooks"
	@echo "core.hooksPath = .githooks — the commit gate is active."

##@ Tests

# One suite, two halves, told apart by a marker rather than by a language.
#
# **Per-page tests are the loop format development runs in.** They pin one page of one document at a
# time, so a failure says *which segment* broke rather than that the document did, and they are
# cheap enough to pay for at every commit. The whole-document tests are a full extraction run each —
# minutes across a repository of any size — so they run when you ask for them, and at every commit
# to a production branch.
#
# The line between them is drawn by **cost, not by importance**. A gate that costs a minute is a
# gate people commit around, in bigger and rarer commits, and then `--no-verify`, and then it gates
# nothing. What is left out is not left silent: each suite records its outcome *and the commit it
# ran at*, and `ci-check` prints a suite nobody ran here as `NOT RUN` and one that last ran elsewhere
# as `STALE`. Neither reads as a pass.
#
# `$(SUITE)` runs a recipe and records what happened either way. Written as a function so that a
# suite added later cannot forget the recording half: there is one spelling of "run this and write
# down how it went", and it is here.
#
# The last line is where `GATE` does its work. Reached from `ci-fast` or `ci-full` it is set, and
# `test -n` makes the recipe succeed however the suite went — the outcome is in `reports/` and the
# verdict is `ci-check`'s, which is the only step that knows whether this branch refuses. Typed on
# its own it is empty, and the recipe fails exactly when the suite did.
SUITE = @outcome=passed; $(1) || outcome=failed; \
        $(FREEPORTS_DEV) ci-record $(REPO) --suite $(2):$$outcome >/dev/null; \
        test -n "$(GATE)" || test "$$outcome" = passed

check: test-all ## Run every test (canonical GNU name, alias of test-all)

test: test-fast ## The per-page suite — the everyday one, and what the commit gate runs

test-fast: ## The per-page suite: one page of one document at a time (seconds)
	$(call SUITE,$(FREEPORTS_DEV) test $(REPO) $(FORMAT_ARG) --fast,formats.single_page)

test-slow: ## The whole-document suite: a full extraction run per document (minutes)
	$(call SUITE,$(FREEPORTS_DEV) test $(REPO) $(FORMAT_ARG) --slow,formats.integration)

test-all: test-fast test-slow ## Both suites — before you call a format done

##@ Quality

# Two runs of ruff, and the second is not waste. The first prints the diagnostics, which is what a
# person typing `make lint` came for; the second is `lint-score`, which parses the machine-readable
# form and records the score for `ci-check` to judge. Ruff is sub-second over a repository this size.
#
# **The `-` is the point of this recipe.** `content/` is a working codebase with violations in it,
# and `ci.yaml` gates the *score* — a ratchet that lets today's commits through and stops tomorrow's
# making it worse. A recipe that failed on any violation would refuse every commit until somebody
# fixed all of them at once, which is how a gate gets switched off. Here the linter reports and the
# score gates. (The engine, whose tree is clean, does the opposite and treats a violation as a
# failure; both follow from the state of the code, not from a difference of opinion.)
lint: ## ruff over content/, and the score recorded for the gate
	-$(RUFF) check $(LINT_PATHS)
	@$(FREEPORTS_DEV) lint-score $(REPO) --out --format none

fmt: ## Reformat content/ with ruff
	$(RUFF) format $(LINT_PATHS)

fmt-check: ## Verify the formatting without rewriting anything
	$(RUFF) format --check $(LINT_PATHS)

# **The document inventory, and it is what `coverage` means in a format repository.** There is no
# line coverage here: the code under test is the engine, and what this repository can be measured on
# is how much of what it claims to parse it actually pins — `tests.formats.single_page` counts the
# documents carrying a whole fixture triple, `tests.formats.integration` those with a whole-document
# test at all. Both are counted by **document**, not by format: a format with one `report.pdf` is one
# document, and a format with four is four.
coverage: ## How much of this repository is tested, by document
	$(FREEPORTS_DEV) coverage $(REPO) --out --format none

doc-coverage: ## Public Python objects under content/ carrying a docstring
	@$(FREEPORTS_DEV) doc-coverage $(REPO) --out --format none

##@ Validation

# What the grants in `validation/` claim, and whether the claim still holds.
#
# **Everything here reaches the network** — resolving a methodology page from wherever this machine
# is configured to fetch it, looking a granter's key up on a key server. That is why none of it is in
# the dev commit gate: six seconds of somebody else's server at every commit is the single most
# expensive thing a hook could do, for a reason that has nothing to do with the change being made,
# and a commit has to be possible on a train.
#
# **And why none of it refuses, on any branch.** A granted reference output regenerated by
# `make-tests` is ordinary work, and a contributor who does not hold the granting key could not clear
# such a gate at all. What may refuse is the `grants.coverage` *threshold*, which is `ci-check`'s
# decision and only on a `prod` branch — where `ci-full` has just re-established the figure, so there
# is nothing left to be stale.
validation: validation-report check-grants check-keys ## Everything the grants say, re-established over the network

# **One walk of the repository, rendered nine times.** `collect` resolves every methodology page from
# the configured sources; collecting once per artefact would fetch each page nine times over for an
# answer that cannot have changed in between. So the model is written once and every rendering is
# `--model` against it.
#
# **A walk that resolved nothing is not a walk whose answer may be published.** `collect` exits 0
# whether or not the pages came back — establishing what it could establish is its whole job — but
# every rendering below is *committed*, so writing one out of a walk that reached nothing would
# replace a report saying "coverage 100 %, check-grants passing" with one saying "coverage --,
# failing". That difference is a fact about somebody's web server, not about this repository. It is
# the rule this project already holds for a page it could not reach: unverifiable is not granted,
# and it must not be published as ungranted either.
#
# So the coverage figure is recorded first and read back. `unmeasured` means nothing resolved, and
# then nothing is rewritten and the reason is printed loudly. The metric stays unmeasured, `ci-check`
# reports it as such, and on a production branch it refuses — which is the correct outcome: a
# production commit whose grants nobody could check is not one that may be made.
validation-report: ## Refresh the badges, the README block and the six lookup pages
	@command -v $(FREEPORTS_VALIDATE) >/dev/null 2>&1 || { \
	    echo "freeports-validate is not installed, so the grant report was not refreshed." >&2; \
	    echo "grants.coverage stays unmeasured, and \`make ci-check\` reports it as such." >&2; \
	    exit 0; }
	@model=$$(mktemp) || exit 1; \
	 trap 'rm -f "$$model"' EXIT; \
	 if ! $(VALIDATE) collect > "$$model" 2>/dev/null || [ ! -s "$$model" ]; then \
	     echo "freeports-validate: the grants could not be read — nothing was rewritten." >&2; \
	     exit 0; \
	 fi; \
	 $(FREEPORTS_DEV) ci-record $(REPO) --metric grants.coverage \
	     --from validate --input "$$model" >/dev/null 2>&1; \
	 if grep -q '"state": *"unmeasured"' "$(REPORTS)/grants-coverage.json" 2>/dev/null; then \
	     echo "freeports-validate: no methodology page could be resolved, so the report is" >&2; \
	     echo "unchanged. It is not being rewritten to say the grants failed: that would be a" >&2; \
	     echo "claim about the network. \`freeports-validate sources\` says where pages are" >&2; \
	     echo "fetched from and what each name resolves to." >&2; \
	     exit 0; \
	 fi; \
	 $(VALIDATE) report --model "$$model" --format badges --out $(GRANT_BADGES_DIR)/; \
	 if grep -q "$(GRANT_MARKER)" README.md 2>/dev/null; then \
	     $(VALIDATE) report --model "$$model" --format markdown --out README.md; \
	 fi; \
	 for table in $(GRANT_TABLES); do \
	     page="$(GRANT_REPORT_DIR)/$$table.md"; \
	     grep -q "$(GRANT_MARKER)" "$$page" 2>/dev/null || continue; \
	     $(VALIDATE) report --model "$$model" --format markdown --table "$$table" \
	         --out "$$page" || exit 1; \
	 done; \
	 echo "freeports-validate: report refreshed"

# The integrity check itself: signatures verify, hashes still match the files, methodology pages
# still say what they said. Rendering what `validation/` claims is not the same as checking that the
# claim holds, which is why this is its own target and its own step.
#
# A page that could not be fetched also comes back non-zero, and deliberately so: a grant nobody
# could verify is one this run does not vouch for. `check-grants` tells that from a real failure in
# its own output and in its exit status.
check-grants: ## Verify every claim made in validation/
	@$(VALIDATE) check-grants || { \
	    echo "" >&2; \
	    echo "freeports-validate: the grants above are not all vouched for by this run." >&2; \
	    echo "Re-grant with \`freeports-validate grant\`, or restate a changed one with" >&2; \
	    echo "\`freeports-validate update\`, once you know which it is." >&2; \
	    test -n "$(GATE)"; }

# Whether the granters' keys are published where a stranger can fetch them.
#
# A signature checks out against a keyring, which answers "was this signed by the key it names". It
# does not answer "can anybody else get that key" — and if they cannot, the signature is checkable
# only by people who already have it, which is everybody except the person a published grant exists
# to convince.
#
# A key that could not be looked up makes the metric *unmeasured* rather than lowering it: reporting
# "I could not reach the server" as a number would turn a fact about the network into a fact about
# this repository.
check-keys: ## Whether the granters' keys are published on the configured key server
	@keys=$$(mktemp) || exit 1; \
	 trap 'rm -f "$$keys"' EXIT; \
	 $(VALIDATE) check-keys | tee "$$keys"; \
	 $(FREEPORTS_DEV) ci-record $(REPO) --metric grants.keys_online \
	     --from check-keys --input "$$keys" >/dev/null

##@ Continuous integration

# **The gate is a name, not a list.** `.githooks/pre-commit` runs `make pre-commit` on a dev branch
# and `make ci-full` on a prod one, and what either consists of is decided here — so the set of
# checks can grow without the hook ever being edited again.
#
# Three layers, and nothing in one layer knows the next:
#
#   measurement   each target above answers one question and writes one JSON file into `reports/`.
#                 None of them decides anything.
#   the gate      `ci-check` reads those files, `ci.yaml`, and the class of the current branch,
#                 prints one table and chooses an exit status.
#   the wiring    these two aggregates, and a thin hook.
#
# **`.WAIT` is what orders them.** The measurements are independent and under `make -j` run at once;
# the report may only be rendered once they have all landed; the fingerprint rule may only be applied
# once the report is written, because it is the first step that can refuse and a run refused before
# the report was written would leave the badges describing the previous commit; and the verdict comes
# last of all. `.WAIT` needs GNU Make 4.4 — the empty target below is the compatibility spelling the
# GNU manual gives, under which an older make treats it as a no-op and runs everything in sequence.
.WAIT:

# `GATE` is set on both aggregates and inherited by every prerequisite: under it a failing suite or
# an unvouched-for grant is recorded and reported rather than raised, and the decision is left to
# `ci-check`, which is the only step that knows which branch this is. See the essay at the top.
ci-fast: GATE := 1
ci-fast: lint test-fast coverage doc-coverage .WAIT ci-report .WAIT fingerprint .WAIT ci-check ## The dev commit gate: the fast half, the report, the verdict

pre-commit: ci-fast ## The commit gate (alias of ci-fast, and the name the hook calls)

# The other commit gate: what a commit to a **prod** branch has to clear.
#
# Same three layers, same report, same verdict — only the set of measurements differs, and it differs
# by exactly the fast/slow line drawn above. Everything gated is established *here, at this commit*:
# both suites, and the grants re-resolved over the network. `ci-check` then has nothing to call
# stale, which is the point — on a production branch nothing about the commit may be unknown.
#
# It is minutes rather than seconds, and that is the trade. A commit to `main` is a deliberate act,
# usually a merge, made a few times a week; paying for certainty then, and not at every keystroke of
# a working afternoon, is the whole shape of this arrangement.
ci-full: GATE := 1
ci-full: lint test-all coverage doc-coverage validation .WAIT ci-report .WAIT fingerprint .WAIT ci-check ## The prod commit gate: everything gated, measured at this commit

ci: ci-full ## Everything runnable here — what a pipeline would run

# **One process, every rendering.** `--render` is repeatable and each rendering is drawn from a
# single evaluation: five invocations would be five interpreter starts, and five evaluations that
# could disagree with one another about the same run.
#
# The list is built rather than passed blind, because **only what is actually there is rewritten**. A
# page somebody deleted, or one whose marker pair they removed, is left alone rather than recreated
# behind their back — and `--render markdown:` against a file that is not there is an error, which
# would take the other renderings down with it.
#
# Nothing here can refuse a commit, on any branch. A report one run out of date is a smaller problem
# than a commit that cannot be made, and a report is not a gate.
ci-report: ## Refresh the CI badges, the README block, the two pages and the HTML
	@renderings="--render badges:$(CI_BADGES_DIR)/ --render html:$(CI_REPORT_HTML)"; \
	 for page in README.md $(foreach table,$(CI_REPORT_TABLES),ci/report/$(table).md); do \
	     grep -q "$(CI_MARKER)" "$$page" 2>/dev/null || continue; \
	     case "$$page" in \
	         README.md) renderings="$$renderings --render markdown:$$page" ;; \
	         *) renderings="$$renderings --render markdown@$$(basename "$$page" .md):$$page" ;; \
	     esac; \
	 done; \
	 $(FREEPORTS_DEV) ci-report $(REPO) $$renderings >/dev/null || { \
	     echo "freeports-dev: the measurements could not be read — nothing was rewritten" >&2; \
	     exit 0; }

# Reads whatever is already in `reports/` and judges it. Measures nothing itself, which is why it is
# instant and why it can be run over and over while something is being fixed.
ci-check: ## The verdict table over whatever is already in reports/
	@$(FREEPORTS_DEV) ci-check $(REPO)

branch-class: ## Which class this branch is in, and the rule that put it there
	@$(FREEPORTS_DEV) branch-class $(REPO)

# `info.validation_sha256` is the hash of the hashes of every file a grant covers, and when it moves
# `info.version` must move with it — otherwise the manifest claims that version X covers content Y,
# which is false, and the point of the field is that it is not.
#
# `--update` writes the new fingerprint **only when the manifest is already entitled to hold it**,
# which is to say once the version has moved. On a dev branch a fingerprint that moved without its
# version is a loud warning and the value is deliberately *not* written — writing it is exactly how
# the manifest would come to hold the false statement. On a prod branch it refuses.
fingerprint: ## Check that the manifest's hash of its own contents moved with its version
	@$(FREEPORTS_DEV) fingerprint $(REPO) --update

##@ Cleaning

# `reports/` is included: it is gitignored, it is facts about one commit on one machine, and
# `make ci-fast` writes it again in seconds. Nothing here touches `validation/` or `ci/report/`,
# which are committed artefacts rather than build products.
clean: ## Measurements, caches and stray logs — nothing committed
	rm -rf $(REPORTS) .pytest_cache .ruff_cache .benchmarks
	find . -name '__pycache__' -type d -exec rm -rf {} + 2>/dev/null || true
	rm -f freeports.log.jsonl
