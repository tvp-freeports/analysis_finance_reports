//! Pipe `deserialize` standard — sottoinsieme autosufficiente di
//! `freeports_core/src/formats_utils/deserialize/standard_funcs.rs`.
//!
//! Scope deciso dall'utente (`agent-memory/M4-implementation-plan.md` §0, opzione A): solo
//! `DeserializerPageClassifyStandard` e' costruibile senza `output::classes` (M8) — le altre
//! (`DeserializeSfdrArticleStandard`, `DeserializerFundStandard`,
//! `DeserializerManagmentCompanyStandard`, `DeserializerInvestmentsManagerFromManco`,
//! `DeserializerInvestmentsManagerStandard`) costruiscono entita' che non esistono ancora.
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

#[derive(Debug, thiserror::Error)]
pub enum DeserializeStandardFuncsError {
    #[error(transparent)]
    Value(#[from] BlockValueError),
}

pub struct DeserializerPageClassifyStandard;

impl DeserializerPageClassifyStandard {
    pub fn call(&self, txt_blk: &TextBlock) -> Result<BlockValue, DeserializeStandardFuncsError> {
        Ok(txt_blk.metadata_or_fail("page_type")?.clone())
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
}
