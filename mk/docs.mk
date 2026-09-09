# Documentation: the Sphinx site, the rustdoc it embeds, and the translation catalogues.
#
# The one recursive `$(MAKE)` in this build lives here, in `docs/Makefile`. That file is Sphinx's
# own generated one and belongs to Sphinx; wrapping it would mean maintaining a copy of something
# `sphinx-quickstart` regenerates.

##@ Documentation

# `docs-validation` is deliberately **not** here. The coverage page is committed, so building the
# site only has to copy it; regenerating it on a machine whose keyring lacks the contributors'
# public keys would rewrite it to say every signature is invalid, which is exactly what it must
# never say on somebody else's behalf.
docs: docs-rustdoc docs-html ## The whole site, rustdoc included
	@echo "Built. Read it with 'make docs-serve' — http://localhost:$(DOCS_PORT)"
	@echo "(opening docs/build/html/index.html as a file:// path leaves the search box"
	@echo " stuck on \"Searching\": a file origin may not fetch the search index.)"

docs-html: ## Sphinx only — what you want while writing prose
	$(MAKE) -C docs html SPHINXBUILD="$(SPHINXBUILD)"

docs-rustdoc: ## cargo doc only, deposited in docs/source/_extra/rustdoc/
	$(MAKE) -C docs rustdoc CARGO="$(CARGO)"

# The coverage page on its own, for whoever is regenerating it deliberately. It is written by
# `validation-report` along with everything else, and this target exists for the case where only
# the page is wanted.
#
# Run it only if your keyring holds the public keys of the contributors whose documents it reads.
# Signature validity is computed from that keyring, so a run without them writes a page claiming
# the grants are broken -- see `make check-grants`, which explains the same failure at length.
docs-validation: ## The validation coverage page only, into docs/source/_extra/validation/
	$(MAKE) -C docs validation-coverage \
	    VALIDATE="$(ENV_BIN)/freeports-validate$(EXE)" VALIDATE_SOURCES="$(VALIDATE_SOURCES)"

# Renamed from `docs-coverage`, because that name claimed something this target does not measure.
#
# `sphinx.ext.coverage` measures whether an object **appears in the built site**, which is a fact
# about how `autosummary` is configured rather than about whether anything is documented. Two
# things follow from reading its source, and both make it unusable as a threshold: every module of
# the compiled `freeports` extension reports 100.00 % because `inspect.getmembers` attributes
# nothing to it — a vacuous 100, not a perfect one — and its TOTAL line is a union over object
# names, which is why it reads 25.93 % while nearly every row above it reads 100 %.
#
# It is still a real question worth asking about the *site*, so the target stays. The **gated**
# documentation metric is `make doc-coverage-python` in `mk/quality.mk`, which counts docstrings
# with an AST walk and is the exact counterpart of what `rustdoc --show-coverage` counts.
#
# The report ends up in docs/build/coverage/python.txt.
docs-site-coverage: ## What fraction of the API appears in the built site (not docstring coverage)
	$(MAKE) -C docs coverage SPHINXBUILD="$(SPHINXBUILD)"
	@echo "Report at docs/build/coverage/python.txt"
	@echo "This measures the site, not docstrings. For the gated figure: make doc-coverage-python"

html: docs-html ## The HTML site (canonical GNU name, alias of docs-html)

# Depends on `i18n-build`, and that dependency is the whole reason this line is not one command.
# Sphinx reads the compiled `.mo` files, never the `.po` a translator edits, so building a language
# without recompiling first silently produces the *English* page for every string touched since the
# last compile -- a failure with no error message, which is the worst kind to leave lying around.
docs-lang: i18n-build ## Build a single language: make docs-lang DOCLANG=it
	$(SPHINXBUILD) -b html -D language=$(DOCLANG) docs/source docs/build/$(DOCLANG)

# `DOCLANG=` (empty) serves the English site in docs/build/html; any other value serves the
# language built by `docs-lang`, so reading a translation is the same command as reading the
# original rather than a path to remember.
docs-serve: ## Serve the built site locally (DOCS_PORT=8000, DOCLANG=it for a translation)
	$(PYTHON) -m http.server $(DOCS_PORT) --directory docs/build/$(if $(DOCLANG),$(DOCLANG),html)

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

i18n-stat: ## How far each language has got: translated / fuzzy / untranslated per page
	$(SPHINXINTL) stat -d docs/source/locales

# The step `sphinx-intl update` cannot take. It merges the current strings into the catalogues and
# marks what has gone stale *inside* a file, but a `.po` belonging to a page that no longer exists
# is not stale, it is orphaned: nothing points at it, `stat` still counts it, and a translator can
# spend an afternoon on a page nobody will ever build. Reorganising the site produces these by the
# dozen, so removing them is a step rather than a chore.
#
# Deliberately compared against `docs/build/gettext`, which is what `i18n-extract` has just
# written: a catalogue is orphaned when there is no `.pot` for it, and only an extraction from the
# current sources can say that.
i18n-prune: ## Delete catalogues whose page no longer exists (run after i18n-extract)
	@if [ ! -d docs/build/gettext ]; then \
	    echo "docs/build/gettext is missing -- run 'make i18n-extract' first." >&2; exit 1; \
	 fi
	@find docs/source/locales -name '*.po' | while read -r po; do \
	    page=$${po#docs/source/locales/}; page=$${page#*/LC_MESSAGES/}; \
	    if [ ! -f "docs/build/gettext/$${page%.po}.pot" ]; then \
	        echo "orphaned, removing: $$po"; rm -f "$$po" "$${po%.po}.mo"; \
	    fi; \
	 done
	@find docs/source/locales -type d -empty -delete 2>/dev/null || true

