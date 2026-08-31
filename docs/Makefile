# Makefile della documentazione.
#
# `make html` costruisce il sito Sphinx. `make rustdoc` genera l'API del crate con `cargo doc`
# e la deposita in `source/_extra/rustdoc/`, da dove `html_extra_path` la copia tale e quale
# nella radice del sito: rustdoc viene pubblicato *accanto* al sito, non integrato in esso
# (strategia D1). Per il sito completo: `make rustdoc html`.

SPHINXOPTS    ?=
SPHINXBUILD   ?= sphinx-build
SOURCEDIR     = source
BUILDDIR      = build

CARGO         ?= cargo
CRATEDIR      = ../packages/freeports
# Directory di build separata da quella dei test: `cargo doc` non ripulisce `target/doc`,
# quindi condividerla farebbe finire nel sito la documentazione di dipendenze rimasta li'
# da qualche corsa precedente (`csv/`, `onig/`, ...) invece del solo crate.
CARGODOCDIR   = $(CRATEDIR)/target/docs-site
RUSTDOCDIR    = $(SOURCEDIR)/_extra/rustdoc

help:
	@$(SPHINXBUILD) -M help "$(SOURCEDIR)" "$(BUILDDIR)" $(SPHINXOPTS) $(O)
	@echo
	@echo "  rustdoc     genera l'API Rust in $(RUSTDOCDIR), pubblicata sotto /rustdoc/ nel sito"
	@echo "  clean-rustdoc  rimuove quella copia"

# `--no-deps` perche' il sito documenta questo crate, non l'intero albero delle dipendenze.
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
