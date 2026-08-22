"""Archived dead code, moved out of `python/freeports/_internals/commons/i18n.py` during the
maturin-idiomatic restructure session (2026-08-21) — see
`analysis_finance_reports/agent-memory/maturin-idiomatic-restructure-plan.md`, §6b. Reference-only,
never packaged (see this directory's own `reference_legacy/README.md`). Docstring below is
preserved verbatim from the live tree.

``_legacy_load_translation`` was the original `gettext.NullTranslations`-based loader, superseded
by the Rust-backed `Translator` (`src/core/i18n.rs`); the live file kept it as dead code pending
this move.
"""

from importlib_resources import files


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
