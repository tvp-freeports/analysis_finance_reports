//! Gettext catalog parsing and message lookup.
//!
//! Port of the *lookup* half of `packages/freeports_core/src/commons/i18n.rs`'s `Translator`
//! (see that reference file's module doc for why locale detection and `.mo` resource loading
//! stay outside Rust — those are OS/packaging concerns, same rationale as the two genuine
//! Python-boundary modules in `PLAN.md` §3, even though `i18n` itself is not one of them). All
//! PyO3 (`#[pyclass]`/`#[pymethods]`, `PyResult`, `PyValueError`) is dropped: a plain struct, a
//! plain `Result`, and a `thiserror` error type for the one genuinely fallible operation
//! (`Translator::new`) — per `PLAN.md` §14's "un enum d'errore per modulo, con `thiserror`".
//!
//! ## Expected API surface these tests assume
//!
//! - `Translator::new(mo_bytes: &[u8]) -> Result<Translator, E>` for some typed `E:
//!   std::error::Error` (name left to the implementer's discretion — the tests only assert
//!   `is_ok()`/`is_err()`, never a concrete error variant, so they don't couple to that choice).
//! - `Translator::gettext(&self, msg_id: &str) -> String` — falls back to `msg_id` itself when
//!   untranslated. This fallback is the entire point of the type: it must never panic or error
//!   on a missing translation, and the tests below assert it explicitly.

/// A parsed gettext catalog, ready for message lookup.
pub struct Translator {
    catalog: gettext::Catalog,
}

/// Failure mode of [`Translator::new`]: the given bytes are not a valid `.mo` catalog.
#[derive(Debug, thiserror::Error)]
#[error("invalid gettext catalog: {0}")]
pub struct TranslatorError(#[from] gettext::Error);

impl Translator {
    /// Parses a gettext catalog from raw `.mo` file bytes.
    pub fn new(mo_bytes: &[u8]) -> Result<Self, TranslatorError> {
        let catalog = gettext::Catalog::parse(mo_bytes)?;
        Ok(Translator { catalog })
    }

    /// Returns the translation of `msg_id`, or `msg_id` itself if no translation exists —
    /// exactly the fallback behavior of `gettext.NullTranslations.gettext` that the Python
    /// original relies on (untranslated strings degrade gracefully to the original text rather
    /// than erroring).
    pub fn gettext(&self, msg_id: &str) -> String {
        self.catalog.gettext(msg_id).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real Italian catalog originally shipped with `freeports_core`
    /// (`packages/freeports_core/python/freeports/_internals/locales/it/LC_MESSAGES/messages.mo`,
    /// compiled from `messages.po` by the existing gettext toolchain), copied verbatim into this
    /// crate's `testdata/` now that `freeports_core` itself is being removed — the tree it used
    /// to be reached from relatively no longer exists.
    const IT_MO: &[u8] = include_bytes!("testdata/messages.it.mo");

    mod catalog_parsing {
        use super::*;

        #[test]
        fn real_catalog_parses() {
            assert!(Translator::new(IT_MO).is_ok());
        }

        #[test]
        fn garbage_bytes_are_rejected() {
            // Verified empirically against the `gettext` crate (not assumed): parsing 5 random
            // bytes fails with an EOF-shaped error, not a panic.
            let garbage = [0u8, 1, 2, 3, 4];
            assert!(Translator::new(&garbage).is_err());
        }

        #[test]
        fn empty_bytes_are_rejected() {
            // Verified empirically: an empty byte slice has no valid `.mo` header and fails to
            // parse, same as `garbage_bytes_are_rejected`.
            assert!(Translator::new(&[]).is_err());
        }
    }

    mod lookup {
        use super::*;

        /// Verified directly against `messages.po` (not trusted from a stale comment):
        /// `msgid "URL of the dir where to find the pdf"` ->
        /// `msgstr "Indirizzo URL della risorsa pdf"`.
        #[test]
        fn known_translation_matches_messages_po() {
            let t = Translator::new(IT_MO).unwrap();
            assert_eq!(
                t.gettext("URL of the dir where to find the pdf"),
                "Indirizzo URL della risorsa pdf"
            );
        }

        #[test]
        fn missing_translation_falls_back_to_msgid() {
            let t = Translator::new(IT_MO).unwrap();
            let msgid = "Some string that definitely is not in the catalog";
            assert_eq!(t.gettext(msgid), msgid);
        }

        #[test]
        fn empty_msgid_does_not_panic() {
            // gettext convention: msgid "" holds the catalog's own metadata header, not a real
            // message. `gettext("")` must return *something* without panicking — the exact
            // content (the metadata block) is an implementation detail of the `gettext` crate,
            // not asserted here.
            let t = Translator::new(IT_MO).unwrap();
            let _ = t.gettext("");
        }
    }
}
