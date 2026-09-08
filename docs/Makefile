# Makefile of the documentation.
#
# `make html` builds the Sphinx site. `make rustdoc` generates the crate's API with `cargo doc`
# and deposits it in `source/_extra/rustdoc/`, from where `html_extra_path` copies it verbatim
# into the root of the site: rustdoc is published *beside* the site, not integrated into it
# (strategy D1). For the complete site: `make rustdoc html`.
#
# The root Makefile delegates here — `make docs`, `make docs-html`, `make docs-rustdoc`,
# `make docs-site-coverage`, `make i18n-extract` — passing the `SPHINXBUILD` and `CARGO` of the
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
# `html_extra_path` to copy into the site.
#
# Unlike rustdoc beside it, this page is **committed** rather than rebuilt with the site, and the
# reason is that not every machine can compute it correctly. Signature validity comes from the
# keyring of whoever runs the command, so a builder with no public keys -- a Read the Docs runner,
# a fresh container, a contributor who has never imported anybody's key -- produces a page saying
# every grant in the repository is invalid. That is a false statement about real people, and the
# site must not publish it. So the page is written where the keys are, by the commit hook, and
# every build after that only copies it.
#
# Which leaves this target for the person deliberately regenerating it. It never fails a build, and
# it never overwrites a page that is already there: a run that could not resolve the methodologies
# leaves the committed page alone, because one run out of date is a better page than no page. The
# placeholder is written only when there is nothing to keep.
validation-coverage:
	@mkdir -p "$(dir $(COVERAGEHTML))"
	@$(VALIDATE) --repo "$(REPO)" $(VALIDATE_SOURCE_ARGS) \
	    report --format html --out "$(COVERAGEHTML).new" \
	    && mv -f "$(COVERAGEHTML).new" "$(COVERAGEHTML)" \
	    || { rm -f "$(COVERAGEHTML).new"; \
	        echo "freeports-validate: the report could not be generated" >&2; \
	        if [ -f "$(COVERAGEHTML)" ]; then \
	            echo "  the committed page is left as it is" >&2; \
	        else \
	            printf '%s\n' \
	                '<!doctype html><meta charset="utf-8">' \
	                '<title>Validation coverage — unavailable</title>' \
	                '<h1>Validation coverage</h1>' \
	                '<p>This page is generated from the signed documents in <code>validation/</code>' \
	                'by whoever holds the contributors public keys, and no such rendering was' \
	                'available when this site was built.</p>' \
	                '<p>The claims themselves are unaffected. To see them, run' \
	                '<code>freeports-validate check-grants</code> in a clone of the repository.</p>' \
	                > "$(COVERAGEHTML)"; \
	        fi; }

clean-validation-coverage:
	rm -f "$(COVERAGEHTML)"

.PHONY: help Makefile rustdoc clean-rustdoc validation-coverage clean-validation-coverage

# Catch-all target: route all unknown targets to Sphinx using the new
# "make mode" option.  $(O) is meant as a shortcut for $(SPHINXOPTS).
%: Makefile
	@$(SPHINXBUILD) -M $@ "$(SOURCEDIR)" "$(BUILDDIR)" $(SPHINXOPTS) $(O)
