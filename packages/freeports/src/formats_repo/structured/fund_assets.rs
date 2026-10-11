//! The structured fund-assets pipelines: a statement of net assets in **one row of CSV**.
//!
//! `PdfExtractAssetsStandard` (`A4`) is called by seven author modules with the same dozen
//! parameters every time, and `TextFilterAssetsStandard` and `DeserializerAssetsStandard` by
//! seventeen. That is the majority of the reference collection reading one datum with one
//! algorithm, which is the condition for a datum to belong to this level.
//!
//! # The row is a default, not a form to fill in
//!
//! Only four cells are required — the fund column and the three amounts — and everything else has
//! the value the engine itself defaults to. A statement laid out the common way is therefore four
//! anchors, and whoever writes a format that does not exist yet has something that probably already
//! works before having measured anything but the four labels.
//!
//! # The windows, which are where the formats really differ
//!
//! Each amount is read in a window hung off its own label: the label's box, moved by `(dx, dy)` of
//! its own size and scaled by `(width, height)`, one cell per amount. There is no cell setting the
//! three at once: it would be a second way of writing what the three already say, and where a row
//! wrote both one of them was silently ignored. ANIMA-EN23 and FINECO-EN23@LUX need a different
//! window for the total than for the other two, because their `Total assets` label sits on the same
//! line as its number while the others sit above theirs.
//!
//! # The casts are named, not written
//!
//! A structured format contains no code, so the two conversions a statement needs are chosen from a
//! vocabulary: see [`AmountCast`] and [`DateCast`]. An amount cast is a base — `int` or `float` —
//! and then the **conventions of presentation** the page uses, added with `+`: a dash standing for
//! zero, and a sign that is typography rather than data. They compose because the measured
//! statements combine them: FIDEURAM-EN23 writes a dash for zero *and* brackets for the sign, and
//! enumerating one name per combination would be a vocabulary that grows by multiplication.

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use crate::commons::date::Date;
use crate::core::pipeline::{Pipeline, PipelineName};
use crate::formats_utils::deserialize::cast::{self, CastError};
use crate::formats_utils::deserialize::standard_funcs::{DeserializerAssetsStandard, NumConverter};
use crate::formats_utils::pdf_extract::standard_funcs::{
    AssetsColumn, AssetsPairing, AssetsStandardArgs, PdfExtractAssetsStandard,
};
use crate::formats_utils::text_filter::standard_funcs::TextFilterAssetsStandard;
use crate::formats_utils::pdf_extract::select::relative::PdfLineSelection;
use crate::input::document::selection::pdfline_selection_from_str;

use super::StructuredError;
use super::tables::{FundAssetsConfig, ValueWindow, get_fund_assets_configs};

/// Which of the engine's two numeric casts an amount goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AmountBase {
    /// The engine's own default.
    #[default]
    Int,
    Float,
}

/// How a structured statement reads an amount: a base cast, plus the conventions of presentation
/// its page uses.
///
/// The base is `int` or `float`, which is the engine's own choice of cast and the only part that
/// concerns the **data**. Everything written after it with a `+` concerns the **page**:
///
/// - `dash_as_zero` — the report writes a dash where the amount is zero, which is a zero and not an
///   absence. ANIMA-EN23 and MEDIOLANUM-IT24.A do it, and the same idea has a name in the
///   investments pipe. An empty cell counts as the same thing, since a statement that leaves the
///   figure out where it is nil is making the same statement with less ink;
/// - `magnitude` — the sign the page prints is typography and not data (`R31`). A statement that
///   adds the liabilities to the net assets instead of subtracting them prints them negative
///   (CARNE-EN23), and one that marks a deduction with brackets writes `(1.234)`
///   (FIDEURAM-EN23); the output schema wants the magnitude in both cases, because it checks that
///   liabilities plus net assets make the total. The brackets come off before the cast, which is
///   the only way `to_int` can read the number at all.
///
/// They compose, and they are written rather than enumerated for that reason: three conventions
/// over two bases is a vocabulary of twelve names if each combination needs one, and FIDEURAM-EN23
/// already needs a combination of two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AmountCast {
    pub base: AmountBase,
    pub dash_as_zero: bool,
    pub magnitude: bool,
}

impl FromStr for AmountCast {
    type Err = String;

    /// `int`, `float + magnitude`, `int + dash_as_zero + magnitude`: the base first, then the
    /// conventions in any order.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let lowered = name.trim().to_ascii_lowercase();
        let mut terms = lowered.split('+').map(str::trim);
        let base = match terms.next() {
            Some("int") => AmountBase::Int,
            Some("float") => AmountBase::Float,
            other => {
                return Err(format!(
                    "'{}' is not an amount cast: it starts with int or float, and then any of \
                     dash_as_zero and magnitude, added with '+'",
                    other.unwrap_or_default()
                ));
            }
        };
        let mut cast = Self { base, ..Self::default() };
        for term in terms {
            match term {
                "dash_as_zero" => cast.dash_as_zero = true,
                "magnitude" => cast.magnitude = true,
                other => {
                    return Err(format!(
                        "'{other}' is not a convention an amount cast knows: they are dash_as_zero \
                         and magnitude"
                    ));
                }
            }
        }
        Ok(cast)
    }
}

/// The two dashes a report writes for a zero: the hyphen-minus and the en dash.
const ZERO_DASHES: [&str; 2] = ["-", "\u{2013}"];

impl AmountCast {
    /// The conversion this cell stands for, in the type the pipe takes.
    ///
    /// The conventions are applied to the **text**, in the order the page wrote it: the dash is
    /// recognised first, because a dash is not a number and no cast would read it, and the brackets
    /// come off next, because `to_int` would refuse them too. Only then does the base cast run, and
    /// the magnitude is taken of what it returns — which also covers the plain minus sign, that the
    /// cast does read.
    pub(crate) fn converter(self) -> NumConverter {
        Arc::new(move |text: &str| {
            let trimmed = text.trim();
            if self.dash_as_zero && (trimmed.is_empty() || ZERO_DASHES.contains(&trimmed)) {
                return Ok(0.0);
            }
            let cleaned = if self.magnitude { trimmed.trim_matches(['(', ')']) } else { trimmed };
            let value = match self.base {
                AmountBase::Int => cast::to_int(cleaned).map(|v| v as f64),
                AmountBase::Float => cast::to_float(cleaned),
            }?;
            Ok(if self.magnitude { value.abs() } else { value })
        })
    }
}

/// How a structured statement reads its date, when it reads one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateCast {
    /// Digits, in the order the engine's own `to_date` accepts. The default.
    #[default]
    Numeric,
    /// `31 December 2023`.
    EnMonth,
    /// `31 dicembre 2023`.
    ItMonth,
    /// `FEBRUARY 28, 2023`: the month first, which none of the engine's converters reads.
    EnMonthFirst,
    /// `31 de diciembre de 2024`, which is the only way MEDIOLANUM-ES24.A writes a date.
    EsMonth,
}

impl FromStr for DateCast {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name.trim().to_ascii_lowercase().as_str() {
            "numeric" => Ok(DateCast::Numeric),
            "en_month" => Ok(DateCast::EnMonth),
            "it_month" => Ok(DateCast::ItMonth),
            "en_month_first" => Ok(DateCast::EnMonthFirst),
            "es_month" => Ok(DateCast::EsMonth),
            other => Err(format!(
                "'{other}' is not a date cast: they are numeric, en_month, it_month, \
                 en_month_first, es_month"
            )),
        }
    }
}

impl DateCast {
    pub(crate) fn converter(self) -> fn(&str) -> Result<Date, CastError> {
        match self {
            DateCast::Numeric => cast::to_date,
            DateCast::EnMonth => cast::to_date_with_en_month,
            DateCast::ItMonth => cast::to_date_with_it_month,
            DateCast::EnMonthFirst => cast::to_date_with_en_month_first,
            DateCast::EsMonth => cast::to_date_with_es_month,
        }
    }
}

/// The pipeline a fund-assets row belongs to when its id names none.
pub const FUND_ASSETS_PIPELINE: &str = "fund_assets";

/// Builds one amount's window: the row's own cell if it has one, else the engine's default.
fn column(
    anchor: &str,
    window: Option<ValueWindow>,
    id: &str,
    name: &'static str,
) -> Result<AssetsColumn, StructuredError> {
    let selection = pdfline_selection_from_str(anchor)
        .map_err(|source| StructuredError::LineSelection { id: id.to_string(), column: name, source })?;
    let mut built = AssetsColumn::new(selection);
    if let Some(ValueWindow { dx, dy, width, height }) = window {
        built.vector = (dx, dy);
        built.width = width;
        built.height = height;
    }
    Ok(built)
}

/// An optional selection cell.
fn optional_selection(
    cell: &Option<String>,
    id: &str,
    name: &'static str,
) -> Result<Option<PdfLineSelection>, StructuredError> {
    match cell {
        None => Ok(None),
        Some(raw) => pdfline_selection_from_str(raw)
            .map(Some)
            .map_err(|source| StructuredError::LineSelection { id: id.to_string(), column: name, source }),
    }
}

/// The fund-assets pipelines the repository defines for `format_name`.
///
/// One pipeline per row, with all three segments: the row carries the parameters of each.
pub fn get_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<HashMap<PipelineName, Pipeline>, StructuredError> {
    let configs = get_fund_assets_configs(formats_repo_dir)?;
    let mut pipelines: HashMap<PipelineName, Pipeline> = HashMap::new();

    for config in configs.into_iter().filter(|c| c.id.format == format_name) {
        let FundAssetsConfig { id, row } = config;
        let key = id.to_string();

        let fund_set = pdfline_selection_from_str(&row.fund_set)
            .map_err(|source| StructuredError::LineSelection { id: key.clone(), column: "Fund set", source })?;
        let currency_set = optional_selection(&row.currency_set, &key, "Currency set")?;
        let date = match optional_selection(&row.date_set, &key, "Date set")? {
            Some(selection) => vec![selection],
            None => Vec::new(),
        };

        let args = AssetsStandardArgs {
            date,
            table_condition: row.table_condition.unwrap_or(false),
            // By abscissa, always: a number goes with the fund whose column it stands under. A
            // statement that can only be paired by position is the semistructured
            // `assets_by_position`.
            pairing: AssetsPairing::ByAbscissa,
            drop: optional_selection(&row.drop_set, &key, "Drop set")?,
            drop_funds: optional_selection(&row.drop_fund_set, &key, "Drop fund set")?,
            ..AssetsStandardArgs::new(
                fund_set,
                currency_set,
                column(&row.net_assets, row.net_assets_window, &key, "Net assets")?,
                column(&row.liabilities, row.liabilities_window, &key, "Total liabilities")?,
                column(&row.tot_assets, row.tot_assets_window, &key, "Total assets")?,
            )
        };

        // One cut per cell, as an alternation where there is more than one, exactly as the
        // investments table does it.
        let cuts: Vec<String> = row.remove_from_fund.clone().into_iter().collect();
        let filter = TextFilterAssetsStandard::new(row.date_regex.as_deref(), cuts)
            .map_err(|source| StructuredError::TextFilter { id: key.clone(), source })?;

        let deserializer = DeserializerAssetsStandard::with_converters(
            row.amount_cast.unwrap_or_default().converter(),
            Arc::new(row.date_cast.unwrap_or_default().converter()),
        );

        let name = PipelineName::new(&id.pipeline);
        let mut pipeline = Pipeline::new(name.clone());
        // `build` and not a constructor: the pipe refuses a statement that is neither tabular nor
        // carries a currency, because there would be nothing to derive one from — and a
        // misconfigured row must fail when the repository loads, not mid-run.
        let extractor = PdfExtractAssetsStandard::build(args)
            .map_err(|source| StructuredError::PdfExtract { id: key.clone(), source })?;
        pipeline.pdf_extract.push(Arc::new(extractor));
        pipeline.text_filter.push(Arc::new(filter));
        pipeline.deserialize.push(Arc::new(deserializer));
        pipelines.insert(name, pipeline);
    }

    tracing::debug!(pipeline_count = pipelines.len(), "built fund assets pipelines");
    Ok(pipelines)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod the_amount_casts {
        use super::*;
        use pretty_assertions::assert_eq;

        fn read(cast: &str, text: &str) -> Option<f64> {
            cast.parse::<AmountCast>().expect("a cast this test spells right").converter()(text).ok()
        }

        #[test]
        fn int_is_the_default_because_it_is_the_engines() {
            assert_eq!(AmountCast::default().base, AmountBase::Int);
            assert!(!AmountCast::default().dash_as_zero);
            assert!(!AmountCast::default().magnitude);
            assert_eq!(read("int", "1,234"), Some(1234.0));
        }

        #[test]
        fn float_keeps_the_decimals_the_page_carries() {
            assert_eq!(read("float", "1,234.56"), Some(1234.56));
        }

        #[test]
        fn a_dash_is_the_zero_the_page_means_and_both_dashes_are_in_use() {
            // ANIMA-EN23 writes a dash where the amount is zero: a zero, not an absence.
            assert_eq!(read("int + dash_as_zero", "-"), Some(0.0));
            assert_eq!(read("int + dash_as_zero", "\u{2013}"), Some(0.0));
            assert_eq!(read("int + dash_as_zero", " - "), Some(0.0));
            // A cell left empty says the same thing with less ink.
            assert_eq!(read("int + dash_as_zero", "  "), Some(0.0));
            // And it still reads a number where there is one.
            assert_eq!(read("int + dash_as_zero", "1,234"), Some(1234.0));
        }

        #[test]
        fn a_dash_is_not_a_zero_unless_the_cell_says_so() {
            // The whole point of naming the convention: the page that means zero says so in the
            // cell, and a page that does not is left to fail on a value it cannot read.
            assert_eq!(read("int", "-"), None);
            assert_eq!(read("float", "-"), None);
        }

        #[test]
        fn the_magnitude_convention_drops_a_sign_the_page_prints_for_presentation() {
            // CARNE-EN23 prints the liabilities negative, because its statement adds them to the
            // net assets instead of subtracting them. The output schema wants the magnitude.
            assert_eq!(read("float + magnitude", "-365,138.81"), Some(365138.81));
            assert_eq!(read("float + magnitude", "365,138.81"), Some(365138.81));
        }

        #[test]
        fn the_magnitude_convention_also_takes_the_brackets_off() {
            // FIDEURAM-EN23 marks a deduction with brackets, which `to_int` cannot read at all, so
            // they come off before the cast rather than after it.
            assert_eq!(read("int + magnitude", "(1.234)"), Some(1234.0));
            assert_eq!(read("int + magnitude", "1.234"), Some(1234.0));
        }

        #[test]
        fn the_conventions_compose_which_is_why_they_are_written_and_not_enumerated() {
            // FIDEURAM-EN23 needs both: a dash for zero and brackets for the sign.
            for cast in ["int + dash_as_zero + magnitude", "int + magnitude + dash_as_zero"] {
                assert_eq!(read(cast, "-"), Some(0.0), "{cast}");
                assert_eq!(read(cast, "(1.234)"), Some(1234.0), "{cast}");
                assert_eq!(read(cast, "1.234"), Some(1234.0), "{cast}");
            }
        }

        #[test]
        fn every_name_is_read_and_a_typo_lists_the_vocabulary() {
            let cast = |base, dash_as_zero, magnitude| AmountCast { base, dash_as_zero, magnitude };
            assert_eq!("int".parse(), Ok(cast(AmountBase::Int, false, false)));
            assert_eq!("float".parse(), Ok(cast(AmountBase::Float, false, false)));
            assert_eq!("int + dash_as_zero".parse(), Ok(cast(AmountBase::Int, true, false)));
            assert_eq!("float + magnitude".parse(), Ok(cast(AmountBase::Float, false, true)));
            // Case and surrounding spaces do not matter: a spreadsheet adds them.
            assert_eq!("  FLOAT ".parse(), Ok(cast(AmountBase::Float, false, false)));
            assert_eq!("int+magnitude".parse(), Ok(cast(AmountBase::Int, false, true)));

            let base = "flaot".parse::<AmountCast>().unwrap_err();
            for name in ["int", "float", "dash_as_zero", "magnitude"] {
                assert!(base.contains(name), "{base}");
            }
            let convention = "int + absolute".parse::<AmountCast>().unwrap_err();
            assert!(convention.contains("absolute"), "{convention}");
            assert!(convention.contains("magnitude"), "{convention}");
        }
    }

    mod the_date_casts {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn numeric_is_the_default_because_it_is_the_engines() {
            assert_eq!(DateCast::default(), DateCast::Numeric);
        }

        #[test]
        fn each_one_reads_its_own_spelling() {
            assert!(DateCast::Numeric.converter()("31/12/2023").is_ok());
            assert!(DateCast::EnMonth.converter()("31 December 2023").is_ok());
            assert!(DateCast::ItMonth.converter()("31 dicembre 2023").is_ok());
            assert!(DateCast::EnMonthFirst.converter()("FEBRUARY 28, 2023").is_ok());
        }

        #[test]
        fn a_spelling_the_named_cast_does_not_know_is_refused_rather_than_guessed() {
            assert!(DateCast::EnMonth.converter()("31 dicembre 2023").is_err());
            // The two English casts read opposite orders, and neither guesses the other's.
            assert!(DateCast::EnMonth.converter()("FEBRUARY 28, 2023").is_err());
            assert!(DateCast::EnMonthFirst.converter()("31 December 2023").is_err());
        }

        #[test]
        fn the_month_first_cast_reads_the_date_fideuram_heads_its_statement_with() {
            let date = DateCast::EnMonthFirst.converter()("FEBRUARY 28, 2023").unwrap();
            assert_eq!(date, Date::new(2023, 2, 28).unwrap());
            // The comma is a separator, not a requirement: a date is being recognised, not
            // validated.
            assert_eq!(DateCast::EnMonthFirst.converter()("february 28 2023").unwrap(), date);
            // A month that is not one is refused rather than taken for a day.
            assert!(DateCast::EnMonthFirst.converter()("FEBBRAIO 28, 2023").is_err());
            assert!(DateCast::EnMonthFirst.converter()("FEBRUARY 2023").is_err());
        }

        #[test]
        fn every_name_is_read_and_a_typo_lists_the_vocabulary() {
            assert_eq!("numeric".parse(), Ok(DateCast::Numeric));
            assert_eq!("en_month".parse(), Ok(DateCast::EnMonth));
            assert_eq!("it_month".parse(), Ok(DateCast::ItMonth));
            assert_eq!("en_month_first".parse(), Ok(DateCast::EnMonthFirst));
            let message = "english".parse::<DateCast>().unwrap_err();
            for name in ["numeric", "en_month", "it_month", "en_month_first"] {
                assert!(message.contains(name), "{message}");
            }
        }
    }

    mod the_window {
        use super::*;
        use pretty_assertions::assert_eq;

        const OWN: ValueWindow = ValueWindow { dx: 1.0, dy: 2.0, width: 3.0, height: 4.0 };

        fn built(own: Option<ValueWindow>) -> AssetsColumn {
            column("ArialMT", own, "A-EN24", "Total assets").expect("a valid anchor")
        }

        #[test]
        fn without_a_cell_the_engines_own_default_stands() {
            let column = built(None);
            let engine_default = AssetsColumn::new(pdfline_selection_from_str("ArialMT").unwrap());
            assert_eq!(column.vector, engine_default.vector);
            assert_eq!((column.width, column.height), (engine_default.width, engine_default.height));
        }

        #[test]
        fn the_columns_own_cell_is_the_window() {
            let column = built(Some(OWN));
            assert_eq!(column.vector, (1.0, 2.0));
            assert_eq!((column.width, column.height), (3.0, 4.0));
        }

        #[test]
        fn an_anchor_the_parser_refuses_names_the_column_it_came_from() {
            // `Arial[0]` and not `???`: the parser never **rejects** — every group of the grammar
            // is optional, so a nonsense cell yields an empty selection and the table's own
            // `is_pdfline_selection` is what catches it. What the parser does refuse is a value it
            // cannot honour, such as a font size of zero, and that is the path this error serves.
            //
            // `expect_err` would want `Debug` on `AssetsColumn`, which has none, so the error is
            // taken with an explicit match.
            let error = match column("Arial[0]", None, "A-EN24", "Net assets") {
                Ok(_) => panic!("expected a refusal, got a column"),
                Err(error) => error,
            };
            assert!(error.to_string().contains("Net assets"), "{error}");
        }
    }
}
