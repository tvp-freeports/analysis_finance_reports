//! `pdfline_selection_from_dict`/`_from_str`: costruiscono una `PdfLineSelection` (M3) da
//! configurazione esterna (repo formati) — non hanno nulla a che fare con PyMuPDF, ma
//! `PLAN.md` §9/l'`api.rs` esistente le colloca comunque in `input::document` (stesso modulo
//! Python di origine, `pdf_blks_acquire.py`).
//!
//! Contratto: `agent-memory/M6-implementation-plan.md` §3.2. **Deviazione dal contratto letterale
//! del piano**, necessaria per compilare: `LineSelectionError` qui sotto deriva solo `Debug` (non
//! anche `Clone, PartialEq` come scritto nel piano) perché la sua variante `Area` avvolge
//! [`PositionError`] (`formats_utils::pdf_extract::position`, M3, chiuso), che deriva solo
//! `Debug` — `#[derive(PartialEq)]` su questo enum non compilerebbe altrimenti. I test sotto usano
//! pattern-matching (`let LineSelectionError::Area(PositionError::XMinNotPositive(v)) = err else
//! { panic!(...) }`) invece di `assert_eq!` diretto sull'errore, stesso stile già usato dai test
//! di `position.rs` per `PositionError` stesso.

use once_cell::sync::Lazy;
use onig::Regex;

// Non usato dal codice di produzione di questo modulo (nessuna chiamata a `.contains()` qui): solo
// dai test annidati sotto (`mod tests { use super::*; ... }`), che chiamano `PdfLineSet::contains`
// nell'helper `selects`. `#[cfg(test)]` evita un "unused import" sulla build non-test.
#[cfg(test)]
use crate::commons::sets::Container;
use crate::formats_utils::pdf_extract::position::{InputArea, PositionError};
use crate::formats_utils::pdf_extract::relative::OptionallyRelative;
use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;
use crate::formats_utils::pdf_extract::select::pdf_line::area::Area;
use crate::formats_utils::pdf_extract::select::pdf_line::font::FontSet;
use crate::formats_utils::pdf_extract::select::pdf_line::font_size::FontSizeInterval;
use crate::formats_utils::pdf_extract::select::pdf_line::text::TextSet;
use crate::formats_utils::pdf_extract::select::relative::PdfLineSelection;

/// `str` singolo o lista: la forma `font: Optional[str | List[str]]` di `InputPdfLineSet`
/// (Python). `#[serde(untagged)]`: un valore YAML/JSON scalare diventa `Single`, una sequenza
/// diventa `Multiple`.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(untagged)]
pub enum FontCriterion {
    Single(String),
    Multiple(Vec<String>),
}

/// Specchio di `InputArea` (M3) lato deserializzazione: 4 bound opzionali, non ancora validati.
/// `InputArea::build` resta l'unico punto di validazione.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Deserialize)]
pub struct InputAreaSpec {
    #[serde(default)]
    pub x_min: Option<f32>,
    #[serde(default)]
    pub x_max: Option<f32>,
    #[serde(default)]
    pub y_min: Option<f32>,
    #[serde(default)]
    pub y_max: Option<f32>,
}

/// Traduzione diretta di `InputPdfLineSet` (Pydantic, riferimento) — stessi quattro campi, tutti
/// opzionali.
#[derive(Debug, Clone, PartialEq, Default, serde::Deserialize)]
pub struct InputPdfLineSet {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub font: Option<FontCriterion>,
    #[serde(default)]
    pub font_size: Option<f32>,
    #[serde(default)]
    pub area: Option<InputAreaSpec>,
}

#[derive(Debug, thiserror::Error)]
pub enum LineSelectionError {
    #[error("font_size must be positive, found {0}")]
    FontSizeNotPositive(f32),
    /// `font` presente ma vuoto (`[]`): nel riferimento Python `functools.reduce(or_, [])` va in
    /// `TypeError` non gestito — qui diventa un errore tipizzato invece di un panic.
    #[error("font list must not be empty when provided")]
    EmptyFontList,
    #[error(transparent)]
    Area(#[from] PositionError),
}

/// Precisione dell'intervallo di corpo font costruito attorno a un `font_size` esatto, verbatim
/// dal riferimento (`max(fs - 1e-3, 0.0)`, `fs + 1e-3`).
const FONT_SIZE_PRECISION: f32 = 1e-3;

/// Costruisce una `PdfLineSelection` (sempre `Absolute`, mai `Relative`) intersecando i criteri
/// presenti.
pub fn pdfline_selection_from_dict(data: &InputPdfLineSet) -> Result<PdfLineSelection, LineSelectionError> {
    // Called once per selection spec while a formats repo loads, potentially thousands of times
    // across a whole repo (rule 2): never above `trace!`.
    tracing::trace!(?data, "building a pdf line selection from a dict spec");

    let font_size_set = match data.font_size {
        Some(fs) if fs <= 0.0 => return Err(LineSelectionError::FontSizeNotPositive(fs)),
        Some(fs) => Some(FontSizeInterval::from_precision(fs, FONT_SIZE_PRECISION)),
        None => None,
    };

    let area_set = match &data.area {
        Some(spec) => {
            let input_area = InputArea::build(spec.x_min, spec.x_max, spec.y_min, spec.y_max)?;
            let x_min = input_area.x_min().unwrap_or(0.0);
            let y_min = input_area.y_min().unwrap_or(0.0);
            let x_max = input_area.x_max().unwrap_or(1e6);
            let y_max = input_area.y_max().unwrap_or(1e6);
            Some(Area::new(x_min, y_min, x_max, y_max))
        }
        None => None,
    };

    let text_set = data.text.as_deref().map(TextSet::new);

    let base = PdfLineSet::from_sets(None, font_size_set, text_set, area_set);

    let selection = match &data.font {
        None => base,
        Some(FontCriterion::Single(font)) => PdfLineSet::font(FontSet::new(font)) & base,
        Some(FontCriterion::Multiple(fonts)) => {
            let font_set = fonts
                .iter()
                .map(|f| FontSet::new(f))
                .reduce(|a, b| a | b)
                .ok_or(LineSelectionError::EmptyFontList)?;
            PdfLineSet::font(font_set) & base
        }
    };

    Ok(OptionallyRelative::Absolute(selection))
}

// Porting di `LINE_SET_REGEXP` (`pdf_blks_acquire.py`): concatenazione di font / `[font_size]` /
// area (`y_range` da sola o l'intera `area` avvolta da una coppia di parentesi in piu') / testo
// fra virgolette, ciascuno separato da uno spazio opzionale, tutti i gruppi opzionali. A
// differenza del riferimento (named groups + post-processing via `_to_floats`), qui i cinque
// gruppi utili sono catturati per posizione (`Captures::at`, D-M6-7: `onig` non espone un
// accessorio per nome comodo quanto `re.Match.groupdict()`), nell'ordine: 1 font, 2 font_size,
// 3 y_range (intero, comprese le parentesi), 4 area (intera, senza le parentesi esterne),
// 5 text. Ancorato con `\A`: `onig::Regex::captures` cerca ovunque, mentre `re.match` di Python
// tenta solo dalla posizione 0.
static LINE_SET_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"\A([\w\-, ]+)? ?(?:\[([0-9]+(?:\.[0-9]+)?)\])? ?(?:(\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\))|\((\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\)\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\))\))? ?(?:"(.*)")?"#,
    )
    .expect("fixed, hand-written pattern, valid onig regex")
});

/// Il pattern di [`LINE_SET_REGEXP`], ma ancorato **anche in fondo**.
///
/// Serve a `formats_repo::structured`, non a chi analizza una selezione: ogni gruppo del pattern è
/// opzionale, quindi la versione non ancorata a destra combacia con *qualunque* stringa e
/// [`pdfline_selection_from_str`] non rifiuta mai nulla — una cella scritta male produrrebbe in
/// silenzio una selezione vuota. La validazione delle tabelle CSV del repo formati ha invece
/// bisogno di dire "questa cella non è una selezione", ed è esattamente ciò che il riferimento fa
/// con il suo `x.str.match(f"^{LINE_SET_REGEXP_PATTERN}$")` di pandera.
static LINE_SET_ANCHORED_REGEXP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"\A([\w\-, ]+)? ?(?:\[([0-9]+(?:\.[0-9]+)?)\])? ?(?:(\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\))|\((\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\)\((?:[0-9]+(?:\.[0-9]+)?)?:(?:[0-9]+(?:\.[0-9]+)?)?\))\))? ?(?:"(.*)")?\z"#,
    )
    .expect("fixed, hand-written pattern, valid onig regex")
});

/// `true` se `input` è per intero una selezione di righe scritta nella grammatica compatta.
///
/// Vedi [`LINE_SET_ANCHORED_REGEXP`] per perché questo controllo non coincide con "
/// [`pdfline_selection_from_str`] ha restituito `Ok`".
pub fn is_pdfline_selection(input: &str) -> bool {
    LINE_SET_ANCHORED_REGEXP.find(input).is_some()
}

/// Divide una coppia `"a:b"` (senza parentesi) in due bound opzionali, come `_to_floats` nel
/// riferimento: un lato assente (stringa vuota) resta `None`.
fn parse_bound_pair(text: &str) -> (Option<f32>, Option<f32>) {
    let mut parts = text.splitn(2, ':');
    let a = parts.next().unwrap_or("");
    let b = parts.next().unwrap_or("");
    let parse = |s: &str| (!s.is_empty()).then(|| s.parse::<f32>().expect("digits matched by LINE_SET_REGEXP always parse as f32"));
    (parse(a), parse(b))
}

/// Analizza la grammatica compatta (font / `[font_size]` / area `(x0:x1)(y0:y1)` o `(y0:y1)` /
/// `"text"`) in un `InputPdfLineSet`, poi **delega** a [`pdfline_selection_from_dict`]
/// (D-M6-6 di `agent-memory/M6-implementation-plan.md`).
pub fn pdfline_selection_from_str(input: &str) -> Result<PdfLineSelection, LineSelectionError> {
    // Same volume caveat as `pdfline_selection_from_dict` (rule 2): `trace!` only.
    tracing::trace!(input, "parsing a compact pdf line selection expression");

    let captures = LINE_SET_REGEXP
        .captures(input)
        .expect("every group in LINE_SET_REGEXP is optional, so it matches any string, including the empty one");

    let font = captures.at(1).map(|f| FontCriterion::Single(f.trim().to_string()));
    let font_size =
        captures.at(2).map(|s| s.parse::<f32>().expect("digits matched by LINE_SET_REGEXP always parse as f32"));

    let area = if let Some(y_range) = captures.at(3) {
        // `y_range` e' l'intera stringa "(a:b)", comprese le parentesi.
        let (y_min, y_max) = parse_bound_pair(&y_range[1..y_range.len() - 1]);
        Some(InputAreaSpec { x_min: None, x_max: None, y_min, y_max })
    } else if let Some(area_text) = captures.at(4) {
        // `area_text` e' "(a:b)(c:d)" (senza le parentesi esterne di avvolgimento): lo stesso
        // `tmp_area.split(")(")` del riferimento separa le due coppie.
        let (x_part, y_part) = area_text
            .split_once(")(")
            .expect("area_text is always shaped \"(a:b)(c:d)\" by construction of LINE_SET_REGEXP");
        let (x_min, x_max) = parse_bound_pair(x_part.trim_start_matches('('));
        let (y_min, y_max) = parse_bound_pair(y_part.trim_end_matches(')'));
        Some(InputAreaSpec { x_min, x_max, y_min, y_max })
    } else {
        None
    };

    let text = captures.at(5).map(|t| t.to_string());

    pdfline_selection_from_dict(&InputPdfLineSet { text, font, font_size, area })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use crate::formats_utils::pdf_extract::relative::OptionallyRelative;

    fn line(font: &str, size: f32, text: &str, bbox: (f32, f32, f32, f32)) -> PdfLine {
        PdfLine::new(font, size, text, bbox)
    }

    /// `PdfLineSelection` (= `OptionallyRelative<PdfLineSet, RelativePdfLineSet>`) non deriva
    /// `Debug`: `Result::unwrap_err` lo richiederebbe comunque (per il messaggio di panico sul
    /// ramo `Ok` che qui non prendiamo mai), quindi va evitato con un match esplicito invece che
    /// con `.unwrap_err()`.
    fn expect_err(result: Result<PdfLineSelection, LineSelectionError>) -> LineSelectionError {
        match result {
            Ok(_) => panic!("expected a LineSelectionError, got Ok(..)"),
            Err(e) => e,
        }
    }

    /// `pdfline_selection_from_dict`/`_from_str` producono sempre una selezione `Absolute`
    /// (mai `Relative`, D-M6-6 del piano): questo helper lo assume e fallisce rumorosamente se
    /// non e' cosi', invece di confrontare direttamente `PdfLineSelection` (che non deriva
    /// `PartialEq` in un modo utilizzabile qui).
    fn selects(selection: &PdfLineSelection, probe: &PdfLine) -> bool {
        match selection {
            OptionallyRelative::Absolute(set) => set.contains(probe),
            OptionallyRelative::Relative(_) => {
                panic!("pdfline_selection_from_dict/_from_str must never build a Relative selection")
            }
        }
    }

    mod pdfline_selection_from_dict_behavior {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn with_every_field_absent_it_accepts_any_line() {
            let selection = pdfline_selection_from_dict(&InputPdfLineSet::default()).unwrap();
            assert!(selects(&selection, &line("Arial", 12.0, "anything", (0.0, 0.0, 10.0, 10.0))));
            assert!(selects(&selection, &line("Times New Roman", 999.0, "", (500.0, 500.0, 600.0, 600.0))));
        }

        #[test]
        fn a_single_font_accepts_only_that_font() {
            let data = InputPdfLineSet { font: Some(FontCriterion::Single("Arial".to_string())), ..Default::default() };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Times", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn a_list_of_two_or_more_fonts_accepts_the_union() {
            let data = InputPdfLineSet {
                font: Some(FontCriterion::Multiple(vec!["Arial".to_string(), "Times".to_string()])),
                ..Default::default()
            };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(selects(&selection, &line("Times", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Courier", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn font_size_selects_a_narrow_interval_around_the_given_value() {
            let data = InputPdfLineSet { font_size: Some(12.0), ..Default::default() };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(&selection, &line("Arial", 12.0005, "x", (0.0, 0.0, 1.0, 1.0))), "just inside [11.999, 12.001]");
            assert!(!selects(&selection, &line("Arial", 12.005, "x", (0.0, 0.0, 1.0, 1.0))), "clearly outside [11.999, 12.001]");
        }

        #[test]
        fn area_with_partial_bounds_defaults_missing_bounds_to_zero_and_one_million() {
            // x_max/y_min assenti: sostituiti da 1e6/0.0 come da riferimento.
            let data = InputPdfLineSet {
                area: Some(InputAreaSpec { x_min: Some(5.0), x_max: None, y_min: None, y_max: Some(50.0) }),
                ..Default::default()
            };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            // Dentro (5.0, 0.0, 1e6, 50.0).
            assert!(selects(&selection, &line("Arial", 10.0, "x", (10.0, 10.0, 20.0, 20.0))));
            // Fuori: x0 < 5.0.
            assert!(!selects(&selection, &line("Arial", 10.0, "x", (1.0, 10.0, 4.0, 20.0))));
            // Fuori: y1 > 50.0.
            assert!(!selects(&selection, &line("Arial", 10.0, "x", (10.0, 10.0, 20.0, 60.0))));
        }

        #[test]
        fn text_is_passed_through_to_the_text_set_unchanged() {
            let data = InputPdfLineSet { text: Some("^foo".to_string()), ..Default::default() };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "foobar", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Arial", 10.0, "barfoo", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn two_criteria_present_together_intersect_rather_than_union() {
            let data = InputPdfLineSet { font: Some(FontCriterion::Single("Arial".to_string())), font_size: Some(12.0), ..Default::default() };
            let selection = pdfline_selection_from_dict(&data).unwrap();
            assert!(selects(&selection, &line("Arial", 12.0, "x", (0.0, 0.0, 1.0, 1.0))), "matches both");
            assert!(!selects(&selection, &line("Arial", 50.0, "x", (0.0, 0.0, 1.0, 1.0))), "font matches, size doesn't");
            assert!(!selects(&selection, &line("Times", 12.0, "x", (0.0, 0.0, 1.0, 1.0))), "size matches, font doesn't");
        }

        #[test]
        fn a_non_positive_font_size_is_rejected() {
            let zero = InputPdfLineSet { font_size: Some(0.0), ..Default::default() };
            let err = expect_err(pdfline_selection_from_dict(&zero));
            let LineSelectionError::FontSizeNotPositive(v) = err else { panic!("expected FontSizeNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);

            let negative = InputPdfLineSet { font_size: Some(-3.0), ..Default::default() };
            let err = expect_err(pdfline_selection_from_dict(&negative));
            let LineSelectionError::FontSizeNotPositive(v) = err else { panic!("expected FontSizeNotPositive, got {err:?}") };
            assert_eq!(v, -3.0);
        }

        #[test]
        fn an_explicitly_empty_font_list_is_rejected() {
            let data = InputPdfLineSet { font: Some(FontCriterion::Multiple(vec![])), ..Default::default() };
            let err = expect_err(pdfline_selection_from_dict(&data));
            assert!(matches!(err, LineSelectionError::EmptyFontList));
        }

        #[test]
        fn an_invalid_area_bubbles_up_a_position_error() {
            let data = InputPdfLineSet { area: Some(InputAreaSpec { x_min: Some(0.0), ..Default::default() }), ..Default::default() };
            let err = expect_err(pdfline_selection_from_dict(&data));
            let LineSelectionError::Area(PositionError::XMinNotPositive(v)) = err else { panic!("expected Area(XMinNotPositive), got {err:?}") };
            assert_eq!(v, 0.0);
        }
    }

    mod pdfline_selection_from_str_behavior {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_bare_font_selects_only_that_font() {
            let selection = pdfline_selection_from_str("Arial").unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Times", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn a_bracketed_number_alone_selects_that_font_size() {
            let selection = pdfline_selection_from_str("[12.0]").unwrap();
            assert!(selects(&selection, &line("Arial", 12.0005, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(selects(&selection, &line("Times", 12.0005, "x", (0.0, 0.0, 1.0, 1.0))), "font unrestricted");
            assert!(!selects(&selection, &line("Arial", 12.005, "x", (0.0, 0.0, 1.0, 1.0))));
        }

        /// Grammatica dell'area piena: il gruppo "area" del riferimento richiede una coppia di
        /// intervalli avvolta da una *ulteriore* coppia di parentesi (`\((x0:x1)(y0:y1)\)`), non
        /// solo `(x0:x1)(y0:y1)` nudo — verificato riproducendo il regex del riferimento fuori da
        /// questo crate: `(x0:x1)(y0:y1)` senza le parentesi esterne matcha invece il solo
        /// `y_range` (il primo gruppo, ignorando silenziosamente il resto), non l'area completa.
        #[test]
        fn a_double_parenthesized_range_pair_selects_the_full_area() {
            let selection = pdfline_selection_from_str("((1:10)(2:20))").unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "x", (3.0, 5.0, 6.0, 8.0))), "strictly inside (1,2,10,20)");
            assert!(!selects(&selection, &line("Arial", 10.0, "x", (50.0, 5.0, 60.0, 8.0))), "outside on x");
        }

        #[test]
        fn a_single_parenthesized_range_selects_a_vertical_band() {
            let selection = pdfline_selection_from_str("(2:20)").unwrap();
            // x e' illimitato (default 0.0..1e6), solo y e' vincolato a [2, 20].
            assert!(selects(&selection, &line("Arial", 10.0, "x", (100.0, 5.0, 200.0, 10.0))));
            assert!(!selects(&selection, &line("Arial", 10.0, "x", (100.0, 30.0, 200.0, 40.0))));
        }

        #[test]
        fn a_quoted_string_alone_selects_that_text() {
            let selection = pdfline_selection_from_str("\"foo\"").unwrap();
            assert!(selects(&selection, &line("Arial", 10.0, "a foo b", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Arial", 10.0, "bar", (0.0, 0.0, 1.0, 1.0))));
        }

        #[test]
        fn an_empty_string_selects_everything() {
            let selection = pdfline_selection_from_str("").unwrap();
            assert!(selects(&selection, &line("Whatever", 999.0, "anything", (500.0, 500.0, 600.0, 600.0))));
        }

        #[test]
        fn groups_separated_by_spaces_combine_with_intersection() {
            let selection = pdfline_selection_from_str("Arial [12.0] ((1:10)(2:20)) \"foo\"").unwrap();
            assert!(selects(&selection, &line("Arial", 12.0, "a foo b", (3.0, 5.0, 6.0, 8.0))), "matches all four criteria");
            assert!(!selects(&selection, &line("Times", 12.0, "a foo b", (3.0, 5.0, 6.0, 8.0))), "font alone fails");
            assert!(!selects(&selection, &line("Arial", 12.0, "bar", (3.0, 5.0, 6.0, 8.0))), "text alone fails");
        }

        /// La classe di caratteri del font (`[\w\-, ]+`) include la virgola: una virgola
        /// letterale nel font non spezza la cattura in due criteri distinti, resta un unico
        /// `FontCriterion::Single`. Se venisse (a torto) trattata come unione di due font
        /// separati ("Arial" O "Bold"), una riga con font "Bold" da solo passerebbe: non deve.
        #[test]
        fn a_font_containing_a_literal_comma_is_a_single_criterion_not_split_on_the_comma() {
            let selection = pdfline_selection_from_str("Arial,Bold").unwrap();
            assert!(selects(&selection, &line("Arial,Bold", 10.0, "x", (0.0, 0.0, 1.0, 1.0))));
            assert!(!selects(&selection, &line("Bold", 10.0, "x", (0.0, 0.0, 1.0, 1.0))), "must not be treated as font Arial OR font Bold");
            assert!(!selects(&selection, &line("Arial", 10.0, "x", (0.0, 0.0, 1.0, 1.0))), "must not be treated as font Arial OR font Bold");
        }

        #[test]
        fn agrees_with_from_dict_on_an_equivalent_hand_built_criterion_set() {
            let via_str = pdfline_selection_from_str("Arial [12.0] ((1:10)(2:20)) \"foo\"").unwrap();
            let via_dict = pdfline_selection_from_dict(&InputPdfLineSet {
                font: Some(FontCriterion::Single("Arial".to_string())),
                font_size: Some(12.0),
                area: Some(InputAreaSpec { x_min: Some(1.0), x_max: Some(10.0), y_min: Some(2.0), y_max: Some(20.0) }),
                text: Some("foo".to_string()),
            })
            .unwrap();

            let probes = [
                line("Arial", 12.0, "a foo b", (3.0, 5.0, 6.0, 8.0)),
                line("Times", 12.0, "a foo b", (3.0, 5.0, 6.0, 8.0)),
                line("Arial", 50.0, "a foo b", (3.0, 5.0, 6.0, 8.0)),
                line("Arial", 12.0, "bar", (3.0, 5.0, 6.0, 8.0)),
                line("Arial", 12.0, "a foo b", (50.0, 50.0, 60.0, 60.0)),
            ];
            for probe in probes {
                assert_eq!(selects(&via_str, &probe), selects(&via_dict, &probe), "mismatch for {probe:?}");
            }
        }

        #[test]
        fn a_non_positive_font_size_error_propagates_from_from_dict() {
            let err = expect_err(pdfline_selection_from_str("[0.0]"));
            let LineSelectionError::FontSizeNotPositive(v) = err else { panic!("expected FontSizeNotPositive, got {err:?}") };
            assert_eq!(v, 0.0);
        }

        #[test]
        fn an_invalid_area_error_propagates_from_from_dict() {
            let err = expect_err(pdfline_selection_from_str("((0:10)(2:20))"));
            let LineSelectionError::Area(PositionError::XMinNotPositive(v)) = err else { panic!("expected Area(XMinNotPositive), got {err:?}") };
            assert_eq!(v, 0.0);
        }
    }

    mod line_selection_error_display {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn font_size_not_positive_displays_the_offending_value() {
            assert_eq!(LineSelectionError::FontSizeNotPositive(-2.5).to_string(), "font_size must be positive, found -2.5");
        }

        #[test]
        fn empty_font_list_displays_a_fixed_message() {
            assert_eq!(LineSelectionError::EmptyFontList.to_string(), "font list must not be empty when provided");
        }

        #[test]
        fn area_displays_transparently_as_the_wrapped_position_error() {
            let inner = PositionError::XMinNotPositive(0.0);
            let expected = inner.to_string();
            assert_eq!(LineSelectionError::Area(inner).to_string(), expected);
        }
    }
}
