# Makefile at the repository root: the single entry point.
#
# This repository is a collection of packages under `packages/` written in two languages — a Rust
# crate that is also the `freeports` Python extension, and two Python tooling packages — plus a
# Sphinx site in `docs/`. Each ecosystem already has its own tool (cargo, pip/maturin,
# sphinx-build); this file does not replace them, it orchestrates them, and it gives every role
# **one command to learn**:
#
#     make install        end user            the `freeports` command and the Python module
#     make dev-engine     engine              + Rust toolchain, extension in place, tests, lint
#     make dev-formats    format author       + freeports-dev and freeports-validate
#     make dev-docs       documentation       + Sphinx and i18n dependencies
#     make dev-all        maintainer          everything
#     make doctor         anyone              what is installed, what is missing, which target helps
#
# On Windows the same commands are typed `make.bat <target>` from `cmd` or PowerShell, or `make
# <target>` from Git Bash. `make.bat` is a shim of a dozen lines that starts a POSIX shell and
# calls this file: there is one build system here, not one per platform.
#
# `make help` lists the rest, and it is generated from the `##` comments below: a new target
# documents itself, a removed one disappears from the help. That is the only arrangement under
# which this file does not end up like the `contrib/requirements.*.txt` it replaced.
#
# The canonical names from the *GNU Makefile standard* are all here with their standard meaning —
# `all`, `install`, `uninstall`, `check`, `installcheck`, `clean`, `distclean`, `dist` — and
# `install` honours `PREFIX` and `DESTDIR`. These are the Autotools conventions without the
# Autotools machinery: nothing here compiles C, and the platform discovery `autoconf` exists for is
# already done by rustc and by wheel tags. See `agent-memory/build-system-strategy.md` §2.1.
#
# Every target is a recipe of commands you could type by hand. That is deliberate: the
# documentation quotes them in full, and two formulations of the same thing always diverge.

.DEFAULT_GOAL := help

# ---------------------------------------------------------------------------
# Environment
# ---------------------------------------------------------------------------
# Everything goes through `ENV_PREFIX`, the Python environment being worked on. If you already have
# one active — virtualenv or conda — that is the one; otherwise it is the repository's own venv,
# which `make venv` creates. A different environment is one command-line variable away, and needs
# nothing else:
#
#     make install ENV_PREFIX=/opt/pythons/3.12
#
# The one platform difference this file has to know: a virtualenv keeps its programs in `bin/` on
# Unix and in `Scripts/` on Windows, where they also carry a `.exe` suffix. Every path below is
# built out of `BIN` and `EXE`, so no target has to know which of the two it is running on, and
# there is no second copy of this file to keep in step. `make.bat` is the entry point from `cmd`
# or PowerShell: it starts a POSIX shell (Git Bash, MSYS2) and hands it this very Makefile.
ifeq ($(OS),Windows_NT)
BIN = Scripts
EXE = .exe
PY      ?= python
else
BIN = bin
EXE =
PY      ?= python3
endif

CARGO   ?= cargo
VENV    ?= venv/freeports-dev

# `abspath` and not `$(CURDIR)/$(VENV)`: the latter prepends the current directory even to an
# already absolute path, and `make venv VENV=/tmp/scratch` ends up somewhere that does not exist.
REPO_VENV = $(abspath $(VENV))

ifdef VIRTUAL_ENV
ENV_PREFIX ?= $(VIRTUAL_ENV)
else ifdef CONDA_PREFIX
ENV_PREFIX ?= $(CONDA_PREFIX)
else
ENV_PREFIX ?= $(REPO_VENV)
endif

ENV_BIN  = $(ENV_PREFIX)/$(BIN)
PYTHON   = $(ENV_BIN)/python$(EXE)
PIP      = $(PYTHON) -m pip

# GNU conventions. By default the binary goes into the active environment, so that `make uninstall`
# is complete and nothing is left behind in your home directory; the price is that the command is
# not visible with the environment deactivated. For a system-wide install:
# `make install-binary PREFIX=/usr/local`, and a packager has `DESTDIR` for the staging directory.
PREFIX  ?= $(ENV_PREFIX)
DESTDIR ?=
bindir  ?= $(PREFIX)/$(BIN)

# ---------------------------------------------------------------------------
# Tree
# ---------------------------------------------------------------------------
CRATEDIR   = packages/freeports
MANIFEST   = $(CRATEDIR)/Cargo.toml
PKG_DEV    = packages/freeports_dev
PKG_VALID  = packages/freeports_validate
DISTDIR    = dist

# The Python sources of this repository: the two tooling packages, the suites they have, and the
# Sphinx configuration. The engine is not here because it has no Python sources — the package *is*
# the compiled extension.
#
# The `tests` directories are listed separately because they sit beside `src/` rather than inside
# it: a test file is a Python source like any other, and leaving it out of `ruff` is how a suite
# ends up being the only unlinted code in a repository.
PY_SOURCES = $(PKG_DEV)/src $(PKG_DEV)/tests $(PKG_VALID)/src $(PKG_VALID)/tests \
             docs/source/conf.py

MATURIN       = $(ENV_BIN)/maturin$(EXE)
RUFF          = $(ENV_BIN)/ruff$(EXE)
PYTEST        = $(ENV_BIN)/pytest$(EXE)
SPHINXBUILD   = $(ENV_BIN)/sphinx-build$(EXE)
SPHINXINTL    = $(ENV_BIN)/sphinx-intl$(EXE)
FREEPORTS_DEV = $(ENV_BIN)/freeports-dev$(EXE)

# Not `LANG`: that is a standard environment variable, and make would silently import it,
# inheriting the terminal's locale instead of the language actually asked for.
DOCLANG   ?= it
DOCS_PORT ?= 8000

.PHONY: help doctor installcheck all \
        venv githooks init \
        install install-engine install-binary install-tools install-dev-deps install-docs-deps \
        dev-engine dev-formats dev-docs dev-all uninstall reinstall \
        develop build dist check-compile \
        check test test-all test-unit test-full test-doc test-integration test-formats \
        test-tools test-tools-slow test-tools-all \
        lint fmt fmt-check pre-commit \
        validation-report check-grants \
        docs docs-html docs-rustdoc docs-validation docs-coverage docs-serve docs-lang \
        i18n i18n-extract i18n-update i18n-build \
        clean clean-docs clean-rust distclean

##@ Help

help: ## List the available targets
	@awk 'BEGIN {FS = ":.*##"} \
	     /^##@/ { printf "\n%s\n", substr($$0, 5); next } \
	     /^[a-zA-Z0-9_.-]+:.*##/ { printf "  %-20s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@echo
	@echo "Environment in use: $(ENV_PREFIX)"
	@echo "Details and diagnosis: make doctor"

doctor: ## Diagnosis: what is installed, what is missing, which target supplies it
	@echo "Environment"
	@echo "  ENV_PREFIX     $(ENV_PREFIX)"
	@if [ -x "$(PYTHON)" ]; then \
	    echo "  python         $$($(PYTHON) --version 2>&1)"; \
	 else \
	    echo "  python         MISSING  ->  make venv"; \
	 fi
	@if command -v $(CARGO) >/dev/null 2>&1; then \
	    echo "  cargo          $$($(CARGO) --version)"; \
	 else \
	    echo "  cargo          MISSING  ->  install rustup and the stable channel"; \
	 fi
	@echo
	@echo "Distributions"
	@for pkg in freeports freeports-dev freeports-validate; do \
	    if [ -x "$(PYTHON)" ] && $(PYTHON) -m pip show "$$pkg" >/dev/null 2>&1; then \
	        echo "  $$pkg: installed"; \
	    else \
	        echo "  $$pkg: MISSING"; \
	    fi; \
	 done
	@echo
	@echo "Commands"
	@for cmd in freeports freeports-dev freeports-validate; do \
	    if [ -x "$(ENV_BIN)/$$cmd$(EXE)" ]; then \
	        echo "  $$cmd: $(ENV_BIN)/$$cmd$(EXE)"; \
	    elif command -v "$$cmd" >/dev/null 2>&1; then \
	        echo "  $$cmd: $$(command -v $$cmd)  (outside the environment)"; \
	    else \
	        echo "  $$cmd: MISSING"; \
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
	@echo "Consistency"
	@if [ -x "$(PYTHON)" ] && $(PYTHON) -m pip show freeports >/dev/null 2>&1 \
	    && [ ! -x "$(ENV_BIN)/freeports$(EXE)" ]; then \
	    echo "  module present but command missing  ->  make install-binary"; \
	 fi
	@if [ -x "$(PYTHON)" ] && $(PYTHON) -m pip show freeports_analysis >/dev/null 2>&1; then \
	    echo "  stale package freeports_analysis, retired two rewrites ago  ->  make uninstall"; \
	 fi
	@if [ -d build ]; then echo "  build/ is setuptools debris from the retired engine  ->  make clean"; fi
	@echo "  (no line above means nothing was found out of place)"

installcheck: ## Verify that the installation actually answers
	$(ENV_BIN)/freeports$(EXE) --help >/dev/null
	$(PYTHON) -c "import freeports; print(freeports.__doc__.splitlines()[0])"
	@echo "Installation verified."

##@ Environment and installation

venv: ## Create venv/freeports-dev, unless another environment is already in use
	@if [ "$(ENV_PREFIX)" != "$(REPO_VENV)" ]; then \
	    echo "Environment already chosen: $(ENV_PREFIX) — no venv to create."; \
	 elif [ -x "$(PYTHON)" ]; then \
	    echo "venv already present: $(REPO_VENV)"; \
	 else \
	    $(PY) -m venv "$(REPO_VENV)" && "$(REPO_VENV)/$(BIN)/python$(EXE)" -m pip install --upgrade pip; \
	 fi

githooks: ## Wire up the repository's git hooks (.githooks/ and .gitconfig)
	git config --local core.hooksPath "$(CURDIR)/.githooks"
	git config --local include.path "$(CURDIR)/.gitconfig"

init: dev-all ## First-time setup from a fresh clone: environment, hooks, everything installed
	@echo
	@echo "Ready. Activate the environment with:  source $(REPO_VENV)/$(BIN)/activate"

install: install-engine install-binary ## End user: the Python module and the freeports command
	@echo "Engine installed. Verify with: make installcheck"

install-engine: venv ## Only the Python extension (maturin, through pip)
	$(PIP) install $(CRATEDIR)

# The binary and the extension are two separate products of the same crate, and neither implies the
# other: `pip install` builds only the extension. That is why this target exists and why `install`
# includes it — without it you install the package and the command is nowhere.
#
# `cp` plus `chmod` and not `install -m 755`: the latter is a coreutils program that a Windows
# shell does not necessarily carry, and the two are the same thing here.
install-binary: build ## Only the freeports command, compiled and placed in bindir
	@mkdir -p "$(DESTDIR)$(bindir)"
	cp "$(CRATEDIR)/target/release/freeports$(EXE)" "$(DESTDIR)$(bindir)/freeports$(EXE)"
	chmod 755 "$(DESTDIR)$(bindir)/freeports$(EXE)"
	@echo "freeports installed in $(DESTDIR)$(bindir)/freeports$(EXE)"

install-tools: venv ## freeports-dev and freeports-validate
	$(PIP) install $(PKG_DEV) $(PKG_VALID)

install-dev-deps: venv ## Development tools (maturin, ruff, pytest, build, twine)
	$(PIP) install -r contrib/requirements.dev.txt

install-docs-deps: venv ## Dependencies of the documentation build and its i18n
	$(PIP) install -r contrib/requirements.docs.txt

dev-engine: install-dev-deps develop install-binary githooks ## Working on the engine
	@echo "Engine environment ready. Suite: make check"

dev-formats: install install-tools ## Writing formats
	@echo "Formats environment ready. Test a repository: make test-formats REPO=<path>"

# autodoc really imports the documented packages, it does not mock them: building the site requires
# all three to be installed, extension included.
dev-docs: install install-tools install-docs-deps ## Writing and translating documentation
	@echo "Documentation environment ready. Site: make docs"

dev-all: install-dev-deps develop install-binary install-tools install-docs-deps githooks ## Everything
	@echo "Complete environment."

uninstall: ## Remove the distributions and the command from the active environment
	-$(PIP) uninstall -y freeports freeports-dev freeports-validate
	-$(PIP) uninstall -y freeports_analysis
	rm -f "$(DESTDIR)$(bindir)/freeports$(EXE)"

reinstall: uninstall dev-all ## Uninstall and reinstall everything

##@ Building

all: build ## Build the artifacts without installing anything (canonical GNU name)

# The development loop: rebuilds the extension *in place*. A stale `.so` is the usual explanation
# for a Rust change that "had no effect" on the Python side.
develop: install-dev-deps ## Rebuild the extension in place (maturin develop --release)
	VIRTUAL_ENV="$(ENV_PREFIX)" $(MATURIN) develop --release -m $(MANIFEST)

build: ## Compile the freeports binary in release mode
	$(CARGO) build --release --manifest-path $(MANIFEST)

# `--sdist` is not maturin's default: without it the crate would ship as a binary wheel only, and a
# platform with no prebuilt wheel would have nothing to build from.
dist: ## Wheels and sdists of the three distributions into dist/
	@mkdir -p $(DISTDIR)
	$(MATURIN) build --release --sdist --out $(DISTDIR) -m $(MANIFEST)
	$(PYTHON) -m build --outdir $(DISTDIR) $(PKG_DEV)
	$(PYTHON) -m build --outdir $(DISTDIR) $(PKG_VALID)

# `--all-targets` covers tests, examples and the binary: a compilation error in `tests/` or in
# `examples/p0_profile.rs` is invisible when only the library is compiled.
check-compile: ## Compile without running anything, tests and examples included
	$(CARGO) check --manifest-path $(MANIFEST) --all-targets

##@ Tests

# Most of the coverage lives in the `packages/freeports` crate — both the unit tests (in the
# `mod tests` blocks inside `src/`) and the integration ones (`tests/`, one file per flow). The
# tooling packages are Python and run under pytest instead, which is `test-tools`; `test` runs both,
# so the commit gate covers both without the hook knowing there are two languages behind it.
#
# The tests that cross into Python (the `python_boundary` modules: the ones that really open a PDF
# with PyMuPDF) only run inside the `freeports-dev` environment; outside it they fail with a
# message saying so.

check: test ## Run the test suite (canonical GNU name, alias of test)

# What the commit hook runs. The crate's suite in full — its integration tests are the ones that
# would notice a regression in the whole flow, and that is where you want to notice it rather than
# after the push — plus the *fast* half of the tooling suite.
#
# The slow half is `test-tools-slow`, and leaving it out here is deliberate rather than a
# concession: it is minutes of process starts, and a gate that costs minutes is a gate that gets
# disabled. Run `test-all` before calling a piece of work finished.
test: test-full test-tools ## The commit gate — the crate, and the fast tooling tests

test-all: test-full test-tools-all ## Everything, slow tests included — before you call it done

test-unit: ## Unit tests only, without the crate's integration test files
	$(CARGO) test --manifest-path $(MANIFEST) --lib

test-full: ## Unit + integration + doctests
	$(CARGO) test --manifest-path $(MANIFEST)

test-integration: ## Integration tests only (packages/freeports/tests/)
	$(CARGO) test --manifest-path $(MANIFEST) --test '*'

test-doc: ## Only the examples inside the doc-comments
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
# for, not by hand — and deselected by default. `test-tools` is therefore the fast set and is what
# the commit hook runs; `test-tools-slow` is the rest and is run on request. A commit gate that
# costs seven minutes is a commit gate people turn off, and then it gates nothing.
#
# Both sets have to pass before a piece of work is finished. `test-tools-all` runs them together.
#
# The two packages are two invocations rather than one over both directories, because each carries
# its own `[tool.pytest.ini_options]` — the markers, and the deselection that keeps the internet out
# of a normal run — and pytest reads those from the rootdir it settles on, which over two packages
# at once would be neither of them.
#
# `freeports_dev`'s own suite is fast throughout: it initialises repositories in a temporary
# directory and reads what came out, so it belongs in the commit gate whole.
test-tools: ## The tooling packages' fast Python tests (pytest) — the commit gate
	$(PYTEST) $(PKG_DEV)
	$(PYTEST) $(PKG_VALID)

test-tools-slow: ## The tooling tests that start the command as a process (minutes)
	$(PYTEST) $(PKG_VALID) -m 'slow and not network'

test-tools-all: ## Every tooling test, fast and slow, except the ones needing the internet
	$(PYTEST) $(PKG_DEV)
	$(PYTEST) $(PKG_VALID) -m 'not network'

##@ Quality

lint: ## clippy on the crate, ruff on the Python sources
	$(CARGO) clippy --manifest-path $(MANIFEST) --all-targets
	$(RUFF) check $(PY_SOURCES)

fmt: ## Reformat: cargo fmt and ruff format
	$(CARGO) fmt --manifest-path $(MANIFEST)
	$(RUFF) format $(PY_SOURCES)

fmt-check: ## Verify formatting without rewriting anything
	$(CARGO) fmt --manifest-path $(MANIFEST) --check
	$(RUFF) format --check $(PY_SOURCES)

# The gate that `.githooks/pre-commit` fires at every commit. It lives here and not in the hook so
# that it can grow without the hook being touched.
#
# `fmt-check` is **not** part of it, deliberately: the git `clean` filter (see `.gitconfig`) formats
# Python files as they are committed, so an unformatted working tree is a normal condition rather
# than an error. For Rust that does not hold, and adding `fmt-check` here is a one-line change to
# make after a deliberate pass of `make fmt`.
pre-commit: lint test ## The commit gate: lint + the full suite

##@ Validation

# What `validation/` claims, written down wherever a reader meets it.
#
# The three badges in README.md, the summary block in it, the seven pages under
# `docs/source/validation/report/` and the single-page HTML rendering are all functions of the same
# thing: the signed documents in `validation/`. A commit that changes a granted file and leaves them
# alone publishes figures that are already wrong, which is why `.githooks/pre-commit` runs this and
# stages what it rewrote.
#
# **One walk of the repository, rendered ten times.** `collect` resolves every methodology page from
# the configured sources; doing it once per artefact would fetch each page ten times over for an
# answer that cannot have changed in between. So the model is collected into a temporary file and
# every rendering is `--model` against it.
#
# The paths below are literal, as they are in a formats repository's hook: where a table belongs in
# a documentation tree is a judgement about that tree, not a setting.
# `--repo` is given outright rather than left to be resolved. A `formats_repo:` line in whatever
# configuration file is found from here outranks "the enclosing Git repository", so a machine
# configured for a formats repository would otherwise have `make validation-report` write this
# repository's README out of somebody else's grants.
#
# The sources are named here for a sharper reason. The grants in `validation/` are made under the
# methodology pages **of this repository**, and the two documentation channels do not carry the same
# text: `latest` is built from this branch and matches `docs/source/validation/` byte for byte,
# `stable` is the last release and lags behind it. `freeports-validate`'s own default is `stable`
# alone, so a run that said nothing would report every grant here as a hash mismatch — an accusation
# produced entirely by reading a different publication of the same page.
#
# Two sources are needed, in this order, and only the command line and a configuration file can
# carry two (`FREEPORTS_VALIDATE_SOURCE` holds exactly one and is never split). A configuration file
# cannot be committed here — every name the engine recognises is gitignored — so the command line it
# is. Override the pair with `make validation-report VALIDATE_SOURCES="<pattern> <pattern>"`; giving
# it empty falls back to whatever your own tiers say.
VALIDATE_SOURCES ?= https://docs.freeports.org/en/latest/_sources/validation/*.rst.txt \
                    https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt
VALIDATE_SOURCE_ARGS = $(foreach source,$(VALIDATE_SOURCES),--source "$(source)")

VALIDATE      = $(ENV_BIN)/freeports-validate$(EXE) --repo "$(CURDIR)" $(VALIDATE_SOURCE_ARGS)
BADGES_DIR    = validation/report/badges
REPORT_DIR    = docs/source/validation/report
# Generated into `_extra/`, which is gitignored and copied verbatim into the root of the site by
# `html_extra_path` — the same arrangement as rustdoc. So it belongs to the *build* and not to the
# commit, and Read the Docs regenerates it in its `pre_build` job.
COVERAGE_HTML = docs/source/_extra/validation/coverage.html
REPORT_TABLES = file-contributor file-methodology contributor-methodology \
                contributor-file methodology-file methodology-contributor

validation-report: ## Refresh the badges, the README block, the seven doc pages and the HTML
	@model=$$(mktemp) || exit 1; \
	 trap 'rm -f "$$model"' EXIT; \
	 $(VALIDATE) collect > "$$model" || { \
	     echo "freeports-validate: the grants could not be read — nothing was rewritten" >&2; \
	     exit 1; }; \
	 $(VALIDATE) report --model "$$model" --format badges --out $(BADGES_DIR)/; \
	 $(VALIDATE) report --model "$$model" --format markdown --out README.md; \
	 $(VALIDATE) report --model "$$model" --format rst --out $(REPORT_DIR)/index.rst; \
	 for table in $(REPORT_TABLES); do \
	     $(VALIDATE) report --model "$$model" --format rst --table "$$table" \
	         --out "$(REPORT_DIR)/$$table.rst" || exit 1; \
	 done; \
	 mkdir -p $(dir $(COVERAGE_HTML)); \
	 $(VALIDATE) report --model "$$model" --format html --out $(COVERAGE_HTML)

# The integrity check itself: signatures verify, hashes still match, methodology pages still say
# what they said. It is *not* a lint — a passing run says the claims in `validation/` hold, not that
# the software is correct — and it needs no signing key to check somebody else's document.
check-grants: ## Verify every claim made in validation/
	$(VALIDATE) check-grants

##@ Documentation

docs: docs-rustdoc docs-validation docs-html ## The whole site, rustdoc and coverage page included
	@echo "Site at docs/build/html/index.html"

docs-html: ## Sphinx only — what you want while writing prose
	$(MAKE) -C docs html SPHINXBUILD="$(SPHINXBUILD)"

docs-rustdoc: ## cargo doc only, deposited in docs/source/_extra/rustdoc/
	$(MAKE) -C docs rustdoc CARGO="$(CARGO)"

# The one artefact of `validation-report` that belongs to the build rather than to the commit, so it
# is also reachable on its own: Read the Docs runs exactly this in its `pre_build` job, and a local
# `make docs` has to produce the same site.
docs-validation: ## The validation coverage page only, into docs/source/_extra/validation/
	$(MAKE) -C docs validation-coverage \
	    VALIDATE="$(ENV_BIN)/freeports-validate$(EXE)" VALIDATE_SOURCES="$(VALIDATE_SOURCES)"

# `sphinx.ext.coverage` measures how much of the installed packages' API is actually documented.
# The report ends up in docs/build/coverage/python.txt.
docs-coverage: ## Coverage report of the API documentation
	$(MAKE) -C docs coverage SPHINXBUILD="$(SPHINXBUILD)"
	@echo "Report at docs/build/coverage/python.txt"

docs-lang: ## Build a single language: make docs-lang DOCLANG=it
	$(SPHINXBUILD) -b html -D language=$(DOCLANG) docs/source docs/build/$(DOCLANG)

docs-serve: ## Serve docs/build/html locally (DOCS_PORT=8000)
	$(PYTHON) -m http.server $(DOCS_PORT) --directory docs/build/html

##@ Documentation internationalisation

# The three steps translators know: extract the strings from the current sources, merge them into
# the existing `.po` files, compile those into the `.mo` files Sphinx actually reads.
i18n: i18n-extract i18n-update i18n-build ## The three steps in a row

i18n-extract: ## Extract the translatable strings from the sources (docs/build/gettext)
	$(MAKE) -C docs gettext SPHINXBUILD="$(SPHINXBUILD)"

i18n-update: ## Merge the extracted strings into the .po files under docs/source/locales
	$(SPHINXINTL) update -p docs/build/gettext -d docs/source/locales

i18n-build: ## Compile the .po files into .mo
	$(SPHINXINTL) build -d docs/source/locales

##@ Cleaning

# `clean` deliberately does not call `cargo clean`: deleting `target/` costs a full recompilation,
# which is not what you want from a target invoked out of habit. That lives in `clean-rust`.
clean: clean-docs ## Build products and debris, without throwing away the cargo cache
	rm -rf $(DISTDIR) build packages/*/build
	rm -rf packages/*/src/*.egg-info packages/*/*.egg-info
	rm -rf .pytest_cache packages/*/.pytest_cache .benchmarks packages/*/.benchmarks .ruff_cache
	find . -name '__pycache__' -type d -not -path './venv/*' -not -path '*/target/*' \
	    -exec rm -rf {} + 2>/dev/null || true
	rm -f freeports.log freeports.log.jsonl
	rm -f packages/*/freeports.log packages/*/freeports.log.jsonl

clean-docs: ## Only the products of the documentation build
	rm -rf docs/build docs/source/_generated docs/source/_extra

clean-rust: ## cargo clean — the next build will be a full one
	$(CARGO) clean --manifest-path $(MANIFEST)

distclean: clean clean-rust ## clean + clean-rust + removal of the repository's venv
	rm -rf $(REPO_VENV)
