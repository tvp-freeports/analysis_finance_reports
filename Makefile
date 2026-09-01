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
PY      ?= python3
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

ENV_BIN  = $(ENV_PREFIX)/bin
PYTHON   = $(ENV_BIN)/python
PIP      = $(PYTHON) -m pip

# GNU conventions. By default the binary goes into the active environment, so that `make uninstall`
# is complete and nothing is left behind in your home directory; the price is that the command is
# not visible with the environment deactivated. For a system-wide install:
# `make install-binary PREFIX=/usr/local`, and a packager has `DESTDIR` for the staging directory.
PREFIX  ?= $(ENV_PREFIX)
DESTDIR ?=
bindir  ?= $(PREFIX)/bin

# ---------------------------------------------------------------------------
# Tree
# ---------------------------------------------------------------------------
CRATEDIR   = packages/freeports
MANIFEST   = $(CRATEDIR)/Cargo.toml
PKG_DEV    = packages/freeports_dev
PKG_VALID  = packages/freeports_validate
DISTDIR    = dist

# The Python sources of this repository: the two tooling packages and the Sphinx configuration.
# The engine is not here because it has no Python sources — the package *is* the compiled
# extension.
PY_SOURCES = $(PKG_DEV)/src $(PKG_VALID)/src docs/source/conf.py

MATURIN       = $(ENV_BIN)/maturin
RUFF          = $(ENV_BIN)/ruff
SPHINXBUILD   = $(ENV_BIN)/sphinx-build
SPHINXINTL    = $(ENV_BIN)/sphinx-intl
FREEPORTS_DEV = $(ENV_BIN)/freeports-dev

# Not `LANG`: that is a standard environment variable, and make would silently import it,
# inheriting the terminal's locale instead of the language actually asked for.
DOCLANG   ?= it
DOCS_PORT ?= 8000

.PHONY: help doctor installcheck all \
        venv githooks init \
        install install-engine install-binary install-tools install-dev-deps install-docs-deps \
        dev-engine dev-formats dev-docs dev-all uninstall reinstall \
        develop build dist check-compile \
        check test test-unit test-full test-doc test-integration test-formats \
        lint fmt fmt-check pre-commit \
        docs docs-html docs-rustdoc docs-coverage docs-serve docs-lang \
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
	    if [ -x "$(ENV_BIN)/$$cmd" ]; then \
	        echo "  $$cmd: $(ENV_BIN)/$$cmd"; \
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
	    && [ ! -x "$(ENV_BIN)/freeports" ]; then \
	    echo "  module present but command missing  ->  make install-binary"; \
	 fi
	@if [ -x "$(PYTHON)" ] && $(PYTHON) -m pip show freeports_analysis >/dev/null 2>&1; then \
	    echo "  stale package freeports_analysis, retired two rewrites ago  ->  make uninstall"; \
	 fi
	@if [ -d build ]; then echo "  build/ is setuptools debris from the retired engine  ->  make clean"; fi
	@echo "  (no line above means nothing was found out of place)"

installcheck: ## Verify that the installation actually answers
	$(ENV_BIN)/freeports --help >/dev/null
	$(PYTHON) -c "import freeports; print(freeports.__doc__.splitlines()[0])"
	@echo "Installation verified."

##@ Environment and installation

venv: ## Create venv/freeports-dev, unless another environment is already in use
	@if [ "$(ENV_PREFIX)" != "$(REPO_VENV)" ]; then \
	    echo "Environment already chosen: $(ENV_PREFIX) — no venv to create."; \
	 elif [ -x "$(PYTHON)" ]; then \
	    echo "venv already present: $(REPO_VENV)"; \
	 else \
	    $(PY) -m venv "$(REPO_VENV)" && "$(REPO_VENV)/bin/python" -m pip install --upgrade pip; \
	 fi

githooks: ## Wire up the repository's git hooks (.githooks/ and .gitconfig)
	git config --local core.hooksPath "$(CURDIR)/.githooks"
	git config --local include.path "$(CURDIR)/.gitconfig"

init: dev-all ## First-time setup from a fresh clone: environment, hooks, everything installed
	@echo
	@echo "Ready. Activate the environment with:  source $(REPO_VENV)/bin/activate"

install: install-engine install-binary ## End user: the Python module and the freeports command
	@echo "Engine installed. Verify with: make installcheck"

install-engine: venv ## Only the Python extension (maturin, through pip)
	$(PIP) install $(CRATEDIR)

# The binary and the extension are two separate products of the same crate, and neither implies the
# other: `pip install` builds only the extension. That is why this target exists and why `install`
# includes it — without it you install the package and the command is nowhere.
install-binary: build ## Only the freeports command, compiled and placed in bindir
	@mkdir -p "$(DESTDIR)$(bindir)"
	install -m 755 "$(CRATEDIR)/target/release/freeports" "$(DESTDIR)$(bindir)/freeports"
	@echo "freeports installed in $(DESTDIR)$(bindir)/freeports"

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
	rm -f "$(DESTDIR)$(bindir)/freeports"

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

# Almost all the coverage lives in the `packages/freeports` crate — both the unit tests (in the
# `mod tests` blocks inside `src/`) and the integration ones (`tests/`, one file per flow). The
# other two packages are Python and today have no tests of their own: when they do, the place to
# add them is a `test-python` target next to these, not a line in the commit hook.
#
# The tests that cross into Python (the `python_boundary` modules: the ones that really open a PDF
# with PyMuPDF) only run inside the `freeports-dev` environment; outside it they fail with a
# message saying so.

check: test ## Run the test suite (canonical GNU name, alias of test)

# At commit time everything runs: the integration tests are the ones that would notice a regression
# in the whole flow, and that is where you need to notice it, not after the push.
test: test-full ## The crate's full suite — what runs at commit time

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

##@ Documentation

docs: docs-rustdoc docs-html ## The whole site, rustdoc included
	@echo "Site at docs/build/html/index.html"

docs-html: ## Sphinx only — what you want while writing prose
	$(MAKE) -C docs html SPHINXBUILD="$(SPHINXBUILD)"

docs-rustdoc: ## cargo doc only, deposited in docs/source/_extra/rustdoc/
	$(MAKE) -C docs rustdoc CARGO="$(CARGO)"

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
