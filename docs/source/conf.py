"""Configurazione della build Sphinx.

Strategia documentale (passo D1 di `packages/freeports/PLAN.md`, domanda Q-D2 chiusa il
2026-08-31): **un solo sito Sphinx**, con MyST perche' la prosa nuova si scriva in Markdown,
e **rustdoc pubblicato accanto** come sotto-percorso invece che duplicato a mano in `.rst`.

Il sito copre tre pacchetti Python vivi — `freeports` (l'estensione PyO3), `freeports_dev`,
`freeports_validate` — e rimanda a rustdoc per l'API del crate. Il pacchetto `freeports_analysis`
che questo file importava fino a D1 non esiste piu' da due riscritture.
"""

import importlib
import inspect
from importlib.metadata import PackageNotFoundError
from importlib.metadata import version as _installed_version
from pathlib import Path

_SOURCE_DIR = Path(__file__).parent

# -- Sottomoduli dell'estensione compilata -----------------------------------
#
# `autosummary` decide se esplorare i figli di un modulo con `hasattr(obj, "__path__")`, e poi
# li elenca con `pkgutil.iter_modules(obj.__path__)`. Nessuna delle due cose funziona su
# `freeports`, che e' un'estensione PyO3: sul disco c'e' un unico `.so`, e i sottomoduli
# (`utils.text_filter`, `interfaces.pdf_blks`, `standard_funcs.deserialize`, ...) esistono
# come oggetti-modulo registrati in `sys.modules` dal crate, non come file. Senza questo
# accorgimento il sito documenterebbe 10 moduli su 18, perdendo per strada proprio l'API che
# serve a chi scrive un formato.
#
# Il rimedio sta tutto qui, nella build della documentazione: si annota `__path__ = []` sui
# moduli compilati che dichiarano figli in `__all__`. Cosi' `autosummary` li riconosce come
# package ed elenca i figli dalla via che funziona — `__all__`, che il crate popola a ogni
# livello — mentre `iter_modules([])`, non avendo cartelle da leggere, non aggiunge nulla.
# Il crate non viene toccato, e il suo comportamento a runtime nemmeno: l'annotazione vive
# solo nel processo di `sphinx-build`.


def _mark_compiled_subpackages(module, _seen=None):
    """Annota come package i sottomoduli compilati che ne contengono altri."""
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
        # Un pacchetto assente e' gia' segnalato da autodoc, con un messaggio migliore di
        # quello che potrebbe dare un errore di configurazione qui.
        pass

# -- Identita' del progetto --------------------------------------------------

project = "freeports"
copyright = "2025, Oreste Sciacqualegni"
author = "Oreste Sciacqualegni"

# Letta dal pacchetto installato invece che scritta a mano: una versione copiata qui invecchia
# in silenzio, e la build della documentazione importa comunque il pacchetto per autodoc.
#
# L'import e' aliasato perche' Sphinx legge *ogni* nome di livello modulo di questo file come
# un'opzione di configurazione: un `version` importato diventerebbe il `version` del progetto,
# e la scrittura di `objects.inv` morirebbe passando una funzione a `re.sub`.
try:
    release = _installed_version("freeports")
except PackageNotFoundError:
    release = "0.0.0+unknown"

version = release

# -- Internazionalizzazione --------------------------------------------------
#
# Impalcatura gettext mantenuta per decisione esplicita dell'utente (Q-D2): quali lingue
# davvero si mantengano e' una domanda aperta, ma il meccanismo resta cablato in modo che la
# risposta non richieda di rimontarlo. `docs/source/locales/` non e' toccato da D1.

locale_dirs = ["locales/"]
language = "en"
gettext_compact = False

# I `.mo` sono compilati e **versionati**. Con la ricompilazione automatica, ogni `make html`
# li riscrive e sporca la copia di lavoro con una ventina di file binari modificati che nessuno
# ha chiesto di cambiare — per giunta quelli di `en`, che e' la lingua sorgente e non traduce
# nulla. La compilazione dei cataloghi resta un gesto deliberato (`sphinx-intl build`, o questa
# riga a `True`) invece di un effetto collaterale della build del sito.
gettext_auto_build = False

# -- Configurazione generale -------------------------------------------------

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
# La prosa che D3/D4 devono ancora scrivere va in Markdown; i `.rst` esistenti restano `.rst`
# e continuano a costruire, senza conversione forzata.

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

# Indispensabile perche' `freeports` sia documentato per intero, e non un default cosmetico.
# Il pacchetto **e'** l'estensione compilata: sul disco c'e' un solo `.so`, quindi il
# `pkgutil.iter_modules` con cui autosummary esplora un package ricorsivamente trova un unico
# modulo e si ferma li'. I sottomoduli veri (`core`, `utils`, `interfaces`, `standard_funcs`,
# ...) esistono solo come attributi del modulo genitore, ed e' `__all__` a dichiararli — cosa
# che il crate fa a ogni livello. Rispettando `__all__`, autosummary li raccoglie come membri
# importati e l'albero viene fuori tutto: 17 moduli invece di 2.
autosummary_ignore_module_all = False

intersphinx_mapping = {
    "python": ("https://docs.python.org/3", None),
}

# -- Output HTML -------------------------------------------------------------

html_theme = "sphinx_rtd_theme"
html_logo = "https://www.freeports.org/assets/logo/square.svg"
html_static_path = ["_static"]
html_css_files = ["colors.css"]

# rustdoc non e' integrato ne' duplicato: `make rustdoc` (o il job di build di Read the Docs)
# deposita `cargo doc --no-deps` in `_extra/rustdoc/`, e Sphinx lo copia tale e quale nella
# radice del sito. Il percorso e' dichiarato solo se esiste davvero, cosi' una build senza
# toolchain Rust resta verde invece di fallire su una cartella assente.
html_extra_path = [
    str(p.relative_to(_SOURCE_DIR)) for p in [_SOURCE_DIR / "_extra"] if p.is_dir()
]
