//! Gettext catalog parsing and message lookup.
//!
//! Rust port of the *lookup* half of
//! `packages/freeports_core/src/freeports/_internals/commons/i18n.py`. Uses the pure-Rust
//! `gettext` crate (parses the same `.mo` binary format as GNU gettext, no C dependency) — it
//! reads the exact `.mo` catalogs already maintained under `_internals/locales/<lang>/LC_MESSAGES/`,
//! so no translation content needs to change.
//!
//! **Deliberately NOT ported**: locale detection (`locale.getlocale()`, POSIX vs. Windows) and
//! locating/reading the packaged `.mo` resource file (`importlib_resources`). Both stay Python —
//! they're OS/packaging concerns (same category as the pymupdf boundary: Python already has the
//! right tools for "where does my installed package's data live", Rust doesn't need to
//! reinvent them). Python reads the `.mo` file's bytes and hands them to [`Translator::new`];
//! everything downstream (catalog parsing, message lookup) is Rust.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// A parsed gettext catalog, ready for message lookup.
#[pyclass]
pub struct Translator {
    catalog: gettext::Catalog,
}

#[pymethods]
impl Translator {
    /// Parses a gettext catalog from raw `.mo` file bytes. Raises `ValueError` on an invalid or
    /// corrupt catalog.
    #[new]
    fn new(mo_bytes: &[u8]) -> PyResult<Self> {
        let catalog = gettext::Catalog::parse(mo_bytes)
            .map_err(|e| PyValueError::new_err(format!("invalid gettext catalog: {e}")))?;
        Ok(Translator { catalog })
    }

    /// Returns the translation of `msg_id`, or `msg_id` itself if no translation exists —
    /// exactly the fallback behavior of `gettext.NullTranslations.gettext` that the Python
    /// original relies on (untranslated strings degrade gracefully to the original text rather
    /// than erroring).
    fn gettext(&self, msg_id: &str) -> String {
        self.catalog.gettext(msg_id).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real Italian catalog already shipped alongside this crate's own `python/freeports/`
    /// source (formerly under the separate `freeports_core` package, before the
    /// freeports_core -> freeports_engine consolidation — see
    /// `analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md`) — testing
    /// against an actual production `.mo` file (compiled from `messages.po` by the existing
    /// gettext toolchain) is far more trustworthy than a hand-crafted byte array whose header
    /// field layout I could easily get subtly wrong without a real `msgfmt`/`gettext.translation`
    /// to cross-check against right now.
    const IT_MO: &[u8] = include_bytes!(
        "../../python/freeports/_internals/locales/it/LC_MESSAGES/messages.mo"
    );

    #[test]
    fn real_catalog_parses() {
        assert!(Translator::new(IT_MO).is_ok());
    }

    #[test]
    fn known_translation_matches_messages_po() {
        // From messages.po: msgid "URL of the dir where to find the pdf" ->
        // msgstr "Indirizzo URL della risorsa pdf".
        let t = Translator::new(IT_MO).unwrap();
        assert_eq!(
            t.gettext("URL of the dir where to find the pdf"),
            "Indirizzo URL della risorsa pdf"
        );
    }

    #[test]
    fn missing_translation_falls_back_to_msgid() {
        let t = Translator::new(IT_MO).unwrap();
        assert_eq!(
            t.gettext("Some string that definitely is not in the catalog"),
            "Some string that definitely is not in the catalog"
        );
    }

    #[test]
    fn garbage_bytes_are_rejected() {
        let garbage = [0u8, 1, 2, 3, 4];
        assert!(Translator::new(&garbage).is_err());
    }
}
