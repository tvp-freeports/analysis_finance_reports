# Environment and installation: who you are, and the one command that sets you up.
#
# The taxonomy is by *role* rather than by package, because "which of these eleven things do I
# need" is a question nobody should have to answer to start working. `make doctor` in the root
# Makefile says what is missing and which target here supplies it.

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

# The measurement tools, which are neither an engine dependency nor a documentation one — they are
# needed only by whoever is producing the numbers `ci-check` judges. `dev-engine` deliberately does
# not pull them in: somebody fixing a parser should not have to wait for an instrumented rebuild to
# be installable.
#
# `doctor` reports each of these by name, which is what keeps an `unmeasured` verdict an
# explanation rather than a mystery.
install-ci-deps: venv ## The measurement tools: cargo-llvm-cov, llvm-tools, nightly, pytest-cov
	$(PIP) install pytest-cov
	rustup component add llvm-tools-preview
	rustup toolchain install nightly --profile minimal
	cargo install cargo-llvm-cov --locked

dev-ci: install-ci-deps ## Measuring: coverage, documentation coverage, lint scores
	@echo "Measurement tools ready. Everything at once: make ci"

dev-engine: install-dev-deps develop install-binary githooks ## Working on the engine
	@echo "Engine environment ready. Suite: make check"

dev-formats: install install-tools ## Writing formats
	@echo "Formats environment ready. Test a repository: make test-formats REPO=<path>"

# autodoc really imports the documented packages, it does not mock them: building the site requires
# all three to be installed, extension included.
dev-docs: install install-tools install-docs-deps ## Writing and translating documentation
	@echo "Documentation environment ready. Site: make docs"

dev-all: install-dev-deps develop install-binary install-tools install-docs-deps install-ci-deps githooks ## Everything
	@echo "Complete environment."

uninstall: ## Remove the distributions and the command from the active environment
	-$(PIP) uninstall -y freeports freeports-dev freeports-validate
	-$(PIP) uninstall -y freeports_analysis
	rm -f "$(DESTDIR)$(bindir)/freeports$(EXE)"

reinstall: uninstall dev-all ## Uninstall and reinstall everything

