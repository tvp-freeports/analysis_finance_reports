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
#
# ---------------------------------------------------------------------------
# How this file is organised
# ---------------------------------------------------------------------------
# What stays here is what the whole build shares: the environment and tree variables, `help`,
# `doctor`, and the `include` line below. Everything else lives in `mk/`, one fragment per area,
# each opening with the part of this essay that belongs to it.
#
# **Included, not recursive.** A sub-`make` per area would give each one its own process, its own
# variables and its own dependency graph, and the whole point of a single graph is that `make ci`
# can decide `develop` needs rebuilding once rather than four times. Inclusion keeps one namespace,
# one graph and one `.DEFAULT_GOAL` — and `$(MAKEFILE_LIST)` already covers included files, so the
# `help` awk below needs no change and a new fragment's targets appear in `make help` on their own.
# The one recursive call that remains is `docs/Makefile`, which is Sphinx's own generated file and
# belongs to Sphinx.
#
# **The quality surface is laid out on two axes**, so target names are composed rather than
# memorised: `<what>` alone is everything, `<what>-<which>` is one slice.
#
#     test / lint / fmt / coverage / doc-coverage      × rust / python / formats
#
# so `test-rust`, `lint-python`, `coverage-formats` all exist without anybody having to look them
# up, and `ci-rust` / `ci-python` are the whole of one column.
#
# `ci.yaml` at the root says what this repository demands of a commit — the branch classes, the
# thresholds, the key server. `make branch-class` says which class the branch you are on is in.

.DEFAULT_GOAL := help

# Output kept whole per target, so that `make -j` stays readable. Without it two recipes running at
# once interleave line by line and a failure is a jigsaw; with it each target's output arrives in
# one piece, in the order the targets finished. It costs nothing on a sequential run, which is why
# it is set here rather than passed by whoever remembers to.
#
# `-j` itself is deliberately *not* set here. It is right for the quality surface, whose recipes
# are independent measurements, and wrong for `install`, whose recipes are pip invocations that
# would race over the same environment. `.githooks/pre-commit` asks for it where it applies.
MAKEFLAGS += -Otarget

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

# Where the measurements land. Gitignored here and in every other freeports repository: these are
# facts about one commit on one machine, not something to carry in the history.
REPORTS    = reports

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

.PHONY: help doctor installcheck installdirs all \
        venv githooks init \
        install install-engine install-binary install-tools install-dev-deps install-docs-deps \
        install-ci-deps dev-engine dev-formats dev-docs dev-ci dev-all uninstall reinstall \
        develop build dist distcheck check-compile \
        check test test-fast test-slow test-all test-formats \
        test-rust test-rust-unit test-rust-integration test-rust-doc \
        test-python test-python-slow test-python-online test-python-all \
        lint lint-rust lint-python lint-formats fmt fmt-rust fmt-python fmt-check \
        coverage coverage-rust coverage-python coverage-formats \
        doc-coverage doc-coverage-rust doc-coverage-python \
        ci ci-fast ci-full ci-docs ci-rust ci-python ci-check branch-class release pre-commit \
        ci-report ci-report-json ci-report-badges ci-report-readme ci-report-docs \
        ci-report-html \
        validation validation-report check-grants check-keys \
        docs html docs-html docs-rustdoc docs-validation docs-site-coverage docs-serve docs-lang \
        i18n i18n-extract i18n-update i18n-build \
        clean mostlyclean clean-docs clean-rust distclean maintainer-clean

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
	@echo "Measurement tools (make dev-ci supplies them)"
	@if $(CARGO) llvm-cov --version >/dev/null 2>&1; then \
	    echo "  cargo-llvm-cov: $$($(CARGO) llvm-cov --version 2>&1 | head -1)"; \
	 else \
	    echo "  cargo-llvm-cov: MISSING  ->  make dev-ci   (tests.rust.lines is unmeasured without it)"; \
	 fi
	@if rustup component list --installed 2>/dev/null | grep -q llvm-tools; then \
	    echo "  llvm-tools:     installed"; \
	 else \
	    echo "  llvm-tools:     MISSING  ->  make dev-ci"; \
	 fi
	@if rustup toolchain list 2>/dev/null | grep -q nightly; then \
	    echo "  nightly:        installed"; \
	 else \
	    echo "  nightly:        MISSING  ->  make dev-ci   (docs.rust is unmeasured without it)"; \
	 fi
	@if [ -x "$(PYTHON)" ] && $(PYTHON) -m pip show pytest-cov >/dev/null 2>&1; then \
	    echo "  pytest-cov:     installed"; \
	 else \
	    echo "  pytest-cov:     MISSING  ->  make dev-ci"; \
	 fi
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


installdirs: ## Create the directories install would write into (canonical GNU name)
	mkdir -p $(DESTDIR)$(bindir)

# ---------------------------------------------------------------------------
# The areas
# ---------------------------------------------------------------------------
# One line, one namespace, one dependency graph. `$(MAKEFILE_LIST)` covers these, so `help` above
# lists their targets with no change to it.
include mk/install.mk
include mk/build.mk
include mk/test.mk
include mk/quality.mk
include mk/validation.mk
include mk/ci.mk
include mk/docs.mk
include mk/clean.mk
