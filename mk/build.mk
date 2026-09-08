# Building: the two products of one crate.
#
# `packages/freeports` is compiled twice from the same sources — once by maturin into the Python
# extension `freeports`, once by cargo into the `freeports` command. They are one crate because
# they are one algorithm; they are two products because they are used by different people in
# different ways, and neither is a wrapper around the other.

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

# Build the distributions, install them into a scratch environment, and check that they answer
# there. Worth more than the convention it satisfies: `freeports_dev`'s `package-data` was missing
# `lib/**/*`, which shipped a package whose commands failed at their first subcommand. An installed
# copy is the only thing that shows that, and no other target here makes one.
distcheck: dist ## dist, then install into a scratch environment and run installcheck there
	rm -rf $(DISTDIR)/.distcheck
	$(PY) -m venv $(DISTDIR)/.distcheck
	$(DISTDIR)/.distcheck/$(BIN)/python$(EXE) -m pip install --quiet --upgrade pip
	# All three wheels in one command, the engine included. `freeports` is a dependency of
	# `freeports-dev` and is not on any index — it is built here by maturin — so installing the
	# tooling alone sends pip looking for it on PyPI and it fails with "no matching distribution".
	# That is not a packaging bug to fix in `pyproject.toml`: an unpublished dependency is exactly
	# what this repository has, and the answer is to hand pip the wheel that satisfies it.
	$(DISTDIR)/.distcheck/$(BIN)/python$(EXE) -m pip install --quiet \
	    $(DISTDIR)/freeports-*.whl $(DISTDIR)/freeports_dev-*.whl $(DISTDIR)/freeports_validate-*.whl
	# `--help` alone would not have caught what this target exists to catch. The missing
	# `lib/**/*` in `package-data` shipped a package whose *subcommands* failed at their first
	# data file while `--help` answered perfectly, so each one is asked to do something that
	# reaches into `lib/`.
	$(DISTDIR)/.distcheck/$(BIN)/freeports-dev$(EXE) --help >/dev/null
	$(DISTDIR)/.distcheck/$(BIN)/freeports-validate$(EXE) --help >/dev/null
	$(DISTDIR)/.distcheck/$(BIN)/freeports-dev$(EXE) init-input-db --quiet \
	    $(DISTDIR)/.distcheck/scratch-db >/dev/null
	@test -f $(DISTDIR)/.distcheck/scratch-db/.githooks/pre-commit \
	    || { echo "the installed freeports-dev did not ship its templates" >&2; exit 1; }
	@test -f $(DISTDIR)/.distcheck/scratch-db/ci.yaml \
	    || { echo "the installed freeports-dev did not ship ci.template.yaml" >&2; exit 1; }
	@echo "Distributions install and answer in a clean environment."
	rm -rf $(DISTDIR)/.distcheck
