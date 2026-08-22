"""Internationalization module for locale detection and message translation.

This module handles locale detection and provides translation functionality
for the application. It supports both POSIX and Windows systems and falls
back to English if no suitable locale is found.

The gettext catalog parsing and message lookup are now implemented in Rust — see
``packages/freeports_engine/src/core/i18n.rs`` and
``analysis_finance_reports/agent-memory/rust-rewrite-plan.md``. Locale detection and locating
the packaged ``.mo`` resource file stay Python: they are OS/packaging concerns, the same
category as the pymupdf boundary in ``cli/main.py`` — Python already has the right tools for
"where does my installed package's data live" and "what locale is the OS in", Rust doesn't need
to reinvent them. Python reads the ``.mo`` file's bytes and hands them to the Rust
``Translator``; catalog parsing and lookup happen there.

The original ``_legacy_load_translation`` (the pre-Rust-port ``gettext.NullTranslations``-based
loader this module used to keep for reference) was moved to
``reference_legacy/_internals/commons/i18n.py`` during the maturin-idiomatic restructure (see
`agent-memory/maturin-idiomatic-restructure-plan.md`, §6b) — reference-only, never packaged.

Attributes
----------
LOC : Optional[str]
    Detected locale identifier
lang : str
    Language code extracted from locale
TRANSLATION : freeports._native.core.Translator
    Rust-backed translation object for the detected locale
_ : Callable[[str], str]
    Translation function for message strings
"""

import os
import locale
from importlib_resources import files

from freeports import _native

LOC = None
if os.name == "posix":
    LOC = locale.getlocale()[0]
elif os.name == "nt":
    LOC = locale.getlocale()[0][:2].lower()

if LOC is None:
    LOC = "en_US.UFT-8"
lang = LOC.split("_")[0]
if not lang or lang == "C":
    lang = "en"

_mo_bytes = (
    files("freeports._internals.locales") / lang / "LC_MESSAGES" / "messages.mo"
).read_bytes()
TRANSLATION = _native.core.Translator(_mo_bytes)
_ = TRANSLATION.gettext
