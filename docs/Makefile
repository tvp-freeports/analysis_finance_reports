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

# `--no-deps` because the site documents this crate, not the whole dependency tree.
rustdoc:
	$(CARGO) doc --no-deps --manifest-path $(CRATEDIR)/Cargo.toml --target-dir $(CARGODOCDIR)
	rm -rf "$(RUSTDOCDIR)"
	mkdir -p "$(RUSTDOCDIR)"
	cp -r $(CARGODOCDIR)/doc/. "$(RUSTDOCDIR)/"

clean-rustdoc:
	rm -rf "$(SOURCEDIR)/_extra"

.PHONY: help Makefile rustdoc clean-rustdoc

# Catch-all target: route all unknown targets to Sphinx using the new
# "make mode" option.  $(O) is meant as a shortcut for $(SPHINXOPTS).
%: Makefile
	@$(SPHINXBUILD) -M $@ "$(SOURCEDIR)" "$(BUILDDIR)" $(SPHINXOPTS) $(O)
