//! [`TextFilterOnlyDeclaredFunds`]: *an annex that is a document of its own keeps its datum only if
//! the fund has already been declared*.
//!
//! # What it is for
//!
//! MEDIOLANUM-ES24.B publishes its report in two files: the main document, which names the sub-fund
//! and lists its holdings, and the **annex** carrying the periodic SFDR disclosure, which has
//! nothing of the sub-fund but the product name at the head of the form. Passed together they are
//! **one job**: the pages of both land in one pool and the schedule runs once over the union, so
//! the annex's step receives as its data what the main document's step extracted. That is where one
//! reads whether the fund agrees — and `R30` asks for exactly this.
//!
//! # Why `demand_investment_funds_match` is not enough
//!
//! That parameter exists and looks at the funds **of the holdings** of the previous step. On this
//! format it cannot work: the holdings of MEDIOLANUM-ES24.B do not name the sub-fund, they
//! **promise** it, and promises are resolved by the parent *after* every job has been collected.
//! During the schedule the field is still pending, so the set of known funds is empty and **every**
//! article of the annex would be discarded.
//!
//! Here instead one looks at what is already a name in the same data: the [`Fund`]s a page has
//! declared — the cover page of the main document declares one — and the `fund` field of any result
//! that carries it written out. The two routes do not exclude each other: a format whose main
//! document names the sub-fund inside the holdings rows is served by both.
//!
//! # Where it lives, and why here
//!
//! It is text interpretation, hence `text_filter` (`R23`): comparing two spellings of a name is
//! neither geometry nor a cast. The comparison uses [`MatchFund`], the same identity the engine
//! uses to deduplicate funds, rather than a normalisation written by hand.
//!
//! # The price, stated
//!
//! The name in the annex must **agree** with the name in the main document up to normalisation. If
//! the annex prefixes the umbrella where the main document does not, the comparison fails and the
//! datum is lost: that is `R24`, and it is why the discard **warns** instead of being silent.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::core::classes::TextBlock;
use crate::core::match_fund::MatchFund;
use crate::core::pipeline::{FilterData, PipeError, TextFilterPipe};

/// The names of the funds the earlier steps of this job have already declared, normalised.
///
/// Two sources, and neither is a convenience:
///
/// - the [`Fund`]s, which are the explicit statement *this sub-fund exists* — its name may still be
///   a promise, though, and a promise is not a name, so it is skipped;
/// - the `fund` field of any other result that carries it written out (a holding, an assets
///   statement), which is how most formats name the sub-fund.
pub fn declared_funds(data: &FilterData<'_>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut add = |candidate: &str| {
        if !candidate.trim().is_empty() {
            names.insert(MatchFund::new(candidate).normalized().to_string());
        }
    };
    for result in data.previous() {
        if let Some(fund) = result.as_fund()
            && let Some(name) = fund.name()
        {
            add(&name);
        }
        if let Some(assets) = result.as_fund_assets() {
            add(&assets.fund);
        }
        if let Some(sfdr) = result.as_fund_sfdr_classification() {
            add(&sfdr.fund);
        }
        if let Some(esg) = result.as_fund_esg_indicator()
            && let Some(name) = esg.fund.resolved()
        {
            add(name);
        }
        for holding in [result.as_equity().map(|e| &e.data), result.as_bond().map(|b| &b.data)] {
            if let Some(investment) = holding
                && let Some(name) = investment.fund.resolved()
            {
                add(name);
            }
        }
    }
    names
}

/// The `text_filter` of another pipe, keeping a block only if its fund is already declared.
///
/// It wraps the annex's own filter — which does not check the fund itself, because
/// `demand_investment_funds_match` looks at something else (see the module documentation) — and
/// changes neither its blocks nor their metadata: it only decides which ones pass.
pub struct TextFilterOnlyDeclaredFunds {
    inner: Arc<dyn TextFilterPipe>,
    name: String,
}

impl TextFilterOnlyDeclaredFunds {
    pub fn new(inner: Arc<dyn TextFilterPipe>) -> Self {
        let name = format!("{} (only declared funds)", inner.name());
        Self { inner, name }
    }
}

impl TextFilterPipe for TextFilterOnlyDeclaredFunds {
    fn name(&self) -> &str {
        &self.name
    }

    fn filter(
        &self,
        blocks: &[crate::core::classes::PdfBlock],
        data: &FilterData<'_>,
    ) -> Result<Vec<TextBlock>, PipeError> {
        let passed = self.inner.filter(blocks, data)?;
        if passed.is_empty() {
            return Ok(passed);
        }
        let declared = declared_funds(data);
        if declared.is_empty() {
            tracing::error!(
                "no page of this job has declared a fund yet: this annex cannot be matched against \
                 the report it belongs to, and its disclosure is dropped - was the main document \
                 passed in the same job?"
            );
            return Ok(Vec::new());
        }
        let mut kept = Vec::new();
        for block in passed {
            let name = block.content.as_str().unwrap_or_default().trim().to_string();
            if declared.contains(MatchFund::new(&name).normalized()) {
                kept.push(block);
            } else {
                tracing::warn!(
                    "'{name}' is not a fund this report declares: the annex's article is dropped - \
                     it is either another fund's annex or the same fund written differently"
                );
            }
        }
        Ok(kept)
    }

    /// The wrapped pipe decides: a Python annex filter inside this one is still a Python pipe.
    fn scales_with_threads(&self) -> bool {
        self.inner.scales_with_threads()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::{BlockType, PdfBlock};
    use crate::core::pipeline::Extracted;
    use crate::output::classes::fund::Fund;

    /// A filter that passes every block through as a text block of the same content: it stands in
    /// for the annex's own filter, so these tests measure the wrapper and nothing else.
    struct PassEverything;

    impl TextFilterPipe for PassEverything {
        fn name(&self) -> &str {
            "pass everything"
        }

        fn filter(&self, blocks: &[PdfBlock], _: &FilterData<'_>) -> Result<Vec<TextBlock>, PipeError> {
            Ok(blocks
                .iter()
                .map(|block| TextBlock::from_content(block.type_block.clone(), Default::default(), block.content.clone()))
                .collect())
        }
    }

    fn wrapped() -> TextFilterOnlyDeclaredFunds {
        TextFilterOnlyDeclaredFunds::new(Arc::new(PassEverything))
    }

    fn block(content: &str) -> PdfBlock {
        PdfBlock::bare(BlockType::FUND_NAME, content)
    }

    fn contents(blocks: &[TextBlock]) -> Vec<String> {
        blocks.iter().map(|b| b.content.as_str().unwrap_or_default().to_string()).collect()
    }

    mod what_counts_as_declared {
        use super::*;

        #[test]
        fn a_fund_declared_by_name_counts() {
            let previous = vec![Extracted::Fund(Fund::new("Alpha Fund"))];
            let declared = declared_funds(&FilterData::Previous(&previous));
            assert_eq!(declared.len(), 1);
        }

        #[test]
        fn nothing_extracted_declares_nothing() {
            assert!(declared_funds(&FilterData::EMPTY).is_empty());
        }

        #[test]
        fn the_set_is_normalised_so_two_spellings_of_one_fund_are_one_entry() {
            let previous =
                vec![Extracted::Fund(Fund::new("Alpha Fund")), Extracted::Fund(Fund::new("ALPHA  FUND"))];
            assert_eq!(declared_funds(&FilterData::Previous(&previous)).len(), 1);
        }

        #[test]
        fn a_name_of_nothing_but_spaces_is_not_a_declaration() {
            let previous = vec![Extracted::Fund(Fund::new("   "))];
            assert!(declared_funds(&FilterData::Previous(&previous)).is_empty());
        }

        #[test]
        fn the_target_companies_data_declares_nothing_rather_than_failing() {
            // The first step of a schedule receives the target companies, not previous results.
            let companies: Vec<crate::formats_utils::text_filter::matcher::CompanyMatchInfos> = Vec::new();
            assert!(declared_funds(&FilterData::TargetCompanies(&companies)).is_empty());
        }
    }

    mod what_passes {
        use super::*;

        #[test]
        fn a_block_naming_a_declared_fund_passes() {
            let previous = vec![Extracted::Fund(Fund::new("Alpha Fund"))];
            let got = wrapped().filter(&[block("Alpha Fund")], &FilterData::Previous(&previous)).unwrap();
            assert_eq!(contents(&got), ["Alpha Fund"]);
        }

        #[test]
        fn the_comparison_is_the_engines_own_fund_identity_and_not_a_literal_one() {
            let previous = vec![Extracted::Fund(Fund::new("Alpha Fund"))];
            let got = wrapped().filter(&[block("  ALPHA   FUND ")], &FilterData::Previous(&previous)).unwrap();
            assert_eq!(got.len(), 1);
        }

        #[test]
        fn a_block_naming_another_fund_is_dropped() {
            let previous = vec![Extracted::Fund(Fund::new("Alpha Fund"))];
            let got = wrapped().filter(&[block("Beta Fund")], &FilterData::Previous(&previous)).unwrap();
            assert!(got.is_empty());
        }

        #[test]
        fn only_the_blocks_that_agree_are_kept_when_several_arrive() {
            let previous = vec![Extracted::Fund(Fund::new("Alpha Fund"))];
            let blocks = [block("Alpha Fund"), block("Beta Fund")];
            let got = wrapped().filter(&blocks, &FilterData::Previous(&previous)).unwrap();
            assert_eq!(contents(&got), ["Alpha Fund"]);
        }

        #[test]
        fn nothing_declared_drops_everything_rather_than_passing_it() {
            // The annex cannot be matched against the report it belongs to: the datum is dropped,
            // loudly, because the likely cause is the main document not being in the same job.
            let got = wrapped().filter(&[block("Alpha Fund")], &FilterData::EMPTY).unwrap();
            assert!(got.is_empty());
        }

        #[test]
        fn a_wrapped_pipe_that_passes_nothing_is_not_asked_about_declarations() {
            // No block, no question: the empty answer comes back untouched, so an annex page that
            // carries no disclosure does not log a missing declaration.
            let got = wrapped().filter(&[], &FilterData::EMPTY).unwrap();
            assert!(got.is_empty());
        }
    }

    mod the_wrapper_itself {
        use super::*;

        #[test]
        fn its_name_says_which_pipe_it_wraps_and_what_it_adds() {
            let name = wrapped().name().to_string();
            assert!(name.starts_with("pass everything"), "{name}");
            assert!(name.contains("only declared funds"), "{name}");
        }

        #[test]
        fn it_defers_to_the_wrapped_pipe_on_whether_threads_pay_off() {
            assert_eq!(wrapped().scales_with_threads(), PassEverything.scales_with_threads());
        }
    }
}
