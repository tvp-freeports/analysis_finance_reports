//! `assets_by_position` and the `assets` filter: a statement of net assets whose amounts are matched
//! to their funds **by position** rather than by abscissa.
//!
//! The structured table `fund_assets/args.csv` pairs by abscissa, always: a number goes with the
//! fund whose column it stands under, which is the family's rule and which holds on ten of the
//! eleven measured statements — on FINECO-EN23@LUX p31 pairing by position had swapped two funds'
//! figures. The eleventh, MEDIOLANUM-EN24, cannot be read that way: it prints each **total** row as
//! one text object carrying this year's and last year's figures for both funds, so there is no
//! abscissa per value to pair by. Such a page is read by splitting the object on its runs of spaces
//! and taking the values in order, one in every `value_step`.
//!
//! The two are two algorithms rather than one with a switch, because the knobs of the second mean
//! nothing to the first: a table holding both had `Value step` and `Values in one line` silently
//! ignored whenever `Pair by abscissa` was on. Here every field has an effect.

use serde::Deserialize;

use crate::formats_utils::pdf_extract::standard_funcs::{
    AssetsColumn, AssetsPairing, AssetsStandardArgs, PdfExtractAssetsStandard,
    PdfExtractStandardFuncsError,
};
use crate::formats_utils::text_filter::standard_funcs::{
    StandardFuncsError, TextFilterAssetsStandard,
};

use super::selection::{InputSelection, SelectionConfigError};

/// What an `assets_by_position` configuration can be refused for.
#[derive(Debug, thiserror::Error)]
pub enum AssetsByPositionError {
    #[error(transparent)]
    Selection(#[from] SelectionConfigError),
    #[error(transparent)]
    Build(#[from] PdfExtractStandardFuncsError),
    #[error("'value_step' must be at least 1, found {0}")]
    Step(i64),
}

/// The window an amount is read in: its anchor's box, moved by `(dx, dy)` of its own size and
/// scaled by `(width, height)`. Absent means the engine's default.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputWindow {
    pub dx: f32,
    pub dy: f32,
    pub width: f32,
    pub height: f32,
}

/// One of the three amounts: the line its window hangs from, and the window.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputAmount {
    pub anchor: InputSelection,
    #[serde(default)]
    pub window: Option<InputWindow>,
}

impl InputAmount {
    fn build(&self, field: &'static str) -> Result<AssetsColumn, SelectionConfigError> {
        let mut column = AssetsColumn::new(self.anchor.build(field)?);
        if let Some(InputWindow {
            dx,
            dy,
            width,
            height,
        }) = self.window
        {
            column.vector = (dx, dy);
            column.width = width;
            column.height = height;
        }
        Ok(column)
    }
}

fn one() -> i64 {
    1
}

/// The YAML configuration of `assets_by_position`. The page is a table, one fund per column.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputAssetsByPosition {
    /// The lines naming the fund columns.
    pub fund_set: InputSelection,
    /// The line declaring the currency of every column. Absent: the last word of each fund's name.
    #[serde(default)]
    pub currency_set: Option<InputSelection>,
    /// The line carrying the statement's date. Absent: no date.
    #[serde(default)]
    pub date_set: Option<InputSelection>,
    pub tot_assets: InputAmount,
    pub liabilities: InputAmount,
    pub net_assets: InputAmount,
    /// The heading of the number columns to leave out — whole lines standing under it.
    #[serde(default)]
    pub drop_set: Option<InputSelection>,
    /// A fund column that is not a fund, by one of its name lines: it goes whole.
    #[serde(default)]
    pub drop_fund_set: Option<InputSelection>,
    /// One value in every N, in order: `2` where each fund prints last year's figure beside this
    /// year's.
    #[serde(default = "one")]
    pub value_step: i64,
    /// Whether one text object may carry several values, told apart by runs of spaces.
    #[serde(default)]
    pub values_in_one_line: bool,
}

impl InputAssetsByPosition {
    pub fn build(&self) -> Result<PdfExtractAssetsStandard, AssetsByPositionError> {
        if self.value_step < 1 {
            return Err(AssetsByPositionError::Step(self.value_step));
        }
        let optional = |cell: &Option<InputSelection>, field| {
            cell.as_ref().map(|s| s.build(field)).transpose()
        };
        let args = AssetsStandardArgs {
            date: optional(&self.date_set, "date_set")?.into_iter().collect(),
            table_condition: true,
            pairing: AssetsPairing::ByIndex,
            drop: optional(&self.drop_set, "drop_set")?,
            drop_funds: optional(&self.drop_fund_set, "drop_fund_set")?,
            skip_column: self.value_step,
            values_in_one_line: self.values_in_one_line,
            ..AssetsStandardArgs::new(
                self.fund_set.build("fund_set")?,
                optional(&self.currency_set, "currency_set")?,
                self.net_assets.build("net_assets")?,
                self.liabilities.build("liabilities")?,
                self.tot_assets.build("tot_assets")?,
            )
        };
        Ok(PdfExtractAssetsStandard::build(args)?)
    }
}

/// The YAML configuration of the `assets` filter: the date, and what to cut out of a fund's name.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputAssetsFilter {
    /// The expression pulling the date out of its line, with exactly one capturing group.
    #[serde(default)]
    pub date_regex: Option<String>,
    /// One expression cut out of each fund's name — a footnote mark among them, `\s*[*]*\s*$`.
    #[serde(default)]
    pub remove_from_fund: Option<String>,
}

impl InputAssetsFilter {
    pub fn build(&self) -> Result<TextFilterAssetsStandard, StandardFuncsError> {
        TextFilterAssetsStandard::new(
            self.date_regex.as_deref(),
            self.remove_from_fund.clone().into_iter().collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(yaml: &str) -> InputAssetsByPosition {
        serde_yaml::from_str(yaml).expect("the test's own yaml")
    }

    const MINIMAL: &str = "
fund_set: {text: '(expressed in'}
tot_assets: {anchor: {text: TOTAL ASSETS}}
liabilities: {anchor: {text: Liabilities}}
net_assets: {anchor: {text: NET ASSETS}}
";

    #[test]
    fn four_selections_are_a_whole_configuration() {
        let parsed = input(MINIMAL);
        assert_eq!(parsed.value_step, 1);
        assert!(!parsed.values_in_one_line);
        assert!(parsed.build().is_ok());
    }

    #[test]
    fn a_step_below_one_is_refused() {
        let parsed = input(&format!("{MINIMAL}value_step: 0\n"));
        assert!(matches!(
            parsed.build(),
            Err(AssetsByPositionError::Step(0))
        ));
    }

    #[test]
    fn a_key_it_does_not_know_is_refused() {
        // The structured table's switch has no meaning here: pairing is by position by definition.
        let error = serde_yaml::from_str::<InputAssetsByPosition>(&format!(
            "{MINIMAL}pair_by_abscissa: true\n"
        ))
        .unwrap_err();
        assert!(error.to_string().contains("pair_by_abscissa"), "{error}");
    }

    #[test]
    fn a_window_takes_its_four_numbers() {
        let parsed = input(
            "
fund_set: {text: Fund}
tot_assets: {anchor: {text: TOTAL ASSETS}, window: {dx: 1.2, dy: -0.1, width: 100, height: 1.2}}
liabilities: {anchor: {text: Liabilities}}
net_assets: {anchor: {text: NET ASSETS}}
",
        );
        let window = parsed.tot_assets.window.expect("the window");
        assert_eq!(
            (window.dx, window.dy, window.width, window.height),
            (1.2, -0.1, 100.0, 1.2)
        );
    }

    #[test]
    fn the_filter_refuses_a_regex_that_does_not_compile() {
        let filter = InputAssetsFilter {
            date_regex: None,
            remove_from_fund: Some("(unclosed".to_string()),
        };
        assert!(filter.build().is_err());
    }
}
