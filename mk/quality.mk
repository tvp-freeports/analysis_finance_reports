##@ Quality

# The quality surface on the same two axes the suites use: **`<what>` alone is everything,
# `<what>-<which>` is one slice.**
#
#     <what>          (all)              -rust                  -python                -formats
#     lint            both linters       clippy                 ruff                   ruff on content/
#     fmt             both               cargo fmt              ruff format            —
#     coverage        line coverage      cargo llvm-cov (SLOW)  pytest --cov           the document inventory
#     doc-coverage    both               rustdoc --show-coverage  the docstring walker —
#
# Every recipe here is a *measurement*: it answers one question and writes one normalised JSON file
# into `reports/`. None of them decides anything. What decides is `make ci-check`, in `mk/ci.mk`,
# which reads those files and `ci.yaml`. Keeping the two apart is what makes a threshold
# survivable — when a commit is refused, the number that refused it is in a file, produced by a
# command you can re-run by hand.

lint: lint-rust lint-python ## clippy on the crate, ruff on the Python sources

# Each of these runs its linter twice, and the second run is not waste. The first prints the
# diagnostics, which is what a person running `make lint` came for; the second is
# `freeports-dev lint-score`, which parses the machine-readable form and records the score into
# `reports/` for `ci-check` to judge. Both are cheap the second time round — clippy reads its own
# cache and ruff is sub-second over this tree.
#
# The alternative was to record nothing here and leave the measuring to a separate target. That is
# what this file did at first, and it was wrong in a way the gate caught within a day: `ci-fast`
# ran `lint`, nothing wrote a figure, and `ci-check` reported `lint.rust` as measured at whatever
# commit somebody had last run the recorder by hand. A metric that is only ever measured on purpose
# goes stale, and then it refuses a production commit for a reason unrelated to that commit.
lint-rust: ## clippy on the crate
	$(CARGO) clippy --manifest-path $(MANIFEST) --all-targets
	@$(FREEPORTS_DEV) lint-score --language rust --out --format none

lint-python: ## ruff on the Python sources
	$(RUFF) check $(PY_SOURCES)
	@$(FREEPORTS_DEV) lint-score --language python --out --format none

# A formats repository's `content/` is Python nobody has ever linted. REPO says which one.
lint-formats: ## ruff on a formats repository's content/: make lint-formats REPO=<path>
	@test -n "$(REPO)" || { \
	    echo "REPO is not set — for example:" >&2; \
	    echo "  make lint-formats REPO=../analysis_finance_reports_formats" >&2; \
	    exit 1; }
	$(FREEPORTS_DEV) lint-score --repo $(REPO) --out

fmt: fmt-rust fmt-python ## Reformat: cargo fmt and ruff format

fmt-rust: ## cargo fmt
	$(CARGO) fmt --manifest-path $(MANIFEST)

fmt-python: ## ruff format
	$(RUFF) format $(PY_SOURCES)

fmt-check: ## Verify formatting without rewriting anything
	$(CARGO) fmt --manifest-path $(MANIFEST) --check
	$(RUFF) format --check $(PY_SOURCES)

# ---------------------------------------------------------------------------
# Line coverage
# ---------------------------------------------------------------------------

coverage: coverage-python coverage-rust ## Line coverage of both languages (slow: see coverage-rust)

# **Slow, and the only slow measurement here.** It recompiles the crate instrumented, which is
# minutes, so it is never in the commit hook: the hook reads the figure from the last `make ci` and
# `ci-check` refuses on a prod branch when that figure was taken at another commit.
#
# It needs `venv/freeports-dev` active. Six tests in the crate's `python_boundary` submodules
# import PyMuPDF through PyO3, and without the environment they panic, the run exits 101, and
# llvm-cov writes *no report at all* — which `ci-check` then reports as unmeasured rather than as a
# pass, because a figure nobody could compute is not a figure above the threshold.
coverage-rust: ## Line coverage of the crate (cargo llvm-cov) — minutes, needs the venv active
	# Asked of cargo, not of `PATH`. The recipe below invokes `cargo llvm-cov`, so what has to be
	# true is that *cargo* can find the subcommand — and testing `command -v cargo-llvm-cov`
	# instead is a different question with a different answer. On this machine `PATH` contains a
	# literal `~/.cargo/bin`: bash resolves the tilde when it searches, `sh` does not, and make
	# uses `sh`. The guard therefore refused a target whose tool was installed and working.
	@$(CARGO) llvm-cov --version >/dev/null 2>&1 || { \
	    echo "cargo llvm-cov is not available. Run: make dev-ci" >&2; exit 1; }
	mkdir -p $(REPORTS)
	$(CARGO) llvm-cov --manifest-path $(MANIFEST) --lib --json --summary-only \
	    --output-path $(REPORTS)/llvm-cov.json
	$(FREEPORTS_DEV) ci-record --metric tests.rust.lines --from llvm-cov \
	    --input $(REPORTS)/llvm-cov.json

# Per package, and the aggregate over both, in the shape `test-python` already uses: one pytest
# invocation per package, because each carries its own rootdir configuration.
coverage-python: ## Line coverage of the tooling packages (pytest --cov)
	mkdir -p $(REPORTS)
	$(PYTEST) $(PKG_DEV) --cov=freeports_dev \
	    --cov-report=json:$(REPORTS)/coverage-freeports_dev.json -q
	$(PYTEST) $(PKG_VALID) --cov=freeports_validate \
	    --cov-report=json:$(REPORTS)/coverage-freeports_validate.json -q
	$(FREEPORTS_DEV) ci-record --metric tests.python.freeports_dev.lines --from coverage-py \
	    --input $(REPORTS)/coverage-freeports_dev.json
	$(FREEPORTS_DEV) ci-record --metric tests.python.freeports_validate.lines --from coverage-py \
	    --input $(REPORTS)/coverage-freeports_validate.json
	$(FREEPORTS_DEV) ci-record --metric tests.python.lines --from coverage-py \
	    --input $(REPORTS)/coverage-freeports_dev.json \
	    --input $(REPORTS)/coverage-freeports_validate.json

coverage-formats: ## The document inventory of a formats repository: make coverage-formats REPO=<path>
	@test -n "$(REPO)" || { \
	    echo "REPO is not set — for example:" >&2; \
	    echo "  make coverage-formats REPO=../analysis_finance_reports_formats" >&2; \
	    exit 1; }
	$(FREEPORTS_DEV) coverage --repo $(REPO) --out

# ---------------------------------------------------------------------------
# Documentation coverage
# ---------------------------------------------------------------------------

doc-coverage: doc-coverage-python doc-coverage-rust ## Documented items, both languages

doc-coverage-rust: ## Documented items of the crate (rustdoc --show-coverage), needs nightly
	@rustup toolchain list 2>/dev/null | grep -q nightly || { \
	    echo "The nightly toolchain is not installed. Run: make dev-ci" >&2; exit 1; }
	mkdir -p $(REPORTS)
	$(CARGO) +nightly rustdoc --manifest-path $(MANIFEST) --lib -- \
	    -Zunstable-options --show-coverage --output-format json >/dev/null
	$(FREEPORTS_DEV) ci-record --metric docs.rust --from rustdoc \
	    --input $(CRATEDIR)/target/doc/freeports.json

# The *gated* documentation metric, and deliberately not sphinx's number — see `docs-site-coverage`
# in `mk/docs.mk` for what that one measures and why a threshold on it would be a threshold on how
# autosummary is configured.
doc-coverage-python: ## Public Python objects carrying a docstring
	$(FREEPORTS_DEV) doc-coverage --out
