# Makefile of the documentation.
#
# `make html` builds the Sphinx site. `make rustdoc` generates the crate's API with `cargo doc`
# and deposits it in `source/_extra/rustdoc/`, from where `html_extra_path` copies it verbatim
# into the root of the site: rustdoc is published *beside* the site, not integrated into it
# (strategy D1). For the complete site: `make rustdoc html`.
#
# The root Makefile delegates here — `make docs`, `make docs-html`, `make docs-rustdoc`,
# `make docs-coverage`, `make i18n-extract` — passing the `SPHINXBUILD` and `CARGO` of the
# environment in use.

SPHINXOPTS    ?=
SPHINXBUILD   ?= sphinx-build
SOURCEDIR     = source
BUILDDIR      = build

VALIDATE      ?= freeports-validate
REPO          = ..
# Kept in step with the root Makefile, which passes its own value down. The default is repeated
# rather than referenced because Read the Docs invokes this file directly, and because the two
# documentation channels do not carry the same methodology pages: `latest` is this branch, `stable`
# is the last release, and a run that read only `stable` would call every grant here a mismatch.
VALIDATE_SOURCES ?= https://docs.freeports.org/en/latest/_sources/validation/*.rst.txt \
                    https://docs.freeports.org/en/stable/_sources/validation/*.rst.txt
VALIDATE_SOURCE_ARGS = $(foreach source,$(VALIDATE_SOURCES),--source "$(source)")
COVERAGEHTML  = $(SOURCEDIR)/_extra/validation/coverage.html

CARGO         ?= cargo
CRATEDIR      = ../packages/freeports
# A build directory separate from the tests' one: `cargo doc` does not clean out `target/doc`, so
# sharing it would put into the site the documentation of dependencies left there by some earlier
# run (`csv/`, `onig/`, ...) instead of the crate alone.
CARGODOCDIR   = $(CRATEDIR)/target/docs-site
RUSTDOCDIR    = $(SOURCEDIR)/_extra/rustdoc

help:
	@$(SPHINXBUILD) -M help "$(SOURCEDIR)" "$(BUILDDIR)" $(SPHINXOPTS) $(O)
	@echo
	@echo "  rustdoc     generates the Rust API in $(RUSTDOCDIR), published under /rustdoc/ on the site"
	@echo "  clean-rustdoc  removes that copy"
	@echo "  validation-coverage  renders the grants as $(COVERAGEHTML), published under /validation/"

# `--no-deps` because the site documents this crate, not the whole dependency tree.
rustdoc:
	$(CARGO) doc --no-deps --manifest-path $(CRATEDIR)/Cargo.toml --target-dir $(CARGODOCDIR)
	rm -rf "$(RUSTDOCDIR)"
	mkdir -p "$(RUSTDOCDIR)"
	cp -r $(CARGODOCDIR)/doc/. "$(RUSTDOCDIR)/"

clean-rustdoc:
	rm -rf "$(SOURCEDIR)/_extra"

# The grants, as one page with the three viewpoints as tabs, deposited in `_extra/` for
# `html_extra_path` to copy into the site — the same arrangement as rustdoc, and for the same
# reason: it is generated, so it is a product of the build rather than a file in the tree.
#
# Never fatal. The figures depend on fetching every methodology page from the configured sources,
# and a documentation build must not fail because a documentation server was down. What it must not
# do either is leave the link from `validation/report/index.rst` pointing at nothing, so a build
# that could not collect writes a page saying exactly that instead of no page at all.
validation-coverage:
	@mkdir -p "$(dir $(COVERAGEHTML))"
	@$(VALIDATE) --repo "$(REPO)" $(VALIDATE_SOURCE_ARGS) \
	    report --format html --out "$(COVERAGEHTML)" || { \
	    echo "freeports-validate: the report could not be generated for this build" >&2; \
	    printf '%s\n' \
	        '<!doctype html><meta charset="utf-8">' \
	        '<title>Validation coverage — unavailable</title>' \
	        '<h1>Validation coverage</h1>' \
	        '<p>This page is generated at build time from the signed documents in' \
	        '<code>validation/</code>, and this build could not read them — most often because' \
	        'the methodology pages could not be fetched from their configured sources.</p>' \
	        '<p>The claims themselves are unaffected. To see them, run' \
	        '<code>freeports-validate check-grants</code> in a clone of the repository.</p>' \
	        > "$(COVERAGEHTML)"; }

clean-validation-coverage:
	rm -f "$(COVERAGEHTML)"

.PHONY: help Makefile rustdoc clean-rustdoc validation-coverage clean-validation-coverage

# Catch-all target: route all unknown targets to Sphinx using the new
# "make mode" option.  $(O) is meant as a shortcut for $(SPHINXOPTS).
%: Makefile
	@$(SPHINXBUILD) -M $@ "$(SOURCEDIR)" "$(BUILDDIR)" $(SPHINXOPTS) $(O)
