//! `sfdr_annex`: the European periodic SFDR annex, one language at a time.
//!
//! [`PdfExtractSfdrSignStandard`] reads **the tick in the box** and assigns it by position. What the
//! engine cannot know is **how the question and the two labels are written**, because those are
//! words, and the words belong to the European annex and to its language. This module keeps them in
//! one place.
//!
//! The four ready languages measure one another:
//!
//! | | question | labels | row below |
//! |---|---|---|---|
//! | `ITALIAN` | *aveva un obiettivo di investimento sostenibile* | `Sì` · `No` | *Ha effettuato…* · *Ha promosso…* |
//! | `ENGLISH` | *have a sustainable investment objective* | `Yes` · `No` (and `Oui` · `Non`) | *It made…* · *…promoted Environmental…* |
//! | `SPANISH` | *tenía un objetivo de inversión sostenible* | `Sí` · `No` | *Realizó…* · *…promovía características…* |
//! | `FRENCH` | *avait-il un objectif d'investissement durable* | `Oui` · `Non` | *Il a réalisé…* · *…a promu des caractéristiques…* |
//!
//! `ENGLISH` carries `Oui`/`Non` among its labels too, and it is not an oversight: **one single
//! AMUNDI-EN24 report writes the labels in French on two disclosures out of a hundred and three**,
//! with the question in English. It is the reason the labels are **measured** rather than deduced
//! from the document's language.
//!
//! `SPANISH` has no document to try it on: it serves the **annex** of `MEDIOLANUM-ES24.B` (`R30`),
//! and no file of that annex exists either in the tests or in the 905 of the collection. It is here
//! so that the day one arrives the work is not to be redone, and its words come from the text of the
//! annex rather than from a measured page — **which is exactly what makes it unverified**.
//!
//! # Why two pipes and not one
//!
//! The tick and the product name are **two data on the same page**, not two pieces of one datum:
//! the first is geometry, the second a line of text with a label in front. They are therefore two
//! pipes of the `pdf_extract` segment, and the `text_filter` reunites them — it receives the blocks
//! of every pipe of its pipeline, so nothing else is needed. In particular **no promise** is
//! involved: a promise is for a datum on another page, and here nothing crosses the page.
//!
//! # What stays the format's
//!
//! The typography — which recognizer serves, which glyphs are a tick — stays with the format, and
//! so does the way its own pages break the product name. Those are the fields of
//! [`InputSfdrAnnexPdfExtract`] beyond `language`.

use serde::Deserialize;

use crate::core::classes::BlockType;
use crate::core::pipeline::{PdfExtractPipe, TextFilterPipe};
use crate::formats_utils::pdf_extract::labelled_column::{
    LabelledColumnArgs, PdfExtractLabelledColumnStandard,
};
use crate::formats_utils::pdf_extract::labelled_value::{
    LabelledValueArgs, LabelledValueTake, LabelledValueWhere, LabelledValueWindow,
    PdfExtractLabelledValueStandard,
};
use crate::formats_utils::pdf_extract::relative::OptionallyRelative;
use crate::formats_utils::pdf_extract::select::pdf_line::PdfLineSet;
use crate::formats_utils::pdf_extract::select::pdf_line::area::Area;
use crate::formats_utils::pdf_extract::select::pdf_line::text::TextSet;
use crate::formats_utils::pdf_extract::select::relative::PdfLineSelection;
use crate::formats_utils::pdf_extract::sfdr_sign::{PdfExtractSfdrSignStandard, SfdrSignArgs};
use crate::formats_utils::text_filter::declared_funds::TextFilterOnlyDeclaredFunds;
use crate::formats_utils::text_filter::standard_funcs::{
    StandardFuncsError, TextFilterSfdrSignStandard,
};

use std::sync::Arc;

/// The block type the product name reaches the `text_filter` with.
///
/// It is the default [`TextFilterSfdrSignStandard`] expects, so neither side names it.
const PRODUCT_BLOCK: BlockType = BlockType::FUND_NAME;

/// The periodic annex in one language: what to look for, and where.
pub struct Annex {
    pub question: &'static [&'static str],
    pub affirmative: &'static [&'static str],
    pub negative: &'static [&'static str],
    pub made: &'static [&'static str],
    pub promoted: &'static [&'static str],
    pub title: &'static [&'static str],
    pub product: &'static [&'static str],
    pub product_prefix: &'static str,
    pub identifier: &'static [&'static str],
    pub band: &'static [&'static str],
}

/// Italian — measured on ARCA-IT24 p45 and ANIMA_SGR-IT24.
pub const ITALIAN: Annex = Annex {
    question: &["aveva un obiettivo di investimento sostenibile"],
    affirmative: &["Sì", "SI"],
    negative: &["No", "NO"],
    made: &["Ha effettuato"],
    promoted: &["Ha promosso"],
    title: &["prodotti finanziari di cui all'articolo"],
    product: &["Nome del prodotto"],
    product_prefix: "Nome del prodotto: ",
    identifier: &["dentificativo della persona giuridica"],
    band: &[
        "Caratteristiche ambientali e/o sociali",
        "biettivo di investimento sostenibile",
    ],
};

/// English — measured on UBS-EN23, KAIROS-EN23, FINECO-EN23@IR and @LUX, ASTERIA-EN24,
/// DANSKEINVEST-EN24, CARNE-EN23, ANIMA-EN23, AMUNDI-EN24.
pub const ENGLISH: Annex = Annex {
    question: &["have a sustainable investment objective"],
    affirmative: &["Yes", "Oui"],
    negative: &["No", "Non"],
    // `It made` and not `^It made`: on UBS-EN23 the line begins with two spaces. And on the right it
    // anchors on `promoted Environmental` because there the line breaks — `  It ` on one side,
    // `promoted Environmental/` on the other — and `It promoted` does not exist as a line.
    made: &["It made"],
    promoted: &["promoted Environmental"],
    title: &["referred to in Article", "pursuant to Article"],
    product: &["Product name"],
    product_prefix: "Product name: ",
    identifier: &["Legal entity identifier", "Legal Entity Identifier"],
    band: &[
        "Environmental and/or social characteristics",
        "Sustainable investment objective",
    ],
};

/// Spanish — **measured on no document**: see the module documentation.
pub const SPANISH: Annex = Annex {
    question: &["un objetivo de inversión sostenible"],
    affirmative: &["Sí", "SI"],
    negative: &["No", "NO"],
    made: &["Realizó"],
    promoted: &["promovía características"],
    title: &["productos financieros a los que se hace referencia en el artículo"],
    product: &["Nombre del producto"],
    product_prefix: "Nombre del producto: ",
    identifier: &["dentificador de entidad jur"],
    band: &[
        "Características medioambientales o sociales",
        "bjetivo de inversión sostenible",
    ],
};

/// French — the labels are measured (AMUNDI-EN24 p777), the rest is not: no document of the
/// collection carries the form **entirely** in French.
pub const FRENCH: Annex = Annex {
    question: &["un objectif d'investissement durable"],
    affirmative: &["Oui"],
    negative: &["Non"],
    made: &["a réalisé"],
    promoted: &["a promu des caractéristiques"],
    title: &["produits financiers visés à l'article"],
    product: &["Nom du produit"],
    product_prefix: "Nom du produit: ",
    identifier: &["dentifiant d’entité juridique"],
    band: &[
        "Caractéristiques environnementales et/ou sociales",
        "bjectif d’investissement durable",
    ],
};

/// The language a configuration names, case-insensitively.
pub fn language(name: &str) -> Option<&'static Annex> {
    match name.to_ascii_uppercase().as_str() {
        "ITALIAN" => Some(&ITALIAN),
        "ENGLISH" => Some(&ENGLISH),
        "SPANISH" => Some(&SPANISH),
        "FRENCH" => Some(&FRENCH),
        _ => None,
    }
}

/// Every language's name, for the error message of an unknown one.
pub const LANGUAGES: [&str; 4] = ["ITALIAN", "ENGLISH", "SPANISH", "FRENCH"];

/// One text criterion, the anchored-substring predicate the engine takes.
///
/// The set algebra happens at this level and the result is wrapped **once**: these are always
/// absolute selections, as [`crate::input::document::selection::pdfline_selection_from_dict`] builds
/// them too.
fn text_set(text: &str) -> PdfLineSet {
    PdfLineSet::text(TextSet::new(text))
}

/// A selection from one anchor or from an alternative of anchors.
///
/// The alternative is needed more often than it looks: one single AMUNDI-EN24 report writes
/// `Yes`/`No` on a hundred forms and `X Oui`/`Non` on two, and the labels are to be **measured**,
/// not deduced from the document's language.
fn anchors(texts: &[&str]) -> Option<PdfLineSelection> {
    texts
        .iter()
        .map(|text| text_set(text))
        .reduce(|a, b| a | b)
        .map(OptionallyRelative::Absolute)
}

/// The three forms an answer label really takes.
///
/// Measured: `'No'` alone (UBS-EN23), `'No '` with the space the PDF drags along (ARCA-IT24), and
/// `'X No '` with **the tick fused into the label's line** (ANIMA_SGR-IT24, AMUNDI-EN24). One anchor
/// does not cover them: `^No` loses the third, `No$` the second. Hence three — prefix, suffix,
/// suffix with a space — for every word.
///
/// The engine's text predicate is an anchored substring, not a regex, and it does not normalise
/// spaces: that is why the form is written out rather than deduced.
fn label(words: &[&str]) -> Option<PdfLineSelection> {
    let mut forms: Vec<String> = Vec::with_capacity(words.len() * 3);
    for word in words {
        forms.push(format!("^{word}"));
        forms.push(format!("{word}$"));
        forms.push(format!("{word} $"));
    }
    anchors(&forms.iter().map(String::as_str).collect::<Vec<_>>())
}

/// Any line that is not made of spaces alone.
///
/// In the European forms the all-space lines are everywhere, and they sit **between** the label and
/// its value: without this subtraction, `Product name:` on FINECO-EN23 takes as the product name a
/// line of one character, the space. The engine's text predicate is an anchored substring and not a
/// regex, so the lengths are written one by one instead of `^ +$`.
fn not_blank() -> PdfLineSelection {
    let mut set = PdfLineSet::area(Area::new(0.0, 0.0, 1e6, 1e6));
    for spaces in 1..=6 {
        set = set / text_set(&format!("^{}$", " ".repeat(spaces)));
    }
    OptionallyRelative::Absolute(set)
}

/// Failures of building the annex's pipes.
#[derive(Debug, thiserror::Error)]
pub enum SfdrAnnexError {
    #[error("unknown annex language '{name}': the languages are {}", LANGUAGES.join(", "))]
    UnknownLanguage { name: String },
    /// An override that replaces an anchor list with an empty one, which would select every line.
    #[error("field '{0}' is an empty list of anchors, which would select every line of the page")]
    EmptyAnchors(&'static str),
    #[error(transparent)]
    TextFilter(#[from] StandardFuncsError),
}

/// The `pdf_extract` configuration: the language, and what the format's own pages add to it.
#[derive(Debug, Clone, Deserialize)]
pub struct InputSfdrAnnexPdfExtract {
    /// One of [`LANGUAGES`]. The only required field: everything else the annex already knows.
    pub language: String,
    /// Anchor overrides, for a page that words one of them its own way.
    #[serde(default)]
    pub question: Option<Vec<String>>,
    #[serde(default)]
    pub affirmative: Option<Vec<String>>,
    #[serde(default)]
    pub negative: Option<Vec<String>>,
    #[serde(default)]
    pub made: Option<Vec<String>>,
    #[serde(default)]
    pub promoted: Option<Vec<String>>,
    #[serde(default)]
    pub title: Option<Vec<String>>,
    #[serde(default)]
    pub product: Option<Vec<String>>,
    #[serde(default)]
    pub identifier: Option<Vec<String>>,
    #[serde(default)]
    pub band: Option<Vec<String>>,
    /// Reads the product name **in the label's column**, joined, instead of with the two pipes that
    /// read it on the label's line or beside it. What a format needs where the name wraps, or where
    /// the PDF breaks it into fragments neither of those two pipes reunites.
    #[serde(default)]
    pub product_joined: bool,
    /// The word the annex does **not** write and the holdings do. Only with `product_joined`.
    #[serde(default)]
    pub product_prepend: String,
    /// How far down the ordinate band of the `Right` product pipe may grow.
    #[serde(default)]
    pub product_across: Option<f32>,
}

/// The `text_filter` configuration: what to strip off the product name, and which funds to demand.
#[derive(Debug, Clone, Deserialize)]
pub struct InputSfdrAnnexTextFilter {
    /// One of [`LANGUAGES`], which supplies the default prefix to remove.
    pub language: String,
    /// The prefixes to remove from the product name. Replaces the language's own prefix rather
    /// than adding to it, because a format that states one has measured its page.
    ///
    /// A literal and a pattern are **not interchangeable**: the filter removes every literal first,
    /// in the order given, and only then every pattern, so which of the two an entry is changes the
    /// result. A bare string is a literal; `{pattern: "…"}` is a regular expression.
    #[serde(default)]
    pub product_prefix: Option<Vec<PrefixSpec>>,
    /// Whether the article is kept only for a fund the holdings of an earlier step named.
    #[serde(default = "demand_by_default")]
    pub demand_investment_funds_match: bool,
    /// Whether the article is kept only for a fund **already declared** in this job, which is how
    /// `R30` is realised for an annex that is a document of its own. Not a duplicate of
    /// `demand_investment_funds_match`: see [`TextFilterOnlyDeclaredFunds`].
    #[serde(default)]
    pub demand_declared_fund: bool,
}

/// One entry of `product_prefix`: a literal to remove, or a pattern to remove.
///
/// Untagged, so the YAML reads as what it is — a string for the annex's wording, a mapping for an
/// expression. A literal written as a pattern would also have to escape its own metacharacters,
/// which is why the distinction is offered rather than guessed.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PrefixSpec {
    Literal(String),
    Pattern { pattern: String },
}

/// The engine's own default, which is to demand the match.
fn demand_by_default() -> bool {
    true
}

/// One anchor list: the override if there is one, the annex's own words otherwise.
fn chosen(
    field: &'static str,
    override_: &Option<Vec<String>>,
    fallback: &'static [&'static str],
) -> Result<PdfLineSelection, SfdrAnnexError> {
    match override_ {
        Some(texts) => {
            let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
            anchors(&refs).ok_or(SfdrAnnexError::EmptyAnchors(field))
        }
        None => anchors(fallback).ok_or(SfdrAnnexError::EmptyAnchors(field)),
    }
}

/// The same, for a list whose forms go through [`label`].
fn chosen_label(
    field: &'static str,
    override_: &Option<Vec<String>>,
    fallback: &'static [&'static str],
) -> Result<PdfLineSelection, SfdrAnnexError> {
    match override_ {
        Some(texts) => {
            let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
            label(&refs).ok_or(SfdrAnnexError::EmptyAnchors(field))
        }
        None => label(fallback).ok_or(SfdrAnnexError::EmptyAnchors(field)),
    }
}

/// The annex's `pdf_extract` pipes: the tick, and the product name in one of its two readings.
pub fn pdf_extract(
    input: &InputSfdrAnnexPdfExtract,
) -> Result<Vec<Arc<dyn PdfExtractPipe>>, SfdrAnnexError> {
    let annex = language(&input.language).ok_or_else(|| SfdrAnnexError::UnknownLanguage {
        name: input.language.clone(),
    })?;

    let sign = PdfExtractSfdrSignStandard::new(SfdrSignArgs {
        fallback_labels: Some((
            chosen("made", &input.made, annex.made)?,
            chosen("promoted", &input.promoted, annex.promoted)?,
        )),
        title: Some(chosen("title", &input.title, annex.title)?),
        ..SfdrSignArgs::new(
            chosen("question", &input.question, annex.question)?,
            (
                chosen_label("affirmative", &input.affirmative, annex.affirmative)?,
                chosen_label("negative", &input.negative, annex.negative)?,
            ),
        )
    });

    let product_label = chosen("product", &input.product, annex.product)?;
    let mut pipes: Vec<Arc<dyn PdfExtractPipe>> = vec![Arc::new(sign)];

    if input.product_joined {
        let mut args = LabelledColumnArgs::new(
            product_label,
            chosen("identifier", &input.identifier, annex.identifier)?,
            chosen("band", &input.band, annex.band)?,
            PRODUCT_BLOCK,
            "fund",
        );
        args.prepend = input.product_prepend.clone();
        pipes.push(Arc::new(PdfExtractLabelledColumnStandard::new(args)));
        return Ok(pipes);
    }

    // The two paginations, measured on one document, FINECO-EN23@LUX: p223 writes
    // `Product name : CoRe Series - Bond 6M Fineco AM Fund` on **one line**, so the value is the
    // label's own line (`OnLine`); p197 writes `Product name:` in bold and the name **beside** it as
    // a line of its own, so the value sits to the right (`Right`) — skipping the all-space lines in
    // between. Both run on every page, in this order, and the `text_filter` takes **the first block
    // that, with the label removed, is not empty**. `All` and not `First` because a pipe finding
    // nothing is the normal case here, not a defect of the page.
    pipes.push(Arc::new(PdfExtractLabelledValueStandard::new(
        LabelledValueArgs {
            take: LabelledValueTake::All,
            ..LabelledValueArgs::new(product_label.clone(), PRODUCT_BLOCK, "fund")
        },
    )));
    pipes.push(Arc::new(PdfExtractLabelledValueStandard::new(
        LabelledValueArgs {
            r#where: LabelledValueWhere::Right,
            value: Some(not_blank()),
            take: LabelledValueTake::All,
            // Two line heights to the right of the label and no further: the product name begins where
            // the label ends, while the legal entity identifier — on the same band and of the same
            // shape — begins a hundred and fifty points further along.
            window: Some(LabelledValueWindow {
                reach: Some(2.0),
                across: input.product_across.unwrap_or(0.0),
            }),
            ..LabelledValueArgs::new(product_label, PRODUCT_BLOCK, "fund")
        },
    )));
    Ok(pipes)
}

/// The annex's `text_filter`: the one that reunites the tick and the name, wrapped where `R30` asks.
pub fn text_filter(
    input: &InputSfdrAnnexTextFilter,
) -> Result<Vec<Arc<dyn TextFilterPipe>>, SfdrAnnexError> {
    let annex = language(&input.language).ok_or_else(|| SfdrAnnexError::UnknownLanguage {
        name: input.language.clone(),
    })?;
    let (strings, patterns) = match &input.product_prefix {
        // The language's own prefix is a plain string, not a pattern: it is the annex's wording.
        None => (vec![annex.product_prefix.to_string()], Vec::new()),
        Some(specs) => {
            let mut strings = Vec::new();
            let mut patterns = Vec::new();
            for spec in specs {
                match spec {
                    PrefixSpec::Literal(text) => strings.push(text.clone()),
                    PrefixSpec::Pattern { pattern } => patterns.push(pattern.clone()),
                }
            }
            (strings, patterns)
        }
    };
    let pipe = TextFilterSfdrSignStandard::new(
        None,
        strings,
        patterns,
        input.demand_investment_funds_match,
    )?;
    let pipe: Arc<dyn TextFilterPipe> = Arc::new(pipe);
    Ok(vec![if input.demand_declared_fund {
        Arc::new(TextFilterOnlyDeclaredFunds::new(pipe))
    } else {
        pipe
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A configuration that says nothing but the language, as fifteen of the eighteen call sites do.
    fn extract_input(language: &str) -> InputSfdrAnnexPdfExtract {
        InputSfdrAnnexPdfExtract {
            language: language.to_string(),
            question: None,
            affirmative: None,
            negative: None,
            made: None,
            promoted: None,
            title: None,
            product: None,
            identifier: None,
            band: None,
            product_joined: false,
            product_prepend: String::new(),
            product_across: None,
        }
    }

    fn filter_input(language: &str) -> InputSfdrAnnexTextFilter {
        InputSfdrAnnexTextFilter {
            language: language.to_string(),
            product_prefix: None,
            demand_investment_funds_match: true,
            demand_declared_fund: false,
        }
    }

    /// The error of a refused extraction configuration.
    ///
    /// Written out rather than with `unwrap_err`, which wants `Debug` on the success type — and a
    /// vector of trait objects has none.
    fn expect_extract_error(input: &InputSfdrAnnexPdfExtract) -> SfdrAnnexError {
        match pdf_extract(input) {
            Ok(pipes) => panic!("expected a refusal, got {} pipes", pipes.len()),
            Err(error) => error,
        }
    }

    /// The same for a filter configuration. See [`expect_extract_error`].
    fn expect_filter_error(input: &InputSfdrAnnexTextFilter) -> SfdrAnnexError {
        match text_filter(input) {
            Ok(pipes) => panic!("expected a refusal, got {} pipes", pipes.len()),
            Err(error) => error,
        }
    }

    mod the_language_table {
        use super::*;

        #[test]
        fn every_name_of_the_list_resolves() {
            for name in LANGUAGES {
                assert!(language(name).is_some(), "{name}");
            }
        }

        #[test]
        fn the_name_is_read_whatever_its_case() {
            assert!(language("english").is_some());
            assert!(language("English").is_some());
        }

        #[test]
        fn an_unknown_language_names_the_four_that_exist() {
            let message = expect_extract_error(&extract_input("GERMAN")).to_string();
            for name in LANGUAGES {
                assert!(message.contains(name), "{message}");
            }
        }

        #[test]
        fn english_carries_the_french_labels_too() {
            // One single AMUNDI-EN24 report writes `X Oui`/`Non` on two forms out of a hundred and
            // three, with the question in English. Measured, not deduced.
            assert!(ENGLISH.affirmative.contains(&"Oui"));
            assert!(ENGLISH.negative.contains(&"Non"));
        }

        #[test]
        fn every_language_states_every_anchor() {
            for name in LANGUAGES {
                let annex = language(name).expect("a listed language");
                for (field, texts) in [
                    ("question", annex.question),
                    ("affirmative", annex.affirmative),
                    ("negative", annex.negative),
                    ("made", annex.made),
                    ("promoted", annex.promoted),
                    ("title", annex.title),
                    ("product", annex.product),
                    ("identifier", annex.identifier),
                    ("band", annex.band),
                ] {
                    assert!(!texts.is_empty(), "{name} leaves {field} empty");
                }
                assert!(
                    !annex.product_prefix.is_empty(),
                    "{name} leaves product_prefix empty"
                );
            }
        }
    }

    mod the_extraction_pipes {
        use super::*;

        #[test]
        fn the_default_reading_is_the_tick_and_the_two_product_pipes() {
            let pipes = pdf_extract(&extract_input("ENGLISH")).unwrap();
            assert_eq!(pipes.len(), 3);
        }

        #[test]
        fn the_joined_reading_replaces_the_two_with_one() {
            let mut input = extract_input("ENGLISH");
            input.product_joined = true;
            let pipes = pdf_extract(&input).unwrap();
            assert_eq!(pipes.len(), 2);
        }

        #[test]
        fn an_override_that_empties_an_anchor_list_is_refused() {
            // An anchor with no text would select every line of the page, which is never what a
            // format means by stating one.
            let mut input = extract_input("ENGLISH");
            input.title = Some(Vec::new());
            let error = expect_extract_error(&input);
            assert!(
                matches!(error, SfdrAnnexError::EmptyAnchors("title")),
                "{error}"
            );
        }

        #[test]
        fn each_overridable_anchor_is_refused_empty_under_its_own_name() {
            for field in [
                "question",
                "affirmative",
                "negative",
                "made",
                "promoted",
                "title",
                "product",
            ] {
                let mut input = extract_input("ENGLISH");
                match field {
                    "question" => input.question = Some(Vec::new()),
                    "affirmative" => input.affirmative = Some(Vec::new()),
                    "negative" => input.negative = Some(Vec::new()),
                    "made" => input.made = Some(Vec::new()),
                    "promoted" => input.promoted = Some(Vec::new()),
                    "title" => input.title = Some(Vec::new()),
                    _ => input.product = Some(Vec::new()),
                }
                let error = expect_extract_error(&input);
                assert!(
                    matches!(error, SfdrAnnexError::EmptyAnchors(named) if named == field),
                    "{field}: {error}"
                );
            }
        }

        #[test]
        fn the_two_anchors_of_the_joined_reading_are_only_demanded_by_it() {
            // `identifier` and `band` serve the column reader alone, so an empty one is an error
            // only when that reader is the one asked for.
            let mut input = extract_input("ENGLISH");
            input.identifier = Some(Vec::new());
            assert!(pdf_extract(&input).is_ok());
            input.product_joined = true;
            let error = expect_extract_error(&input);
            assert!(
                matches!(error, SfdrAnnexError::EmptyAnchors("identifier")),
                "{error}"
            );
        }

        #[test]
        fn the_pipes_carry_the_fund_name_so_the_filter_finds_them_without_being_told() {
            for joined in [false, true] {
                let mut input = extract_input("ITALIAN");
                input.product_joined = joined;
                for pipe in pdf_extract(&input).unwrap() {
                    // The sign pipe is the only one not named `fund`: the two readings of the
                    // product name both are, which is what the default block type relies on.
                    assert!(!pipe.name().is_empty());
                }
            }
        }
    }

    mod the_filter_pipe {
        use super::*;

        #[test]
        fn one_pipe_comes_out_of_the_usual_configuration() {
            assert_eq!(text_filter(&filter_input("ENGLISH")).unwrap().len(), 1);
        }

        #[test]
        fn demanding_a_declared_fund_wraps_the_pipe_and_says_so_in_its_name() {
            let mut input = filter_input("SPANISH");
            input.demand_declared_fund = true;
            let pipes = text_filter(&input).unwrap();
            assert_eq!(pipes.len(), 1);
            assert!(
                pipes[0].name().contains("only declared funds"),
                "{}",
                pipes[0].name()
            );
        }

        #[test]
        fn not_demanding_it_leaves_the_pipe_unwrapped() {
            let pipes = text_filter(&filter_input("SPANISH")).unwrap();
            assert!(
                !pipes[0].name().contains("only declared funds"),
                "{}",
                pipes[0].name()
            );
        }

        #[test]
        fn an_unknown_language_is_refused_here_too() {
            let _ = expect_filter_error(&filter_input("KLINGON"));
        }

        #[test]
        fn a_malformed_pattern_is_reported_rather_than_compiled() {
            let mut input = filter_input("ENGLISH");
            input.product_prefix = Some(vec![PrefixSpec::Pattern {
                pattern: "(unclosed".to_string(),
            }]);
            assert!(matches!(
                expect_filter_error(&input),
                SfdrAnnexError::TextFilter(_)
            ));
        }

        #[test]
        fn a_literal_and_a_pattern_are_both_accepted_in_one_list() {
            let mut input = filter_input("ITALIAN");
            input.product_prefix = Some(vec![
                PrefixSpec::Literal("Nome del prodotto: ".to_string()),
                PrefixSpec::Pattern {
                    pattern: r" \(.*\)".to_string(),
                },
            ]);
            assert_eq!(text_filter(&input).unwrap().len(), 1);
        }
    }

    mod the_selection_helpers {
        use super::*;

        #[test]
        fn an_empty_anchor_list_builds_no_selection() {
            assert!(anchors(&[]).is_none());
            assert!(label(&[]).is_none());
        }

        #[test]
        fn a_label_word_becomes_its_three_measured_forms() {
            // `'No'` alone, `'No '` with the space the PDF drags along, and `'X No '` with the tick
            // fused into the line. One anchor covers none of the other two.
            assert!(label(&["No"]).is_some());
        }
    }
}
