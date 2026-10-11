//! [`PdfLine`]: one line of text extracted from a page — font, size, text, bounding box.
//!
//! The *data* half of a line. The algebra for selecting lines by font, size, text or area lives in
//! [`super::select`], which imports [`Font`] from here and implements its set traits there. The
//! dependency runs one way on purpose: selections depend on the data, never the other way round.
//!
//! A line deliberately carries no cached `area`: deriving an area from its bounding box is a
//! wrapping with no real work in it, so caching one would be a redundant field that can go stale.
//! [`super::select::pdf_line::area`] adds the accessor that derives it on demand.

use crate::commons::geometry::Rectangle;

/// A normalised font name.
#[derive(Debug, PartialEq, Clone, Hash, Eq)]
pub struct Font(String);

impl Font {
    /// Normalises eagerly: Latin accents, spaces, brackets and punctuation, lowercase.
    ///
    /// Eagerly rather than on comparison, because a font name is compared far more often than it is
    /// built, and because it makes two lines with the same font compare equal by construction.
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

    /// The normalised string.
    pub fn inner(&self) -> &str {
        &self.0
    }
}

/// A line of text extracted from a PDF page: normalised font, size, text, bounding box.
#[derive(Debug, Clone)]
pub struct PdfLine {
    font: Font,
    font_size: f32,
    text: String,
    bbox: Rectangle,
}

impl PdfLine {
    /// Normalises `font` with [`Font::new`] and builds the bounding box with [`Rectangle::new`].
    ///
    /// # Panics
    ///
    /// If `area` is not a valid rectangle, through [`Rectangle::new`], and if `font_size` is zero
    /// or negative — a line without a positive size is a malformed input, not a line worth carrying
    /// further.
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
