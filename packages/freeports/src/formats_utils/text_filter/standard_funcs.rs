//! Pipe `text_filter` standard — sottoinsieme autosufficiente di
//! `freeports_core/src/formats_utils/text_filter/standard_funcs.rs`.
//!
//! Scope deciso dall'utente (`agent-memory/M4-implementation-plan.md` §0, opzione A): solo
//! `TextFilterPageClassifyStandard` ed `extract_currency_from_text` sono costruibili senza
//! `FilterData`/`Extracted` (M5) — le altre quattro classi (`TextFilterSfdrArticleStandard`,
//! `TextFilterManagmentCompanyStandard`, `TextFilterAssetsStandard`,
//! `TextFilterInvestmentsStandard`) leggono `filter_data` e restano lavoro di M5.
//!
//! **Contratto atteso dai test qui sotto** (il test-writer non scrive codice di produzione):
//!
//! ```text
//! pub struct TextFilterPageClassifyStandard;
//! impl TextFilterPageClassifyStandard {
//!     pub fn call(&self, pdf_blks: &[PdfBlock]) -> Result<Vec<TextBlock>, StandardFuncsError>;
//! }
//!
//! pub fn extract_currency_from_text(text: &str) -> Result<Currency, StandardFuncsError>;
//!
//! #[derive(Debug, thiserror::Error)]
//! pub enum StandardFuncsError { /* un enum locale, stesso trattamento provvisorio di
//!     CommonsError (M3, pdf_extract::commons) — verra' assorbito da PipeError in M5 */ }
//! ```
//!
//! `TextFilterPageClassifyStandard::call`: itera `pdf_blks` **nell'ordine dato**; ogni blocco
//! deve gia' avere in `metadata` la chiave `"page_type"` (impostata a monte da un pipe
//! `pdf_extract`, eventualmente a `BlockValue::Null` se quel blocco non e' classificato) — legge
//! quel campo con `metadata_or_fail("page_type")`. Se piu' di un blocco porta un `page_type`
//! diverso da `Null`, e' un errore (anche se i due valori non-null fossero uguali fra loro conta
//! comunque come "gia' trovato un valore" nel riferimento: qui pero' i test si limitano al caso
//! con valori diversi, l'unico specificato dal piano). Il risultato e' un singolo `TextBlock` di
//! tipo `BlockType::PAGE_CLASS`, costruito con `TextBlock::new` dall'**ultimo** `PdfBlock` della
//! lista (non il primo — facile da invertire per errore nel porting), con
//! `metadata = {"page_type": <valore trovato, o Null se nessuno}`. `pdf_blks` vuoto e' un errore.
//!
//! `extract_currency_from_text`: due passate, la prima vince appena trova qualcosa (bugfix gia'
//! documentato nel riferimento — un vecchio flag `found` che doveva interrompere la scansione di
//! fallback dopo il primo match non veniva mai impostato, quindi l'ultima valuta dichiarata
//! nell'enum vinceva sempre invece della prima incontrata nel testo; qui la prima passata che
//! trova qualcosa restituisce subito, senza continuare):
//! 1. Cerca ogni sotto-stringa di 3 lettere maiuscole delimitata da confini di parola (`\b`) nel
//!    testo **cosi' come scritto** (non maiuscolizzato), **nell'ordine in cui appaiono**; per la
//!    prima che e' un codice/nome valido (`Currency::from_name`), restituisce quella valuta.
//! 2. Se nessuna delle candidate sopra e' valida (incluso il caso "nessuna candidata trovata"),
//!    prova, sul testo maiuscolizzato, il codice ISO di ogni `Currency::variants()` **in ordine
//!    di dichiarazione**, delimitato da `\b`; poi prova l'alias `"EURO"` allo stesso modo. La
//!    prima che matcha vince.
//! 3. Se niente matcha in nessuna delle due passate, e' un errore.
//!
//! Nessuna delle due passate individua mai una sotto-stringa che non sia delimitata da un confine
//! di parola vero e proprio: una tripletta di lettere maiuscole incollata ad altre lettere o
//! cifre adiacenti (es. `"EURUSD"`, `"100EUR"`) non conta.

use once_cell::sync::Lazy;
use onig::Regex;
use std::collections::BTreeMap;

use crate::commons::consts::Currency;
use crate::core::classes::value::BlockValue;
use crate::core::classes::{BlockType, PdfBlock, TextBlock};

#[derive(Debug, thiserror::Error)]
pub enum StandardFuncsError {
    #[error("expected at least one pdf block to classify")]
    NoPdfBlocks,
    #[error("page classified both as {first:?} and as {second:?}")]
    ConflictingPageClass { first: String, second: String },
    #[error(transparent)]
    Value(#[from] crate::core::classes::value::BlockValueError),
    #[error("no currency found in text")]
    NoCurrencyFound,
}

pub struct TextFilterPageClassifyStandard;

impl TextFilterPageClassifyStandard {
    pub fn call(&self, pdf_blks: &[PdfBlock]) -> Result<Vec<TextBlock>, StandardFuncsError> {
        let last = pdf_blks.last().ok_or(StandardFuncsError::NoPdfBlocks)?;
        let mut found: Option<&BlockValue> = None;
        for blk in pdf_blks {
            let page_type = blk.metadata_or_fail("page_type")?;
            if !page_type.is_null() {
                if let Some(existing) = found {
                    return Err(StandardFuncsError::ConflictingPageClass {
                        first: format!("{existing:?}"),
                        second: format!("{page_type:?}"),
                    });
                }
                found = Some(page_type);
            }
        }
        let page_type = found.cloned().unwrap_or(BlockValue::Null);
        let metadata = BTreeMap::from([("page_type".to_string(), page_type)]);
        Ok(vec![TextBlock::new(BlockType::PAGE_CLASS, metadata, last.clone())])
    }
}

static ISO_CODE_CANDIDATE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b[A-Z]{3}\b").expect("fixed, hand-written pattern, valid onig regex"));

fn word_boundary_pattern(word: &str) -> Regex {
    Regex::new(&format!(r"\b{word}\b")).expect("currency code/alias is a fixed, valid pattern")
}

/// Estrae una [`Currency`] da testo libero — vedi il doc-comment del modulo per l'algoritmo a due
/// passate (prima passata su candidate a tre lettere maiuscole nell'ordine in cui appaiono nel
/// testo cosi' com'e' scritto, seconda passata sui codici ISO/alias `EURO` nel testo
/// maiuscolizzato).
///
/// **Nota su `onig`**: `Regex::is_match` in questo crate testa un match sull'*intera* stringa
/// (vedi la sua doc "Match vs Search"), non una ricerca al suo interno — a differenza di
/// `Regex::find`/`find_iter`, che cercano ovunque. Ogni controllo qui sotto usa quindi `find`,
/// non `is_match`, per un pattern che deve poter matchare in qualunque posizione del testo.
pub fn extract_currency_from_text(text: &str) -> Result<Currency, StandardFuncsError> {
    for (start, end) in ISO_CODE_CANDIDATE.find_iter(text) {
        if let Some(currency) = Currency::from_name(&text[start..end]) {
            return Ok(currency);
        }
    }

    let upper = text.to_uppercase();
    for currency in Currency::variants() {
        if word_boundary_pattern(currency.code()).find(&upper).is_some() {
            return Ok(*currency);
        }
    }
    if word_boundary_pattern("EURO").find(&upper).is_some() {
        return Ok(Currency::EUR);
    }

    Err(StandardFuncsError::NoCurrencyFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::consts::Currency;
    use crate::core::classes::value::BlockValue;
    use crate::core::classes::{BlockType, PdfBlock};
    use std::collections::BTreeMap;

    fn page_class_block(page_type: Option<&str>) -> PdfBlock {
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "page_type".to_string(),
            page_type.map(BlockValue::from).unwrap_or(BlockValue::Null),
        );
        PdfBlock::new(BlockType::new("SOME_PDF_TYPE"), metadata, "")
    }

    mod text_filter_page_classify {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn no_classified_block_yields_a_null_page_type() {
            let blks = vec![page_class_block(None), page_class_block(None)];
            let result = TextFilterPageClassifyStandard.call(&blks).unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].type_block, BlockType::PAGE_CLASS);
            assert_eq!(result[0].metadata.get("page_type"), Some(&BlockValue::Null));
        }

        #[test]
        fn a_single_classified_block_is_reported() {
            let blks = vec![page_class_block(None), page_class_block(Some("investments"))];
            let result = TextFilterPageClassifyStandard.call(&blks).unwrap();
            assert_eq!(
                result[0].metadata.get("page_type"),
                Some(&BlockValue::Str("investments".to_string()))
            );
        }

        #[test]
        fn two_conflicting_classifications_is_an_error() {
            let blks =
                vec![page_class_block(Some("investments")), page_class_block(Some("other"))];
            assert!(TextFilterPageClassifyStandard.call(&blks).is_err());
        }

        #[test]
        fn an_empty_list_of_pdf_blocks_is_an_error() {
            assert!(TextFilterPageClassifyStandard.call(&[]).is_err());
        }

        #[test]
        fn the_resulting_pdf_block_is_the_last_one_in_the_list_not_the_first() {
            let first = page_class_block(None);
            let last = page_class_block(None);
            let blks = vec![first, last.clone()];
            let result = TextFilterPageClassifyStandard.call(&blks).unwrap();
            assert_eq!(result[0].pdf_block.as_deref(), Some(&last));
        }
    }

    mod extract_currency_from_text {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn finds_an_iso_code_isolated_in_noise() {
            assert_eq!(
                extract_currency_from_text("Value: 100 EUR total").unwrap(),
                Currency::EUR
            );
        }

        #[test]
        fn finds_a_currency_by_full_name_case_insensitively() {
            assert_eq!(extract_currency_from_text("amount in euro today").unwrap(), Currency::EUR);
        }

        #[test]
        fn finds_the_euro_alias() {
            assert_eq!(extract_currency_from_text("priced in EURO").unwrap(), Currency::EUR);
        }

        #[test]
        fn prefers_the_first_currency_mentioned_in_the_text() {
            // Pins the already-fixed reference bug: the result must depend on which code
            // actually appears first in the text, not on Currency's declaration order.
            assert_eq!(
                extract_currency_from_text("Converted from USD to EUR").unwrap(),
                Currency::USD
            );
            assert_eq!(
                extract_currency_from_text("Converted from EUR to USD").unwrap(),
                Currency::EUR
            );
        }

        #[test]
        fn errors_when_no_currency_is_present() {
            assert!(extract_currency_from_text("no currency here").is_err());
        }

        #[test]
        fn does_not_match_a_code_glued_to_other_letters() {
            // "EURUSD" is a single 6-letter word: neither "EUR" nor "USD" is a standalone,
            // word-boundary-delimited match inside it.
            assert!(extract_currency_from_text("Ticker: EURUSD").is_err());
        }

        #[test]
        fn does_not_match_a_code_glued_to_a_digit() {
            assert!(extract_currency_from_text("100EUR").is_err());
        }
    }
}
