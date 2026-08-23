//! PdfLine: riga di testo estratta (font, corpo, bbox, testo).
//!
//! Porting verbatim del riferimento (`PLAN.md` §0/§12 D14): stessa logica di
//! `freeports_core::formats_utils::pdf_extract::select::pdf_line` (la parte *dati*, non le
//! selezioni), solo spostata in un modulo a se' (`pdf_extract::pdf_line`, sibling di
//! `pdf_extract::select`) e senza PyO3.
//!
//! Contratto atteso dai test qui sotto (il test-writer non scrive codice di produzione):
//!
//! - `pub struct Font(String)` — solo la parte *dati*: normalizzazione (accenti/spazi/parentesi/
//!   virgole/trattini, vedi il riferimento `select/pdf_line/font.rs`), non l'algebra di
//!   selezione. `Font::new(input: &str) -> Font` normalizza eagerly (stessa logica del
//!   riferimento); `Font::inner(&self) -> &str` espone la stringa normalizzata. Deriva almeno
//!   `Debug, Clone, PartialEq, Eq, Hash` (serve a `select::pdf_line::font::FontSet =
//!   DisjointAtomsSet<Font,Font>`, che richiede quei bound).
//!   - **Decisione R4 (`PLAN.md`)**: `Container`/`Overlappable`/`AtomOperations`/`AtomAlgebra`
//!     per `Font`, e il tipo `FontSet` stesso, **non** stanno qui — stanno in
//!     `select::pdf_line::font` (che importa `Font` da qui e vi implementa quei trait: lecito in
//!     Rust, l'`impl` non deve stare nel file di definizione del tipo purche' nello stesso
//!     crate). Questo modulo (`pdf_line`) non deve dipendere da `select` (le selezioni dipendono
//!     dai dati, non il contrario).
//! - `pub struct PdfLine { font: Font, font_size: f32, text: String, bbox: Rectangle }` — **niente
//!   campo `area`** a differenza del riferimento: costruire un `Area` da un `Rectangle` e' solo
//!   un wrapping (`Area::from_atom`, nessuna normalizzazione reale, a differenza di
//!   `Font::new`), quindi cache-arlo qui sarebbe un campo ridondante derivabile a costo zero.
//!   `select::pdf_line::area` aggiunge invece un metodo `PdfLine::area(&self) -> Area` che lo
//!   deriva on demand (vedi il doc-comment di quel modulo per i test che lo riguardano) — non e'
//!   testato qui, dato che dipende dal tipo `Area` (selezione), non dai dati puri.
//!   - `PdfLine::new(font: &str, font_size: f32, text: &str, area: (f32,f32,f32,f32)) -> Self`:
//!     normalizza il font con `Font::new`, costruisce `bbox` con `Rectangle::new` (verbatim:
//!     puo' quindi andare in panico se `area` non e' un rettangolo valido, esattamente come
//!     `Rectangle::new`). **Va in panico** con il messaggio esatto
//!     `"Font size of a PdfLine cannot be negative"` se `font_size <= 0.0` (verbatim dal
//!     riferimento, incluso lo zero: la guardia e' `<= 0.0`, non `< 0.0`).
//!   - Accessori: `font(&self) -> &Font`, `font_size(&self) -> &f32`, `bbox(&self) -> &Rectangle`,
//!     `text(&self) -> &String` (stessa forma del riferimento).
//!   - Deriva almeno `Debug, Clone` (il riferimento non richiede `PartialEq`/`Eq`/`Hash` qui, e
//!     `f32`/`Rectangle` a bordo rendono `Eq` scomodo senza `OrderedFloat`; non e' richiesto da
//!     nessun contratto pubblico di questa milestone).

use crate::commons::geometry::Rectangle;

/// Font normalizzato (dati + normalizzazione soltanto: l'algebra di selezione vive in
/// `select::pdf_line::font`, R4 del `PLAN.md`).
#[derive(Debug, PartialEq, Clone, Hash, Eq)]
pub struct Font(String);

impl Font {
    /// Normalizza eagerly: accenti latini, spazi/parentesi/punteggiatura, minuscolo. Verbatim dal
    /// riferimento (`freeports_core::...::select::pdf_line::font::Font::new`).
    pub fn new(input: &str) -> Self {
        let trimmed_input = input.trim();
        let mut out = String::with_capacity(trimmed_input.len());
        let mut last_was_div = false;
        for ch in trimmed_input.chars() {
            let replacement: Option<String> = match ch {
                'é' | 'è' | 'ê' | 'ë' => Some("e".into()),
                'á' | 'à' | 'â' | 'ä' => Some("a".into()),
                'í' | 'ì' | 'î' | 'ï' => Some("i".into()),
                'ó' | 'ò' | 'ô' | 'ö' => Some("o".into()),
                'ú' | 'ù' | 'û' | 'ü' => Some("u".into()),
                '&' => Some("and".into()),
                '{' | '(' => Some('['.into()),
                '}' | ')' => Some(']'.into()),
                '–' | '/' | '.' => Some('-'.into()),
                ',' => None, // Usato in formati come EURIZON-IT24, deliberato.
                c if c.is_whitespace() => Some('-'.into()),
                c => Some(c.to_lowercase().to_string()),
            };

            if let Some(rep) = replacement {
                if rep == "-" {
                    if !last_was_div {
                        out.push('-');
                        last_was_div = true;
                    }
                } else {
                    out.push_str(&rep);
                    last_was_div = false;
                }
            }
        }
        Self(out)
    }

    /// La stringa normalizzata.
    pub fn inner(&self) -> &str {
        &self.0
    }
}

/// Riga di testo estratta da una pagina PDF: font (normalizzato), corpo, testo, bbox.
#[derive(Debug, Clone)]
pub struct PdfLine {
    font: Font,
    font_size: f32,
    text: String,
    bbox: Rectangle,
}

impl PdfLine {
    /// Normalizza `font` con [`Font::new`] e costruisce `bbox` con [`Rectangle::new`] (puo'
    /// quindi andare in panico se `area` non e' un rettangolo valido). Va in panico anche se
    /// `font_size <= 0.0`, verbatim dal riferimento.
    pub fn new(font: &str, font_size: f32, text: &str, area: (f32, f32, f32, f32)) -> Self {
        if font_size <= 0.0 {
            panic!("Font size of a PdfLine cannot be negative")
        }
        let (x0, y0, x1, y1) = area;
        Self { font: Font::new(font), font_size, text: text.to_string(), bbox: Rectangle::new(x0, y0, x1, y1) }
    }

    pub fn font(&self) -> &Font {
        &self.font
    }

    pub fn font_size(&self) -> &f32 {
        &self.font_size
    }

    pub fn bbox(&self) -> &Rectangle {
        &self.bbox
    }

    pub fn text(&self) -> &String {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commons::geometry::Rectangle;

    mod font_normalization {
        use super::*;
        use test_case::test_case;

        #[test_case("NicaRAguA", "nicaragua"; "lowercases")]
        #[test_case("ulma turman\t \n gerico\tsum", "ulma-turman-gerico-sum"; "collapses whitespace runs into a single dash")]
        #[test_case("áàâäéèêëíìîïóòôöúùûü", "aaaaeeeeiiiioooouuuu"; "strips latin accents")]
        #[test_case("oba{pes}li(cu)[b]", "oba[pes]li[cu][b]"; "normalizes parenthesis and braces to square brackets")]
        #[test_case("&", "and"; "spells out ampersand")]
        #[test_case("ooo,oooo–o/ooo.oo", "ooooooo-o-ooo-oo"; "treats en-dash slash and dot as separators but keeps commas")]
        #[test_case("\t \n gattopardo \n\n", "gattopardo"; "trims leading and trailing whitespace")]
        fn produces_expected_normalized_form(input: &str, expected: &str) {
            assert_eq!(Font::new(input).inner(), expected);
        }

        #[test]
        fn two_differently_spelled_equivalent_fonts_normalize_equal() {
            assert_eq!(Font::new("Arial\n"), Font::new("  ARIAL "));
        }
    }

    mod construction {
        use super::*;

        #[test]
        fn stores_normalized_font_size_text_and_bbox() {
            let line = PdfLine::new("Arial", 45.3, "La grange muraja axur!", (6.0, 4.0, 70.0, 60.0));
            assert_eq!(line.font(), &Font::new("Arial"));
            assert_eq!(*line.font_size(), 45.3);
            assert_eq!(line.text(), "La grange muraja axur!");
            assert_eq!(line.bbox().as_tuple(), (6.0, 4.0, 70.0, 60.0));
        }

        #[test]
        fn normalizes_font_the_same_way_as_a_bare_font_new_call() {
            let line = PdfLine::new("Arial\n", 43.2, "rumi", (0.0, 0.0, 1.0, 1.0));
            assert_eq!(line.font(), &Font::new("Arial\n"));
            assert_eq!(line.font().inner(), "arial");
        }

        #[test]
        #[should_panic(expected = "Font size of a PdfLine cannot be negative")]
        fn panics_on_negative_font_size() {
            PdfLine::new("Arial", -45.3, "La grange muraja axur!", (6.0, 4.0, 70.0, 60.0));
        }

        #[test]
        #[should_panic(expected = "Font size of a PdfLine cannot be negative")]
        fn panics_on_zero_font_size() {
            PdfLine::new("Arial", 0.0, "text", (0.0, 0.0, 1.0, 1.0));
        }

        #[test]
        fn bbox_is_a_real_rectangle_usable_independently_of_pdfline() {
            let line = PdfLine::new("Arial", 10.0, "text", (1.0, 2.0, 3.0, 4.0));
            assert_eq!(*line.bbox(), Rectangle::new(1.0, 2.0, 3.0, 4.0));
        }
    }
}
