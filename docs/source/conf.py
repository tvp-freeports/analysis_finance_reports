"""Configuration of the Sphinx build.

Documentation strategy (step D1 of `packages/freeports/PLAN.md`, question Q-D2 closed on
2026-08-31): **a single Sphinx site**, with MyST so that new prose is written in Markdown, and
**rustdoc published alongside** as a sub-path rather than transcribed by hand into `.rst`.

The site covers three live Python packages — `freeports` (the PyO3 extension), `freeports_dev`,
`freeports_validate` — and defers to rustdoc for the crate's API. The `freeports_analysis` package
this file used to import until D1 has not existed for two rewrites.
"""

import importlib
import inspect
from importlib.metadata import PackageNotFoundError
from importlib.metadata import version as _installed_version
from pathlib import Path

_SOURCE_DIR = Path(__file__).parent

# -- Submodules of the compiled extension -------------------------------------
#
# `autosummary` decides whether to explore a module's children with `hasattr(obj, "__path__")`,
# and then lists them with `pkgutil.iter_modules(obj.__path__)`. Neither works on `freeports`,
# which is a PyO3 extension: there is a single `.so` on disk, and the submodules
# (`utils.text_filter`, `interfaces.pdf_blks`, `standard_funcs.deserialize`, ...) exist as module
# objects registered in `sys.modules` by the crate, not as files. Without this workaround the site
# would document 10 modules out of 18, losing along the way precisely the API a format author
# needs.
#
# The remedy lives entirely here, in the documentation build: `__path__ = []` is annotated onto the
# compiled modules that declare children in `__all__`. That way `autosummary` recognises them as
# packages and lists the children by the route that works — `__all__`, which the crate populates at
# every level — while `iter_modules([])`, having no directories to read, adds nothing. The crate is
# not touched, and neither is its runtime behaviour: the annotation lives only inside the
# `sphinx-build` process.


def _mark_compiled_subpackages(module, _seen=None):
    """Annotate as packages the compiled submodules that contain others."""
    seen = _seen if _seen is not None else set()
    if module.__name__ in seen:
        return
    seen.add(module.__name__)

    children = [
        value
        for _, value in inspect.getmembers(module, inspect.ismodule)
        if getattr(value, "__name__", "").startswith(f"{module.__name__}.")
    ]
    if children and not hasattr(module, "__path__"):
        module.__path__ = []
    for child in children:
        _mark_compiled_subpackages(child, seen)


for _documented_package in ("freeports", "freeports_dev", "freeports_validate"):
    try:
        _mark_compiled_subpackages(importlib.import_module(_documented_package))
    except ImportError:
        # A missing package is already reported by autodoc, with a better message than a
        # configuration error here could give.
        pass

# -- Project identity --------------------------------------------------------

project = "freeports"
copyright = "2025, Oreste Sciacqualegni"
author = "Oreste Sciacqualegni"

# Read from the installed package instead of written by hand: a version copied here goes stale in
# silence, and the documentation build imports the package for autodoc anyway.
#
# The import is aliased because Sphinx reads *every* module-level name in this file as a
# configuration option: an imported `version` would become the project's `version`, and writing
# `objects.inv` would die passing a function to `re.sub`.
try:
    release = _installed_version("freeports")
except PackageNotFoundError:
    release = "0.0.0+unknown"

version = release

# -- Internationalisation ----------------------------------------------------
#
# gettext scaffolding kept by explicit decision of the user (Q-D2): which languages are really
# maintained is an open question, but the mechanism stays wired up so that the answer does not
# require reassembling it. `docs/source/locales/` is untouched by D1.

locale_dirs = ["locales/"]
language = "en"
gettext_compact = False

# The `.mo` files are compiled and **versioned**. With automatic recompilation, every `make html`
# rewrites them and dirties the working copy with a couple of dozen modified binary files nobody
# asked to change — and those of `en` at that, which is the source language and translates nothing.
# Compiling the catalogues stays a deliberate act (`sphinx-intl build`, or this line set to `True`)
# rather than a side effect of building the site.
gettext_auto_build = False

# -- General configuration ---------------------------------------------------

extensions = [
    "sphinx_rtd_theme",
    "myst_parser",
    "sphinx.ext.autodoc",
    "sphinx.ext.intersphinx",
    "sphinx.ext.autosummary",
    "sphinx.ext.coverage",
    "sphinx.ext.napoleon",
]

templates_path = ["_templates"]
exclude_patterns = ["_extra"]
pygments_style = "sphinx"

# -- MyST --------------------------------------------------------------------
#
# The prose D3/D4 have yet to write goes in Markdown; the existing `.rst` files stay `.rst` and
# keep building, with no forced conversion.

myst_enable_extensions = [
    "colon_fence",
    "deflist",
    "fieldlist",
    "substitution",
]
myst_heading_anchors = 3

# -- autodoc / autosummary ---------------------------------------------------

autosummary_generate = True
napoleon_numpy_docstring = True

# Indispensable for `freeports` to be documented in full, not a cosmetic default. The package
# **is** the compiled extension: there is a single `.so` on disk, so the `pkgutil.iter_modules`
# autosummary uses to explore a package recursively finds one module and stops there. The real
# submodules (`core`, `utils`, `interfaces`, `standard_funcs`, ...) exist only as attributes of the
# parent module, and it is `__all__` that declares them — which the crate does at every level. By
# honouring `__all__`, autosummary collects them as imported members and the whole tree comes out:
# 17 modules instead of 2.
autosummary_ignore_module_all = False

intersphinx_mapping = {
    "python": ("https://docs.python.org/3", None),
}

# -- HTML output -------------------------------------------------------------

html_theme = "sphinx_rtd_theme"

# The side panel must be **the complete index of the site**, not a list of pages alone.
#
# `navigation_depth` counts levels from the root, and a page's sections are a level like any other:
# `whitepaper` -> `usage` -> `configuration` -> `options` already consumes four levels, which is
# the theme's default, so the sixteen options inside `options.md` did not appear at all. Six levels
# cover the deepest branch of the site plus its subsections.
#
# `collapse_navigation: False` removes the other half of the problem: with the default, branches
# that do not contain the current page are not expanded, so to *see* a section you had to have
# arrived at its page already — and getting there needed the in-text links. Now every branch can be
# opened from anywhere on the site.
html_theme_options = {
    "collapse_navigation": False,
    "navigation_depth": 6,
    "sticky_navigation": True,
    "titles_only": False,
}

html_logo = "https://www.freeports.org/assets/logo/square.svg"
html_static_path = ["_static"]
html_css_files = ["colors.css"]

# rustdoc is neither integrated nor duplicated: `make rustdoc` (or Read the Docs' build job)
# deposits `cargo doc --no-deps` into `_extra/rustdoc/`, and Sphinx copies it verbatim into the
# root of the site. The path is declared only if it really exists, so that a build without a Rust
# toolchain stays green instead of failing on a missing directory.
html_extra_path = [
    str(p.relative_to(_SOURCE_DIR)) for p in [_SOURCE_DIR / "_extra"] if p.is_dir()
]
