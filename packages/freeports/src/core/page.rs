//! Modello Rust di `Page` e `Document` (righe, immagini, geometria, provenienza).
//!
//! `PLAN.md` §4.4. Il modulo definisce **solo i dati**: la costruzione a partire dal dict PyMuPDF
//! (rotazione delle bbox, collasso degli span, estrazione immagini) è `input::document` (M6).
//! Nasce qui, in M5, perché è il tipo su cui sono scritte le firme dei tre trait dei pipe
//! (`&Page`, `PLAN.md` §5.1): senza, il motore non è esprimibile.
//!
//! **`Page::raw` — decisione dell'utente (2026-08-23, `agent-memory/M5-implementation-plan.md`
//! D-M5-2).** `PLAN.md` §4.4 mette il dict PyMuPDF originale accanto alla `Page` nativa, mentre
//! §2 principio 1 vuole `Py<PyAny>` solo nei moduli di confine. L'utente ha scelto di aggiungere
//! il campo **subito**, benché il primo consumatore arrivi con M7 (i pipe definiti dall'autore del
//! formato, che si aspettano il dict e non la `Page` nativa): saperlo da ora impedisce di
//! costruire codice che dipende da `Clone`/`PartialEq` su `Page`, derive che quel campo rende
//! comunque impossibili (`Py<T>` è `Clone` solo con la feature `py-clone`, non abilitata). Il
//! resto del crate continua a trattare `Page` come una struct Rust pura: `raw` è privato e
//! nessun modulo fuori dal confine Python lo legge.
//!
//! `Page` resta `Send + Sync` (lo è `Py<PyAny>`), requisito di `PLAN.md` §5.1 per poter
//! parallelizzare per pagina/documento senza riprogettare.

use pyo3::prelude::*;

use crate::commons::geometry::Rectangle;
use crate::formats_utils::pdf_extract::pdf_line::PdfLine;

/// Identificatore di un documento: nome corto, path o url.
///
/// Newtype e non `String` nudo per la stessa ragione di [`BlockType`](crate::core::classes::BlockType):
/// dà un posto dove mettere il comportamento e impedisce di scambiarlo con un
/// [`FormatName`] in una firma. `targets/2_multireport_support.md` lo usa come chiave con cui i
/// risultati vengono raggruppati per documento.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct DocumentId(String);

impl DocumentId {
    pub fn new(id: impl Into<String>) -> Self {
        DocumentId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for DocumentId {
    fn from(value: &str) -> Self {
        DocumentId(value.to_string())
    }
}

impl From<String> for DocumentId {
    fn from(value: String) -> Self {
        DocumentId(value)
    }
}

/// Nome di un formato del repo formati (es. `EURIZON-EN23`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FormatName(String);

impl FormatName {
    pub fn new(name: impl Into<String>) -> Self {
        FormatName(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FormatName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for FormatName {
    fn from(value: &str) -> Self {
        FormatName(value.to_string())
    }
}

impl From<String> for FormatName {
    fn from(value: String) -> Self {
        FormatName(value)
    }
}

/// Un'immagine raster di una pagina, **non decodificata**.
///
/// Il riferimento (`pdf_blks_acquire.pdfimages_from_pagedict`) restituisce array NumPy RGB,
/// decodificando con PIL. Qui i byte restano grezzi, con l'estensione dichiarata dal dict
/// PyMuPDF: decodificarli richiederebbe una dipendenza nuova (`image`) che **nessun pipe in
/// nessuna milestone pianificata consuma** — nessun `standard_funcs` legge le immagini. Se un
/// consumatore reale comparirà, decodificherà da qui; nel frattempo il dato non si perde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageImage {
    /// Riquadro occupato dall'immagine nella pagina.
    pub bbox: Rectangle,
    /// Estensione del formato immagine dichiarata da PyMuPDF (`"png"`, `"jpeg"`, ...).
    pub ext: String,
    /// Byte grezzi dell'immagine, così come PyMuPDF li restituisce.
    pub data: Vec<u8>,
}

/// Una pagina di un documento PDF, già convertita in dati nativi.
///
/// Non deriva `Clone`/`PartialEq`: vedi la nota su [`Page::raw`] nel doc-comment del modulo.
#[derive(Debug)]
pub struct Page {
    /// Numero di pagina **1-based**, come nel riferimento.
    pub number: u32,
    /// `(larghezza, altezza)` in punti PDF.
    pub size: (f32, f32),
    /// Righe di testo, nell'ordine in cui `input::document` le ha estratte.
    pub lines: Vec<PdfLine>,
    /// Immagini raster della pagina.
    pub images: Vec<PageImage>,
    /// Il dict PyMuPDF originale, conservato per i pipe definiti dall'autore (M7).
    raw: Option<Py<PyAny>>,
}

impl Page {
    /// Costruisce una pagina nativa senza dict PyMuPDF allegato — la forma usata da tutto ciò
    /// che non è il confine Python (test compresi).
    pub fn new(number: u32, size: (f32, f32), lines: Vec<PdfLine>, images: Vec<PageImage>) -> Self {
        Page { number, size, lines, images, raw: None }
    }

    /// Allega il dict PyMuPDF originale. Chiamata solo da `input::document` (M6).
    pub fn with_raw(mut self, raw: Py<PyAny>) -> Self {
        self.raw = Some(raw);
        self
    }

    /// Il dict PyMuPDF originale, se presente. Unico accessore che nomina PyO3 in `core`; lo
    /// leggono solo gli adattatori dei pipe Python (`formats_repo::unstructured`, M7).
    pub fn raw(&self) -> Option<&Py<PyAny>> {
        self.raw.as_ref()
    }
}

/// Un documento: la sua identità, il formato con cui va interpretato, le sue pagine.
#[derive(Debug)]
pub struct Document {
    pub id: DocumentId,
    pub format: FormatName,
    pub pages: Vec<Page>,
}

impl Document {
    pub fn new(id: impl Into<DocumentId>, format: impl Into<FormatName>, pages: Vec<Page>) -> Self {
        Document { id: id.into(), format: format.into(), pages }
    }

    /// La pagina con il numero **1-based** dato, cercata per `Page::number` e non per posizione:
    /// un documento può legittimamente contenere un sottoinsieme non contiguo delle sue pagine.
    pub fn page(&self, number: u32) -> Option<&Page> {
        self.pages.iter().find(|p| p.number == number)
    }
}

/// Fallimenti che riguardano una pagina, non un singolo pipe.
///
/// `PLAN.md` §8: `PageParseFail`/`LineParseFail`, oggi eccezioni Python, diventano varianti
/// tipizzate. Un [`PageError`] che risale attraverso un pipe diventa
/// [`PipeError::PageParse`](crate::core::pipeline::PipeError::PageParse), che
/// [`Algorithm`](crate::core::algorithm::Algorithm) tratta come **non fatale**: la pagina si
/// salta e l'elaborazione prosegue.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PageError {
    #[error("{message}")]
    ParseFail { message: String },
    #[error("{message}")]
    LineParseFail { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> PdfLine {
        PdfLine::new("Arial", 10.0, text, (0.0, 0.0, 10.0, 10.0))
    }

    fn image() -> PageImage {
        PageImage {
            bbox: Rectangle::new(0.0, 0.0, 4.0, 4.0),
            ext: "png".to_string(),
            data: vec![1, 2, 3],
        }
    }

    mod identifiers {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_document_id_round_trips_through_its_accessor() {
            assert_eq!(DocumentId::new("report-2023").as_str(), "report-2023");
        }

        #[test]
        fn a_document_id_displays_as_its_bare_string() {
            assert_eq!(DocumentId::new("a/b/c.pdf").to_string(), "a/b/c.pdf");
        }

        #[test]
        fn a_document_id_is_built_from_both_str_and_string() {
            assert_eq!(DocumentId::from("x"), DocumentId::from("x".to_string()));
        }

        #[test]
        fn document_ids_order_and_hash_by_their_string() {
            let mut ids = vec![DocumentId::new("b"), DocumentId::new("a")];
            ids.sort();
            assert_eq!(ids, vec![DocumentId::new("a"), DocumentId::new("b")]);

            let set: std::collections::HashSet<_> =
                [DocumentId::new("a"), DocumentId::new("a")].into_iter().collect();
            assert_eq!(set.len(), 1);
        }

        #[test]
        fn a_format_name_round_trips_and_displays() {
            assert_eq!(FormatName::new("EURIZON-EN23").as_str(), "EURIZON-EN23");
            assert_eq!(FormatName::from("EURIZON-EN23").to_string(), "EURIZON-EN23");
        }

        #[test]
        fn a_format_name_is_built_from_both_str_and_string() {
            assert_eq!(FormatName::from("f"), FormatName::from("f".to_string()));
        }
    }

    mod page_construction {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_page_keeps_the_lines_in_the_order_given() {
            let page = Page::new(1, (595.0, 842.0), vec![line("first"), line("second")], vec![]);
            let texts: Vec<&str> = page.lines.iter().map(|l| l.text().as_str()).collect();
            assert_eq!(texts, vec!["first", "second"]);
        }

        #[test]
        fn a_page_keeps_its_number_and_size() {
            let page = Page::new(7, (595.0, 842.0), vec![], vec![]);
            assert_eq!(page.number, 7);
            assert_eq!(page.size, (595.0, 842.0));
        }

        #[test]
        fn a_page_built_natively_carries_no_pymupdf_dict() {
            let page = Page::new(1, (1.0, 1.0), vec![], vec![]);
            assert!(page.raw().is_none());
        }

        #[test]
        fn a_page_keeps_its_images_undecoded() {
            let page = Page::new(1, (1.0, 1.0), vec![], vec![image()]);
            assert_eq!(page.images.len(), 1);
            assert_eq!(page.images[0].ext, "png");
            assert_eq!(page.images[0].data, vec![1, 2, 3]);
        }

        #[test]
        fn a_page_with_neither_lines_nor_images_is_legal() {
            let page = Page::new(1, (0.0, 0.0), vec![], vec![]);
            assert!(page.lines.is_empty());
            assert!(page.images.is_empty());
        }
    }

    mod document_construction {
        use super::*;
        use pretty_assertions::assert_eq;

        fn doc() -> Document {
            Document::new(
                "report",
                "FMT-EN23",
                vec![Page::new(1, (1.0, 1.0), vec![], vec![]), Page::new(3, (1.0, 1.0), vec![], vec![])],
            )
        }

        #[test]
        fn a_document_keeps_id_and_format() {
            let d = doc();
            assert_eq!(d.id, DocumentId::new("report"));
            assert_eq!(d.format, FormatName::new("FMT-EN23"));
        }

        #[test]
        fn a_page_is_looked_up_by_its_number_not_by_position() {
            // Pagine 1 e 3: la 3 sta in posizione 1, quindi cercare per indice darebbe la
            // risposta sbagliata.
            let d = doc();
            assert_eq!(d.page(3).map(|p| p.number), Some(3));
        }

        #[test]
        fn looking_up_an_absent_page_yields_none() {
            assert!(doc().page(2).is_none());
        }

        #[test]
        fn a_document_with_no_pages_is_legal() {
            let d = Document::new("empty", "FMT", vec![]);
            assert!(d.pages.is_empty());
            assert!(d.page(1).is_none());
        }
    }

    mod page_errors {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_parse_failure_displays_its_bare_message() {
            let err = PageError::ParseFail { message: "no table found".to_string() };
            assert_eq!(err.to_string(), "no table found");
        }

        #[test]
        fn a_line_parse_failure_displays_its_bare_message() {
            let err = PageError::LineParseFail { message: "bad span".to_string() };
            assert_eq!(err.to_string(), "bad span");
        }

        #[test]
        fn the_two_variants_are_distinguishable() {
            assert_ne!(
                PageError::ParseFail { message: "x".to_string() },
                PageError::LineParseFail { message: "x".to_string() }
            );
        }
    }

    /// Unico sottomodulo che si attacca all'interprete (`PLAN.md` §10, D13): verifica che il
    /// dict PyMuPDF allegato sia davvero conservato e restituito. Tutto il resto di `core` non
    /// tocca Python.
    mod python_boundary {
        use super::*;

        #[test]
        fn an_attached_pymupdf_dict_is_kept_and_returned() {
            Python::attach(|py| {
                let raw = pyo3::types::PyDict::new(py);
                raw.set_item("width", 595).unwrap();
                let page = Page::new(1, (595.0, 842.0), vec![], vec![])
                    .with_raw(raw.clone().into_any().unbind());

                let kept = page.raw().expect("raw was just attached");
                let width: i64 = kept.bind(py).get_item("width").unwrap().extract().unwrap();
                assert_eq!(width, 595);
            });
        }
    }
}
