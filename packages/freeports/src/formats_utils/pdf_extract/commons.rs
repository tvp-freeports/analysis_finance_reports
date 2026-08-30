//! Helper condivisi dai pipe pdf_extract (SelectExpectedText, estrazione o fallimento pagina).
//!
//! **Non verbatim** (a differenza degli altri moduli di `pdf_extract`): il vecchio riferimento
//! (`freeports_core::formats_utils::pdf_extract::common`) e' PyO3 al 100% — `selection: Py<PyAny>`
//! duck-tipizzato su un metodo Python `.select(lines) -> list`, errori sollevati come `PyErr`.
//! Non c'e' equivalente Python qui: la pipeline e' Rust puro da M3 in poi, quindi `selection`
//! diventa un `PdfLineSet` concreto gia' risolto (`select::pdf_line::PdfLineSet`) — **non** un
//! `PdfLineSelection` (che puo' ancora essere `Relative` e richiede `.contextualize(lines)` prima
//! di poter essere interrogato): risolvere l'eventuale parte relativa e' responsabilita' del
//! chiamante, cosi' questo modulo non deve dipendere da `pdf_extract::relative`/`select::relative`.
//!
//! `PageError`/`PipeError` non esistono ancora (arrivano in M5/M8): la variante "non trovato" e
//! quella "fallimento pagina" vivono percio' in un solo enum locale, `CommonsError`
//! (`thiserror`, un enum per modulo, `PLAN.md` §8) — quando M5/M8 introdurranno i tipi definitivi,
//! questo enum si convertira' (o sara' sostituito) in quello, ma non prima.
//!
//! Preservata la *logica* del riferimento (corretta gia' li', vedi il suo doc-comment: `logger`/
//! `ExpectedPdfBlockNotFound`/`PageParseFail` referenziati senza import, bug gia' corretto prima
//! del porting):
//!
//! Contratto atteso dai test qui sotto (il test-writer non scrive codice di produzione):
//!
//! - `pub enum CommonsError { ExpectedTextNotFound{ name: String }, PageParseFail{ source:
//!   Box<CommonsError> } }`, `thiserror::Error`:
//!   - `ExpectedTextNotFound`: messaggio `"Pdf block during extraction of \"{name}\" not found"`.
//!   - `PageParseFail`: messaggio uguale a quello della sua `source` (`#[error("{source}")]`,
//!     `#[source] source: Box<CommonsError>`) — rispecchia il riferimento, dove il messaggio del
//!     `PageParseFail` python era letteralmente la stringificazione dell'errore causa
//!     (`err.value(py).str()`), con la causa incatenata (`set_cause`/`__cause__`).
//! - `pub fn select_expected_text(selection: &PdfLineSet, lines: &[PdfLine], name: &str) ->
//!   Result<String, CommonsError>`: il testo della *prima* riga di `lines` (nell'ordine dato, non
//!   riordinato) per cui `selection.contains(line)` e' vero; `Err(CommonsError::
//!   ExpectedTextNotFound{name})` se nessuna riga corrisponde (lista vuota inclusa).
//! - `pub fn extract_text_pdf_block_or_fail_page(selection: &PdfLineSet, lines: &[PdfLine], name:
//!   &str, type_block: BlockType) -> Result<Vec<PdfBlock>, CommonsError>`: chiama
//!   `select_expected_text`; in caso di successo restituisce `vec![PdfBlock::bare(type_block,
//!   text)]` (un solo blocco, coerente con `PLAN.md` §4.2 — niente metadati); in caso di
//!   `ExpectedTextNotFound`, lo avvolge in `CommonsError::PageParseFail{source}` (senza
//!   modificarne il messaggio, che resta quello della causa).

use crate::commons::sets::Container;
use crate::core::classes::{BlockType, PdfBlock};

use super::pdf_line::PdfLine;
use super::select::pdf_line::PdfLineSet;

#[derive(Debug, thiserror::Error)]
pub enum CommonsError {
    #[error("Pdf block during extraction of \"{name}\" not found")]
    ExpectedTextNotFound { name: String },
    #[error("{source}")]
    PageParseFail {
        #[source]
        source: Box<CommonsError>,
    },
}

/// Il testo della prima riga di `lines` (nell'ordine dato) che `selection` seleziona.
pub fn select_expected_text(selection: &PdfLineSet, lines: &[PdfLine], name: &str) -> Result<String, CommonsError> {
    let found = lines.iter().find(|line| selection.contains(line)).map(|line| line.text().clone());
    // Il testo scelto, non un booleano: e' la riga che un autore di formati va a cercare nel PDF.
    match &found {
        Some(text) => tracing::trace!(coord_ref_2 = name, found = %text, "selection resolved to a line"),
        None => tracing::debug!(coord_ref_2 = name, "selection matched no line on this page"),
    }
    found.ok_or_else(|| CommonsError::ExpectedTextNotFound { name: name.to_string() })
}

/// Estrae un unico `PdfBlock` "bare" dal testo selezionato, o fallisce l'intera pagina.
pub fn extract_text_pdf_block_or_fail_page(
    selection: &PdfLineSet,
    lines: &[PdfLine],
    name: &str,
    type_block: BlockType,
) -> Result<Vec<PdfBlock>, CommonsError> {
    match select_expected_text(selection, lines, name) {
        Ok(text) => Ok(vec![PdfBlock::bare(type_block, text)]),
        Err(source) => Err(CommonsError::PageParseFail { source: Box::new(source) }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::{BlockType, PdfBlock};
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;

    fn lines() -> Vec<PdfLine> {
        vec![
            PdfLine::new("Arial", 10.0, "Alpha", (0.0, 0.0, 10.0, 10.0)),
            PdfLine::new("Arial", 10.0, "Beta", (0.0, 10.0, 10.0, 20.0)),
            PdfLine::new("Arial", 10.0, "Beta", (0.0, 20.0, 10.0, 30.0)),
        ]
    }

    mod select_expected_text {
        use super::*;

        #[test]
        fn returns_the_text_of_the_matching_line() {
            let ls = lines();
            let selection = PdfLineSet::select_text("^Alpha$");
            assert_eq!(select_expected_text(&selection, &ls, "field").unwrap(), "Alpha");
        }

        #[test]
        fn returns_the_first_match_by_input_order_when_several_lines_match() {
            let ls = vec![
                PdfLine::new("Arial", 8.0, "Skip", (0.0, 0.0, 10.0, 10.0)),
                PdfLine::new("Arial", 12.0, "First", (0.0, 10.0, 10.0, 20.0)),
                PdfLine::new("Arial", 12.0, "Second", (0.0, 20.0, 10.0, 30.0)),
            ];
            let selection = PdfLineSet::select_fontsize(10.0, 14.0);
            assert_eq!(select_expected_text(&selection, &ls, "field").unwrap(), "First");
        }

        #[test]
        fn errors_with_expected_text_not_found_when_nothing_matches() {
            let ls = lines();
            let selection = PdfLineSet::select_text("^Gamma$");
            let err = select_expected_text(&selection, &ls, "the field").unwrap_err();
            let CommonsError::ExpectedTextNotFound { name } = err else { panic!("expected ExpectedTextNotFound") };
            assert_eq!(name, "the field");
        }

        #[test]
        fn error_message_names_the_field() {
            let ls = lines();
            let selection = PdfLineSet::select_text("^Gamma$");
            let err = select_expected_text(&selection, &ls, "the field").unwrap_err();
            assert!(err.to_string().contains("the field"));
        }

        #[test]
        fn errors_on_an_empty_list_of_lines() {
            let selection = PdfLineSet::select_text("^Anything$");
            let err = select_expected_text(&selection, &[], "field").unwrap_err();
            assert!(matches!(err, CommonsError::ExpectedTextNotFound { .. }));
        }
    }

    mod extract_text_pdf_block_or_fail_page {
        use super::*;

        #[test]
        fn builds_a_single_bare_pdfblock_from_the_matched_text() {
            let ls = lines();
            let selection = PdfLineSet::select_text("^Alpha$");
            let blocks = extract_text_pdf_block_or_fail_page(&selection, &ls, "field", BlockType::new("FUND")).unwrap();
            assert_eq!(blocks, vec![PdfBlock::bare(BlockType::new("FUND"), "Alpha")]);
        }

        #[test]
        fn wraps_not_found_into_a_page_parse_fail_carrying_the_leaf_as_source() {
            let ls: Vec<PdfLine> = vec![];
            let selection = PdfLineSet::select_text("^Anything$");
            let err = extract_text_pdf_block_or_fail_page(&selection, &ls, "field", BlockType::new("FUND")).unwrap_err();
            let CommonsError::PageParseFail { source } = &err else { panic!("expected PageParseFail") };
            assert!(matches!(&**source, CommonsError::ExpectedTextNotFound { .. }));
        }

        #[test]
        fn page_parse_fail_message_matches_its_source_message() {
            let ls: Vec<PdfLine> = vec![];
            let selection = PdfLineSet::select_text("^Anything$");
            let err = extract_text_pdf_block_or_fail_page(&selection, &ls, "field", BlockType::new("FUND")).unwrap_err();
            let CommonsError::PageParseFail { source } = &err else { panic!("expected PageParseFail") };
            assert_eq!(err.to_string(), source.to_string());
        }

        #[test]
        fn page_parse_fail_exposes_the_leaf_via_std_error_source() {
            use std::error::Error;
            let ls: Vec<PdfLine> = vec![];
            let selection = PdfLineSet::select_text("^Anything$");
            let err = extract_text_pdf_block_or_fail_page(&selection, &ls, "field", BlockType::new("FUND")).unwrap_err();
            assert!(err.source().is_some());
        }
    }
}
