//! Pipe `deserialize` standard — sottoinsieme autosufficiente di
//! `freeports_core/src/formats_utils/deserialize/standard_funcs.rs`.
//!
//! Scope deciso dall'utente (`agent-memory/M4-implementation-plan.md` §0, opzione A): solo
//! `DeserializerPageClassifyStandard` e' costruibile senza `output::classes` (M8) — le altre
//! (`DeserializeSfdrArticleStandard`, `DeserializerFundStandard`,
//! `DeserializerManagmentCompanyStandard`, `DeserializerInvestmentsManagerFromManco`,
//! `DeserializerInvestmentsManagerStandard`) costruiscono entita' che non esistono ancora.
//! Dopo la chiusura di M5 questa e' l'**unica** dipendenza che tiene aperta M4: nessuna di queste
//! aspetta piu' il motore.
//!
//! Da M5 `DeserializerPageClassifyStandard` implementa anche
//! [`DeserializePipe`](crate::core::pipeline::DeserializePipe): `call` resta l'API diretta che
//! restituisce il `BlockValue` grezzo, `call_page_class` lo traduce nella
//! [`PageClass`](crate::core::schedule::PageClass) tipizzata, e il trait e' la forma che il
//! motore usa.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! pub struct DeserializerPageClassifyStandard;
//! impl DeserializerPageClassifyStandard {
//!     pub fn call(&self, txt_blk: &TextBlock) -> Result<BlockValue, DeserializeStandardFuncsError>;
//! }
//!
//! #[derive(Debug, thiserror::Error)]
//! pub enum DeserializeStandardFuncsError { /* un enum locale, stesso trattamento provvisorio di
//!     CommonsError (M3, pdf_extract::commons) — verra' assorbito da PipeError in M5 */ }
//! ```
//!
//! `call` legge `txt_blk.metadata["page_type"]` e lo restituisce cosi' com'e' — anche
//! `BlockValue::Null`, che e' un `Ok`, non un errore (un `TextBlock` di
//! `TextFilterPageClassifyStandard` porta sempre quella chiave, valorizzata a `Null` quando
//! nessun blocco pdf era classificato). Il riferimento Python legge il campo con un subscript
//! (`metadata["page_type"]`), che solleva se la chiave manca del tutto — qui l'equivalente e'
//! `metadata_or_fail("page_type")`: una chiave **assente** (non semplicemente valorizzata a
//! `Null`) e' quindi un `Err`, non un `Ok(BlockValue::Null)`.

use crate::core::classes::TextBlock;
use crate::core::classes::value::{BlockValue, BlockValueError};
use crate::core::pipeline::{DeserializePipe, Extracted, PipeError};
use crate::core::schedule::PageClass;

#[derive(Debug, thiserror::Error)]
pub enum DeserializeStandardFuncsError {
    #[error(transparent)]
    Value(#[from] BlockValueError),
    #[error("page_type is a {found}, not a string naming a page class")]
    PageTypeNotAString { found: &'static str },
}

impl DeserializeStandardFuncsError {
    /// Traduzione nell'errore del motore. Il nome del pipe non è ricavabile dall'errore, quindi
    /// lo passa il chiamante — stessa forma di [`PipeError::from_commons`].
    pub fn into_pipe_error(self, pipe: &str) -> PipeError {
        match self {
            DeserializeStandardFuncsError::Value(source) => PipeError::value(pipe, source),
            other @ DeserializeStandardFuncsError::PageTypeNotAString { .. } => {
                PipeError::extraction(pipe, other.to_string())
            }
        }
    }
}

pub struct DeserializerPageClassifyStandard;

impl DeserializerPageClassifyStandard {
    pub fn call(&self, txt_blk: &TextBlock) -> Result<BlockValue, DeserializeStandardFuncsError> {
        Ok(txt_blk.metadata_or_fail("page_type")?.clone())
    }

    /// Il `page_type` letto da [`DeserializerPageClassifyStandard::call`], tradotto nella page
    /// class tipizzata che il motore si aspetta.
    ///
    /// `BlockValue::Null` è la classificazione "nessuna class" — un `Ok(None)`, non un errore:
    /// `TextFilterPageClassifyStandard` mette sempre quella chiave, valorizzata a `Null` quando
    /// nessun blocco della pagina era classificato. Qualunque altro tipo è invece un errore di
    /// configurazione del repo formati.
    pub fn call_page_class(
        &self,
        txt_blk: &TextBlock,
    ) -> Result<Option<PageClass>, DeserializeStandardFuncsError> {
        match self.call(txt_blk)? {
            BlockValue::Null => Ok(None),
            BlockValue::Str(name) => Ok(Some(PageClass::new(name))),
            other => {
                Err(DeserializeStandardFuncsError::PageTypeNotAString { found: other.kind() })
            }
        }
    }
}

impl DeserializePipe for DeserializerPageClassifyStandard {
    fn name(&self) -> &str {
        "DeserializerPageClassifyStandard"
    }

    fn deserialize(&self, block: &TextBlock) -> Result<Vec<Extracted>, PipeError> {
        let class = self.call_page_class(block).map_err(|e| e.into_pipe_error(self.name()))?;
        Ok(vec![Extracted::PageClass(class)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::value::BlockValue;
    use crate::core::classes::{BlockType, TextBlock};
    use std::collections::BTreeMap;

    mod deserializer_page_classify {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn reads_a_present_page_type() {
            let mut metadata = BTreeMap::new();
            metadata.insert("page_type".to_string(), BlockValue::Str("investments".to_string()));
            let txt = TextBlock::from_content(BlockType::PAGE_CLASS, metadata, "");

            let result = DeserializerPageClassifyStandard.call(&txt).unwrap();
            assert_eq!(result, BlockValue::Str("investments".to_string()));
        }

        #[test]
        fn a_present_but_null_page_type_is_ok_not_an_error() {
            let mut metadata = BTreeMap::new();
            metadata.insert("page_type".to_string(), BlockValue::Null);
            let txt = TextBlock::from_content(BlockType::PAGE_CLASS, metadata, "");

            let result = DeserializerPageClassifyStandard.call(&txt).unwrap();
            assert_eq!(result, BlockValue::Null);
        }

        #[test]
        fn a_missing_page_type_key_is_an_error() {
            let txt = TextBlock::from_content(BlockType::PAGE_CLASS, BTreeMap::new(), "");
            assert!(DeserializerPageClassifyStandard.call(&txt).is_err());
        }
    }

    /// M5: lo stesso pipe visto come [`DeserializePipe`], cioè come il motore lo usa.
    mod as_a_deserialize_pipe {
        use super::*;
        use crate::core::pipeline::{DeserializePipe, Extracted, PipeError};
        use crate::core::schedule::PageClass;
        use pretty_assertions::assert_eq;

        fn block_with(page_type: BlockValue) -> TextBlock {
            let metadata = BTreeMap::from([("page_type".to_string(), page_type)]);
            TextBlock::from_content(BlockType::PAGE_CLASS, metadata, "")
        }

        #[test]
        fn a_string_page_type_becomes_a_page_class() {
            let out = DeserializerPageClassifyStandard
                .deserialize(&block_with(BlockValue::from("investments")))
                .unwrap();
            assert_eq!(out, vec![Extracted::PageClass(Some(PageClass::new("investments")))]);
        }

        #[test]
        fn a_null_page_type_becomes_an_explicitly_unclassified_page() {
            let out =
                DeserializerPageClassifyStandard.deserialize(&block_with(BlockValue::Null)).unwrap();
            assert_eq!(out, vec![Extracted::PageClass(None)]);
        }

        #[test]
        fn a_page_type_of_the_wrong_type_is_a_pipe_error_naming_the_pipe() {
            let err = DeserializerPageClassifyStandard
                .deserialize(&block_with(BlockValue::from(3i64)))
                .unwrap_err();
            assert_eq!(err.pipe(), "DeserializerPageClassifyStandard");
            assert!(matches!(err, PipeError::Extraction { .. }));
        }

        #[test]
        fn a_missing_page_type_key_is_a_value_error_not_a_page_failure() {
            let block = TextBlock::from_content(BlockType::PAGE_CLASS, BTreeMap::new(), "");
            let err = DeserializerPageClassifyStandard.deserialize(&block).unwrap_err();
            assert!(matches!(err, PipeError::Value { .. }));
            assert!(!err.is_page_failure());
        }
    }
}
