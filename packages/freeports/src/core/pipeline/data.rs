//! The vocabulary the pipes share: what goes into a pipe ([`FilterData`]), what comes out
//! ([`Extracted`]), how it fails ([`PipeError`]).
//!
//! It lives in a module of its own rather than next to
//! [`Pipeline`](crate::core::pipeline::Pipeline) because all five pieces of the engine use it — the
//! three pipe traits, `Pipeline`, `PipelinesBundle` and `Algorithm`. `core::pipeline` re-exports
//! it, so the public path stays `core::pipeline::{FilterData, Extracted, PipeError}`.
//!
//! # Why [`FilterData`] is an enum
//!
//! At the first step of a schedule a pipe sees **only** the target companies; from the following
//! steps it sees **only** the accumulated results of every preceding step. The two are never
//! available together, and an enum says so, where a two-field struct would suggest they might be.
//!
//! The accepted consequence: a pipe that needs the target companies works only when scheduled in
//! the first step.

use crate::core::classes::value::{BlockValue, BlockValueError};
use crate::core::page::PageError;
use crate::core::promise_resolution::PromiseMap;
use crate::core::schedule::PageClass;
use crate::output::classes::assets_manager::{InvestmentsManager, ManagementCompany};
use crate::output::classes::fund::Fund;
use crate::output::classes::fund_assets::FundAssets;
use crate::output::classes::fund_change_name::{FundMerge, FundRename};
use crate::output::classes::fund_esg_indicator::FundEsgIndicator;
use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
use crate::output::classes::investment::{Bond, Equity};
use crate::formats_utils::pdf_extract::commons::CommonsError;
use crate::formats_utils::text_filter::matcher::CompanyMatchInfos;

/// The failure of a single pipe.
///
/// Every variant names the pipe that produced it. That is also why the three pipe traits carry a
/// `name()`: an anonymous failure in the middle of a chain of pipes is close to undiagnosable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PipeError {
    /// The page cannot be interpreted. **Non-fatal**:
    /// [`Algorithm`](crate::core::algorithm::Algorithm) logs it and skips the page.
    #[error("pipe `{pipe}` could not parse the page: {source}")]
    PageParse {
        pipe: String,
        #[source]
        source: PageError,
    },
    /// The pipe did not find what it expected to find.
    #[error("pipe `{pipe}` failed to extract: {message}")]
    Extraction { pipe: String, message: String },
    /// A field conversion failed.
    #[error("pipe `{pipe}` could not cast field `{field}`: {message}")]
    Cast { pipe: String, field: String, message: String },
    /// A pipe **written by the format author**, in Python, raised.
    ///
    /// This is the boundary: no `PyErr` travels beyond `formats_repo`, it becomes this variant.
    #[error("author pipe `{pipe}` of pipeline `{pipeline}` failed: {message}")]
    Author { pipeline: String, pipe: String, message: String },
    /// A `metadata` or `content` field did not have the expected type.
    #[error("pipe `{pipe}`: {source}")]
    Value {
        pipe: String,
        #[source]
        source: BlockValueError,
    },
}

impl PipeError {
    pub fn page_parse(pipe: impl Into<String>, source: PageError) -> Self {
        PipeError::PageParse { pipe: pipe.into(), source }
    }

    pub fn extraction(pipe: impl Into<String>, message: impl Into<String>) -> Self {
        PipeError::Extraction { pipe: pipe.into(), message: message.into() }
    }

    pub fn cast(
        pipe: impl Into<String>,
        field: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        PipeError::Cast { pipe: pipe.into(), field: field.into(), message: message.into() }
    }

    pub fn author(
        pipeline: impl Into<String>,
        pipe: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        PipeError::Author {
            pipeline: pipeline.into(),
            pipe: pipe.into(),
            message: message.into(),
        }
    }

    pub fn value(pipe: impl Into<String>, source: BlockValueError) -> Self {
        PipeError::Value { pipe: pipe.into(), source }
    }

    /// True only for [`PipeError::PageParse`]: the one failure the algorithm absorbs by skipping
    /// the page instead of stopping.
    pub fn is_page_failure(&self) -> bool {
        matches!(self, PipeError::PageParse { .. })
    }

    /// The name of the pipe that produced the error.
    pub fn pipe(&self) -> &str {
        match self {
            PipeError::PageParse { pipe, .. }
            | PipeError::Extraction { pipe, .. }
            | PipeError::Cast { pipe, .. }
            | PipeError::Author { pipe, .. }
            | PipeError::Value { pipe, .. } => pipe,
        }
    }

    /// Converts the local error of `pdf_extract::commons` into the engine's own.
    ///
    /// Not an `impl From` because the pipe's name cannot be recovered from the error, and inventing
    /// an empty string would make the messages worse than the ones it replaces.
    ///
    /// [`CommonsError::PageParseFail`] becomes the **non-fatal** page failure;
    /// [`CommonsError::ExpectedTextNotFound`] becomes [`PipeError::Extraction`].
    pub fn from_commons(pipe: impl Into<String>, error: CommonsError) -> Self {
        let pipe = pipe.into();
        match error {
            CommonsError::PageParseFail { ref source } => {
                PipeError::PageParse { source: PageError::ParseFail { message: source.to_string() }, pipe }
            }
            CommonsError::ExpectedTextNotFound { .. } => {
                PipeError::Extraction { message: error.to_string(), pipe }
            }
        }
    }
}

/// The promises a deserialize pipe deposits: `id → contribution` pairs, in the order the pipe
/// produced them.
///
/// The order matters: the later one wins when the promise is not *multiple* (see
/// [`FlatPromiseMap::fulfill`](crate::core::promise_resolution::FlatPromiseMap::fulfill)).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct PromiseEntries(Vec<(String, BlockValue)>);

impl PromiseEntries {
    pub fn new() -> Self {
        PromiseEntries::default()
    }

    pub fn push(&mut self, id: impl Into<String>, value: impl Into<BlockValue>) {
        self.0.push((id.into(), value.into()));
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &BlockValue)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Pours the contributions into the multimap, in order.
    pub fn merge_into(&self, map: &mut PromiseMap) {
        // The ids, not the count: they are the key by which a promise is found again in the
        // register of unresolved ones.
        tracing::debug!(
            ids = %self.0.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(", "),
            "promises deposited"
        );
        map.merge(self.0.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
}

impl<K: Into<String>, V: Into<BlockValue>> FromIterator<(K, V)> for PromiseEntries {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut entries = PromiseEntries::new();
        for (k, v) in iter {
            entries.push(k, v);
        }
        entries
    }
}

/// The result of a deserialize pipe.
///
/// An enum rather than a heterogeneous list means that regrouping results by type is a `match` the
/// compiler checks, instead of a chain of runtime type tests that goes quietly wrong when a
/// thirteenth kind of result appears.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "v", rename_all = "snake_case")]
pub enum Extracted {
    /// An equity holding in a target company.
    Equity(Equity),
    /// A bond issued by a target company.
    Bond(Bond),
    /// A fund, by name alone.
    Fund(Fund),
    /// A fund's assets at a given date.
    FundAssets(FundAssets),
    /// A fund's declared SFDR classification (article 6, 8 or 9).
    FundSfdrClassification(FundSfdrClassification),
    /// An ESG indicator of a fund.
    FundEsgIndicator(FundEsgIndicator),
    /// The renaming of a fund.
    FundRename(FundRename),
    /// The merger of one fund into another.
    FundMerge(FundMerge),
    /// A fund's management company.
    ManagementCompany(ManagementCompany),
    /// A fund's investments manager.
    InvestmentsManager(InvestmentsManager),
    /// The promises the pipe deposited, to be poured into the resolution multimap.
    Promises(PromiseEntries),
    /// The outcome of the classification pipeline: the page's class, or `None` if the pipe could
    /// not classify it.
    PageClass(Option<PageClass>),
}

impl Extracted {
    /// The page class, if this result comes from the classification pipeline.
    ///
    /// `Some(None)` and `None` are different things: the first is "a classification happened, and
    /// it is 'no class'", the second is "this result is not a classification".
    #[allow(clippy::option_option)]
    pub fn as_page_class(&self) -> Option<&Option<PageClass>> {
        match self {
            Extracted::PageClass(class) => Some(class),
            _ => None,
        }
    }

    /// The promises, if this result carries any.
    pub fn as_promises(&self) -> Option<&PromiseEntries> {
        match self {
            Extracted::Promises(entries) => Some(entries),
            _ => None,
        }
    }

    /// The fund, if this result is one.
    pub fn as_fund(&self) -> Option<&Fund> {
        match self {
            Extracted::Fund(fund) => Some(fund),
            _ => None,
        }
    }

    /// The equity holding, if this result is one.
    pub fn as_equity(&self) -> Option<&Equity> {
        match self {
            Extracted::Equity(equity) => Some(equity),
            _ => None,
        }
    }

    /// The bond, if this result is one.
    pub fn as_bond(&self) -> Option<&Bond> {
        match self {
            Extracted::Bond(bond) => Some(bond),
            _ => None,
        }
    }

    /// The fund assets, if this result is one.
    pub fn as_fund_assets(&self) -> Option<&FundAssets> {
        match self {
            Extracted::FundAssets(assets) => Some(assets),
            _ => None,
        }
    }

    /// The SFDR classification, if this result is one.
    pub fn as_fund_sfdr_classification(&self) -> Option<&FundSfdrClassification> {
        match self {
            Extracted::FundSfdrClassification(classification) => Some(classification),
            _ => None,
        }
    }

    /// The ESG indicator, if this result is one.
    pub fn as_fund_esg_indicator(&self) -> Option<&FundEsgIndicator> {
        match self {
            Extracted::FundEsgIndicator(indicator) => Some(indicator),
            _ => None,
        }
    }

    /// The renaming, if this result is one.
    pub fn as_fund_rename(&self) -> Option<&FundRename> {
        match self {
            Extracted::FundRename(rename) => Some(rename),
            _ => None,
        }
    }

    /// The merger, if this result is one.
    pub fn as_fund_merge(&self) -> Option<&FundMerge> {
        match self {
            Extracted::FundMerge(merge) => Some(merge),
            _ => None,
        }
    }

    /// The management company, if this result is one.
    pub fn as_management_company(&self) -> Option<&ManagementCompany> {
        match self {
            Extracted::ManagementCompany(company) => Some(company),
            _ => None,
        }
    }

    /// The investments manager, if this result is one.
    pub fn as_investments_manager(&self) -> Option<&InvestmentsManager> {
        match self {
            Extracted::InvestmentsManager(manager) => Some(manager),
            _ => None,
        }
    }
}

/// What a `text_filter` pipe knows about the context it runs in.
///
/// An enum, not a struct: the two are never available together — see the module documentation.
#[derive(Debug, Clone, Copy)]
pub enum FilterData<'a> {
    /// First step of the schedule: the target companies the pipe has to match against.
    TargetCompanies(&'a [CompanyMatchInfos]),
    /// Later steps: the accumulated results of **all** preceding steps.
    Previous(&'a [Extracted]),
}

impl<'a> FilterData<'a> {
    /// The target companies if this is the first step; an empty slice otherwise.
    pub fn target_companies(&self) -> &'a [CompanyMatchInfos] {
        match self {
            FilterData::TargetCompanies(companies) => companies,
            FilterData::Previous(_) => &[],
        }
    }

    /// The results of the preceding steps if this is not the first one; an empty slice otherwise.
    pub fn previous(&self) -> &'a [Extracted] {
        match self {
            FilterData::Previous(results) => results,
            FilterData::TargetCompanies(_) => &[],
        }
    }

    /// The [`FilterData`] the classification pipelines run with, where there is neither a preceding
    /// step nor a list of target companies.
    pub const EMPTY: FilterData<'static> = FilterData::Previous(&[]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats_utils::text_filter::matcher::TargetCompanyInput;

    fn companies() -> Vec<CompanyMatchInfos> {
        CompanyMatchInfos::compile_from_target_companies(vec![TargetCompanyInput {
            name: "Acme".to_string(),
            regexs: vec![],
            symbols: vec![],
            buds: vec![],
        }])
        .expect("fixed, valid input")
    }

    mod pipe_error_classification {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn only_a_page_parse_failure_is_absorbed_by_the_algorithm() {
            let page_fail = PipeError::page_parse(
                "p",
                PageError::ParseFail { message: "no table".to_string() },
            );
            assert!(page_fail.is_page_failure());

            for other in [
                PipeError::extraction("p", "m"),
                PipeError::cast("p", "f", "m"),
                PipeError::author("pl", "p", "m"),
                PipeError::value("p", BlockValueError::MissingField { field: "f".to_string() }),
            ] {
                assert!(!other.is_page_failure(), "{other:?} must not be absorbed");
            }
        }

        #[test]
        fn every_variant_names_the_pipe_that_produced_it() {
            let errors = [
                PipeError::page_parse("a", PageError::ParseFail { message: String::new() }),
                PipeError::extraction("a", "m"),
                PipeError::cast("a", "f", "m"),
                PipeError::author("pl", "a", "m"),
                PipeError::value("a", BlockValueError::MissingField { field: "f".to_string() }),
            ];
            for err in errors {
                assert_eq!(err.pipe(), "a");
            }
        }
    }

    mod pipe_error_messages {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_page_parse_failure_quotes_the_underlying_message() {
            let err =
                PipeError::page_parse("extract", PageError::ParseFail { message: "no table".into() });
            assert_eq!(err.to_string(), "pipe `extract` could not parse the page: no table");
        }

        #[test]
        fn an_extraction_failure_names_pipe_and_reason() {
            assert_eq!(
                PipeError::extraction("extract", "fund not found").to_string(),
                "pipe `extract` failed to extract: fund not found"
            );
        }

        #[test]
        fn a_cast_failure_names_the_field() {
            assert_eq!(
                PipeError::cast("deser", "market_value", "not a number").to_string(),
                "pipe `deser` could not cast field `market_value`: not a number"
            );
        }

        #[test]
        fn an_author_failure_names_both_pipeline_and_pipe() {
            assert_eq!(
                PipeError::author("investments", "custom_extract", "KeyError: 'x'").to_string(),
                "author pipe `custom_extract` of pipeline `investments` failed: KeyError: 'x'"
            );
        }

        #[test]
        fn a_value_failure_forwards_the_block_value_message() {
            let err =
                PipeError::value("deser", BlockValueError::MissingField { field: "fund".into() });
            assert_eq!(err.to_string(), "pipe `deser`: missing field 'fund'");
        }
    }

    mod pipe_error_from_commons {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_commons_page_failure_becomes_a_non_fatal_page_parse() {
            let commons = CommonsError::PageParseFail {
                source: Box::new(CommonsError::ExpectedTextNotFound { name: "fund".to_string() }),
            };
            let err = PipeError::from_commons("extract", commons);
            assert!(err.is_page_failure());
            assert_eq!(
                err,
                PipeError::page_parse(
                    "extract",
                    PageError::ParseFail {
                        message: "Pdf block during extraction of \"fund\" not found".to_string()
                    }
                )
            );
        }

        #[test]
        fn a_bare_not_found_becomes_a_fatal_extraction_failure() {
            let commons = CommonsError::ExpectedTextNotFound { name: "fund".to_string() };
            let err = PipeError::from_commons("extract", commons);
            assert!(!err.is_page_failure());
            assert_eq!(
                err,
                PipeError::extraction("extract", "Pdf block during extraction of \"fund\" not found")
            );
        }
    }

    mod promise_entries {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn keeps_the_order_in_which_the_pipe_produced_them() {
            let entries: PromiseEntries =
                [("id", BlockValue::from("first")), ("id", BlockValue::from("second"))]
                    .into_iter()
                    .collect();
            let values: Vec<&BlockValue> = entries.iter().map(|(_, v)| v).collect();
            assert_eq!(values, vec![&BlockValue::from("first"), &BlockValue::from("second")]);
        }

        #[test]
        fn merging_appends_every_contribution_under_its_id() {
            let entries: PromiseEntries =
                [("a", BlockValue::from(1i64)), ("b", BlockValue::from(2i64)), ("a", BlockValue::from(3i64))]
                    .into_iter()
                    .collect();
            let mut map = PromiseMap::new();
            entries.merge_into(&mut map);

            assert_eq!(map.get("a"), Some([BlockValue::from(1i64), BlockValue::from(3i64)].as_slice()));
            assert_eq!(map.get("b"), Some([BlockValue::from(2i64)].as_slice()));
        }

        #[test]
        fn merging_twice_accumulates_rather_than_replacing() {
            let entries: PromiseEntries = [("a", BlockValue::from(1i64))].into_iter().collect();
            let mut map = PromiseMap::new();
            entries.merge_into(&mut map);
            entries.merge_into(&mut map);
            assert_eq!(map.get("a").map(<[BlockValue]>::len), Some(2));
        }

        #[test]
        fn an_empty_set_of_entries_leaves_the_map_untouched() {
            let mut map = PromiseMap::new();
            PromiseEntries::new().merge_into(&mut map);
            assert!(map.is_empty());
            assert!(PromiseEntries::new().is_empty());
            assert_eq!(PromiseEntries::new().len(), 0);
        }
    }

    mod extracted_accessors {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_page_class_result_exposes_its_class() {
            let e = Extracted::PageClass(Some(PageClass::new("investments")));
            assert_eq!(e.as_page_class(), Some(&Some(PageClass::new("investments"))));
        }

        #[test]
        fn an_unclassified_page_is_still_a_page_class_result() {
            // `Some(None)` — a classification saying "no class" — is not to be confused with
            // `None`, which says this result is not a classification at all.
            let e = Extracted::PageClass(None);
            assert_eq!(e.as_page_class(), Some(&None));
            assert!(e.as_promises().is_none());
        }

        #[test]
        fn a_promises_result_is_not_a_page_class() {
            let e = Extracted::Promises(PromiseEntries::new());
            assert!(e.as_page_class().is_none());
            assert!(e.as_promises().is_some());
        }
    }

    mod filter_data_semantics {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_first_step_sees_target_companies_and_no_previous_results() {
            let companies = companies();
            let data = FilterData::TargetCompanies(&companies);
            assert_eq!(data.target_companies().len(), 1);
            assert!(data.previous().is_empty());
        }

        #[test]
        fn a_later_step_sees_previous_results_and_no_target_companies() {
            let previous = vec![Extracted::PageClass(None)];
            let data = FilterData::Previous(&previous);
            assert_eq!(data.previous().len(), 1);
            assert!(data.target_companies().is_empty());
        }

        #[test]
        fn the_empty_filter_data_used_by_page_classification_sees_neither() {
            let data = FilterData::EMPTY;
            assert!(data.target_companies().is_empty());
            assert!(data.previous().is_empty());
        }
    }

    /// The entity variants that `output::classes` contributes, with their `as_*` accessors and an
    /// exhaustive `match` that prevents a silent regression.
    mod new_entity_variants {
        use super::*;
        use crate::commons::consts::{Currency, SfdrArticle};
        use crate::commons::date::Date;
        use crate::output::classes::assets_manager::{InvestmentsManager, ManagementCompany};
        use crate::output::classes::fund_assets::FundAssets;
        use crate::output::classes::fund_change_name::{FundMerge, FundRename};
        use crate::output::classes::fund_esg_indicator::FundEsgIndicator;
        use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
        use crate::core::promise::Promise;
        use crate::output::classes::investment::InvestmentFields;
        use std::collections::BTreeSet;

        fn equity() -> Equity {
            Equity::build(InvestmentFields::new(
                "Acme Corp",
                "Acme",
                BlockValue::from("Alpha Fund"),
                BlockValue::from(1000.0),
                BlockValue::from(Currency::EUR),
            ))
            .expect("fixed, valid fixture")
        }

        fn bond() -> Bond {
            Bond::build(
                InvestmentFields::new(
                    "Acme Corp",
                    "Acme",
                    BlockValue::from("Alpha Fund"),
                    BlockValue::from(1000.0),
                    BlockValue::from(Currency::EUR),
                ),
                None,
                None,
            )
            .expect("fixed, valid fixture")
        }

        fn fund_assets() -> FundAssets {
            FundAssets::build("Alpha Fund", 100.0, 40.0, 60.0, &BlockValue::from(Currency::EUR), None)
                .expect("fixed, valid fixture")
        }

        fn fund_sfdr_classification() -> FundSfdrClassification {
            FundSfdrClassification::build("Alpha Fund", &BlockValue::from(SfdrArticle::Art8))
                .expect("fixed, valid fixture")
        }

        fn fund_esg_indicator() -> FundEsgIndicator {
            FundEsgIndicator::build(&BlockValue::from("Alpha Fund"), "GHG intensity", "12.3")
                .expect("fixed, valid fixture")
        }

        fn fund_rename() -> FundRename {
            FundRename::build("Old Fund", "New Fund", &BlockValue::from(Date::new(2025, 1, 1).unwrap()))
                .expect("fixed, valid fixture")
        }

        fn fund_merge() -> FundMerge {
            FundMerge::build("Old Fund", "New Fund", &BlockValue::from(Date::new(2025, 1, 1).unwrap()))
                .expect("fixed, valid fixture")
        }

        fn management_company() -> ManagementCompany {
            ManagementCompany::build(&BlockValue::from("Acme AM"), &BlockValue::Set(BTreeSet::new()))
                .expect("fixed, valid fixture")
        }

        fn investments_manager() -> InvestmentsManager {
            InvestmentsManager::build(&BlockValue::from("Acme IM"), &BlockValue::Set(BTreeSet::new()))
                .expect("fixed, valid fixture")
        }

        /// One specimen of each of the twelve [`Extracted`] variants, in declaration order. The
        /// exhaustiveness checks below iterate over this list, the same way `BlockValue`'s tests
        /// do.
        fn one_of_each() -> Vec<Extracted> {
            vec![
                Extracted::Equity(equity()),
                Extracted::Bond(bond()),
                Extracted::Fund(Fund::new("Alpha Fund")),
                Extracted::FundAssets(fund_assets()),
                Extracted::FundSfdrClassification(fund_sfdr_classification()),
                Extracted::FundEsgIndicator(fund_esg_indicator()),
                Extracted::FundRename(fund_rename()),
                Extracted::FundMerge(fund_merge()),
                Extracted::ManagementCompany(management_company()),
                Extracted::InvestmentsManager(investments_manager()),
                Extracted::Promises(PromiseEntries::new()),
                Extracted::PageClass(None),
            ]
        }

        /// The same twelve variants, but with **every promisable field still pending**.
        ///
        /// This is the state in which an [`Extracted`] really crosses a process boundary: a worker
        /// serialises what it produced and the parent fulfils the promises afterwards, once, over
        /// every job. Building the fixtures resolved — as `one_of_each` does — exercises the easy
        /// half and let a pending promise go unread for a long time.
        fn one_of_each_still_promised() -> Vec<Extracted> {
            let promise = |id: &str| BlockValue::Promise(Promise::new(id));

            let investment_fields = || {
                let mut f = InvestmentFields::new(
                    "Acme Corp",
                    "Acme",
                    promise("fund"),
                    promise("market_value"),
                    promise("currency"),
                );
                f.perc_net_assets = Some(promise("perc"));
                f.acquisition_cost = Some(promise("cost"));
                f.acquisition_currency = Some(promise("acq_currency"));
                f
            };

            vec![
                Extracted::Equity(Equity::build(investment_fields()).expect("promises are always admissible")),
                Extracted::Bond(
                    Bond::build(investment_fields(), None, None).expect("promises are always admissible"),
                ),
                Extracted::Fund(Fund::from_value(&promise("fund_name")).expect("a promise is a valid name")),
                Extracted::FundAssets(
                    FundAssets::build("Alpha Fund", 100.0, 40.0, 60.0, &promise("currency"), None)
                        .expect("promises are always admissible"),
                ),
                Extracted::FundSfdrClassification(
                    FundSfdrClassification::build("Alpha Fund", &promise("article"))
                        .expect("promises are always admissible"),
                ),
                Extracted::FundEsgIndicator(
                    FundEsgIndicator::build(&promise("fund"), "GHG intensity", "12.3")
                        .expect("promises are always admissible"),
                ),
                Extracted::FundRename(
                    FundRename::build("Old Fund", "New Fund", &promise("date"))
                        .expect("promises are always admissible"),
                ),
                Extracted::FundMerge(
                    FundMerge::build("Old Fund", "New Fund", &promise("date"))
                        .expect("promises are always admissible"),
                ),
                // The two managers have no promisable field at all — their own documentation says
                // so — hence they appear here as they do in `one_of_each`.
                Extracted::ManagementCompany(management_company()),
                Extracted::InvestmentsManager(investments_manager()),
                Extracted::Promises(
                    [("fund_name", promise("other")), ("isin", BlockValue::Int(1))].into_iter().collect(),
                ),
                Extracted::PageClass(Some(PageClass::new("investments"))),
            ]
        }

        mod accessors {
            use super::*;
            use pretty_assertions::assert_eq;

            #[test]
            fn as_fund_assets_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::FundAssets(_));
                    assert_eq!(e.as_fund_assets().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_fund_sfdr_classification_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::FundSfdrClassification(_));
                    assert_eq!(e.as_fund_sfdr_classification().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_fund_esg_indicator_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::FundEsgIndicator(_));
                    assert_eq!(e.as_fund_esg_indicator().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_fund_rename_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::FundRename(_));
                    assert_eq!(e.as_fund_rename().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_fund_merge_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::FundMerge(_));
                    assert_eq!(e.as_fund_merge().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_management_company_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::ManagementCompany(_));
                    assert_eq!(e.as_management_company().is_some(), expect_some, "{e:?}");
                }
            }

            #[test]
            fn as_investments_manager_returns_some_only_for_its_own_variant() {
                for e in one_of_each() {
                    let expect_some = matches!(e, Extracted::InvestmentsManager(_));
                    assert_eq!(e.as_investments_manager().is_some(), expect_some, "{e:?}");
                }
            }

            /// The entity variants must never satisfy the accessors of the others
            /// (`as_fund`/`as_equity`/`as_bond`/`as_promises`/`as_page_class`).
            #[test]
            fn the_five_pre_existing_accessors_reject_every_new_variant_too() {
                for e in one_of_each() {
                    assert_eq!(e.as_fund().is_some(), matches!(e, Extracted::Fund(_)), "{e:?}");
                    assert_eq!(e.as_equity().is_some(), matches!(e, Extracted::Equity(_)), "{e:?}");
                    assert_eq!(e.as_bond().is_some(), matches!(e, Extracted::Bond(_)), "{e:?}");
                    assert_eq!(e.as_promises().is_some(), matches!(e, Extracted::Promises(_)), "{e:?}");
                    assert_eq!(e.as_page_class().is_some(), matches!(e, Extracted::PageClass(_)), "{e:?}");
                }
            }
        }

        /// An [`Extracted`] travels back from a worker process to the parent serialised as JSON.
        /// Reuses `one_of_each`, so a thirteenth variant enters the round trip automatically
        /// instead of staying silently uncovered.
        mod serde_round_trip {
            use super::*;
            use pretty_assertions::assert_eq;

            use std::collections::BTreeMap;

            fn round_trip(e: &Extracted) -> Extracted {
                let json = serde_json::to_string(e).expect("an extracted value must serialize");
                serde_json::from_str(&json).expect("a serialized extracted value must deserialize back")
            }

            #[test]
            fn every_variant_survives_a_json_round_trip() {
                for e in one_of_each() {
                    assert_eq!(round_trip(&e), e, "variant did not survive: {e:?}");
                }
            }

            /// The crossing as it really happens: nothing is fulfilled yet.
            #[test]
            fn every_variant_survives_it_with_its_fields_still_promised() {
                for e in one_of_each_still_promised() {
                    assert_eq!(round_trip(&e), e, "variant did not survive: {e:?}");
                }
            }

            /// How many fields of an entity are still waiting for a promise. Written as an
            /// exhaustive match so that a thirteenth variant has to be answered for here too.
            fn pending_count(e: &Extracted) -> usize {
                use crate::core::promisable::PromisableFields;
                match e {
                    Extracted::Equity(v) => v.pending().len(),
                    Extracted::Bond(v) => v.pending().len(),
                    Extracted::Fund(v) => v.pending().len(),
                    Extracted::FundAssets(v) => v.pending().len(),
                    Extracted::FundSfdrClassification(v) => v.pending().len(),
                    Extracted::FundEsgIndicator(v) => v.pending().len(),
                    Extracted::FundRename(v) => v.pending().len(),
                    Extracted::FundMerge(v) => v.pending().len(),
                    Extracted::ManagementCompany(v) => v.pending().len(),
                    Extracted::InvestmentsManager(v) => v.pending().len(),
                    Extracted::Promises(_) | Extracted::PageClass(_) => 0,
                }
            }

            /// Surviving is not enough: a promise must not come back looking like a value. For a
            /// promised **name** the two are both strings, so an untagged form read the promise id
            /// back as the fund's name and no comparison of the entity would have noticed.
            #[test]
            fn a_pending_field_comes_back_pending_and_not_as_a_resolved_lookalike() {
                for e in one_of_each_still_promised() {
                    let before = pending_count(&e);
                    assert!(
                        before > 0
                            || matches!(
                                e,
                                Extracted::Promises(_)
                                    | Extracted::PageClass(_)
                                    | Extracted::ManagementCompany(_)
                                    | Extracted::InvestmentsManager(_)
                            ),
                        "the fixture must really be pending: {e:?}"
                    );
                    let back = round_trip(&e);
                    assert_eq!(pending_count(&back), before, "{e:?} came back as {back:?}");
                }
            }

            /// Promises are the only variant carrying an arbitrary nested structure (`BlockValue`,
            /// which is recursive). The empty `PromiseEntries` that `one_of_each` uses would say
            /// nothing about nesting.
            #[test]
            fn promise_entries_survive_with_nested_block_values() {
                let entries: PromiseEntries = [
                    ("scalar", BlockValue::Int(42)),
                    (
                        "list",
                        BlockValue::List(vec![BlockValue::Str("a".to_string()), BlockValue::Null, BlockValue::Bool(true)]),
                    ),
                    (
                        "map",
                        BlockValue::Map(BTreeMap::from([(
                            "inner".to_string(),
                            BlockValue::Set(BTreeSet::from([BlockValue::Int(1), BlockValue::Int(2)])),
                        )])),
                    ),
                ]
                .into_iter()
                .collect();
                let e = Extracted::Promises(entries);
                assert_eq!(round_trip(&e), e);
            }

            /// The order of the promises decides who wins when a promise is not *multiple* (see
            /// `FlatPromiseMap::fulfill`): were the round trip to lose it, a parallel job would
            /// produce a different result from the same job run sequentially.
            #[test]
            fn the_order_of_promise_entries_is_preserved() {
                let entries: PromiseEntries =
                    [("k", BlockValue::Int(1)), ("k", BlockValue::Int(2)), ("k", BlockValue::Int(3))].into_iter().collect();
                let restored = round_trip(&Extracted::Promises(entries.clone()));
                assert_eq!(restored, Extracted::Promises(entries));
            }

            #[test]
            fn a_page_class_survives_both_when_present_and_when_absent() {
                for value in [None, Some(PageClass::new("investments"))] {
                    let e = Extracted::PageClass(value.clone());
                    assert_eq!(round_trip(&e), e, "page class {value:?} did not survive");
                }
            }
        }

        /// The exhaustive `match` — with no `_ =>` arm — over all twelve variants: a thirteenth
        /// added without touching this test is reported by the compiler right here.
        mod exhaustive_match {
            use super::*;
            use pretty_assertions::assert_eq;

            fn label(e: &Extracted) -> &'static str {
                match e {
                    Extracted::Equity(_) => "equity",
                    Extracted::Bond(_) => "bond",
                    Extracted::Fund(_) => "fund",
                    Extracted::FundAssets(_) => "fund_assets",
                    Extracted::FundSfdrClassification(_) => "fund_sfdr_classification",
                    Extracted::FundEsgIndicator(_) => "fund_esg_indicator",
                    Extracted::FundRename(_) => "fund_rename",
                    Extracted::FundMerge(_) => "fund_merge",
                    Extracted::ManagementCompany(_) => "management_company",
                    Extracted::InvestmentsManager(_) => "investments_manager",
                    Extracted::Promises(_) => "promises",
                    Extracted::PageClass(_) => "page_class",
                }
            }

            #[test]
            fn every_variant_matches_its_own_arm_in_declaration_order() {
                let expected = [
                    "equity",
                    "bond",
                    "fund",
                    "fund_assets",
                    "fund_sfdr_classification",
                    "fund_esg_indicator",
                    "fund_rename",
                    "fund_merge",
                    "management_company",
                    "investments_manager",
                    "promises",
                    "page_class",
                ];
                let labels: Vec<&str> = one_of_each().iter().map(label).collect();
                assert_eq!(labels, expected);
            }
        }
    }
}
