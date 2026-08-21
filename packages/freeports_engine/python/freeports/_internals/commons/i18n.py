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

Attributes
----------
LOC : Optional[str]
    Detected locale identifier
lang : str
    Language code extracted from locale
TRANSLATION : freeports_engine.core.Translator
    Rust-backed translation object for the detected locale
_ : Callable[[str], str]
    Translation function for message strings
"""

import os
import locale
from importlib_resources import files

import freeports_engine

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
TRANSLATION = freeports_engine.core.Translator(_mo_bytes)
_ = TRANSLATION.gettext


def _legacy_load_translation(lang: str):
    """Dead code: the original `gettext.NullTranslations`-based loader, superseded by the
    Rust-backed `Translator` above. Kept until the migration is far enough along to delete it.

    Unlike the code above, this also called `.install()`, injecting `_` into `builtins` — grepped
    for and confirmed unused (every real call site does `from freeports.i18n import _`
    explicitly), so the Rust-backed replacement above doesn't bother.
    """
    import tempfile
    import gettext as _gettext_module
    from pathlib import Path

    with tempfile.TemporaryDirectory() as tmp_dir:
        for f in (
            files("freeports._internals.locales") / lang / "LC_MESSAGES"
        ).iterdir():
            translation_dir = Path(tmp_dir) / lang / "LC_MESSAGES"
            translation_dir.mkdir(parents=True, exist_ok=True)
            tmp_file = translation_dir / f.name
            tmp_file.write_bytes(f.read_bytes())
        translation = _gettext_module.translation("messages", tmp_dir, [lang])
        translation.install()
        return translation
