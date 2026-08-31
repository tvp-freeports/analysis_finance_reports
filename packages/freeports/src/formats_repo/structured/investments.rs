//! The structured `investments` pipeline: the repository's most used.
//!
//! It builds the six pipes that extract a fund's holdings from a table:
//!
//! - `pdf_extract` — the table body, the fund name, the currency;
//! - `text_filter` — [`TextFilterInvestmentsStandard`], which crosses the rows with the target companies;
//! - `deserialize` — the investment and fund deserializers.
//!
//! Each segment can be switched off individually: a format may use structured extraction and then
//! filter with its own code, or the other way round. That is what "partial pipelines" means, and
//! the reason the merge of the three levels exists at all.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::core::pipeline::{Pipeline, PipelineName};
use crate::formats_utils::deserialize::standard_funcs::{DeserializerFundStandard, DeserializerInvestmentStandard};
use crate::formats_utils::pdf_extract::standard_funcs::{
    InvestmentsStandardArgs, PdfExtractInvestmentsStandard, pdf_extract_currency_standard, pdf_extract_fund_standard,
};
use crate::formats_utils::pdf_extract::tabularizer::coordinates::TablePosAlgorithm;
use crate::formats_utils::text_filter::standard_funcs::TextFilterInvestmentsStandard;
use crate::input::document::selection::pdfline_selection_from_str;

use super::StructuredError;
use super::tables::{InvestmentsConfig, get_investments_configs};

/// Parses the algorithm-flags column. An empty cell means no flags.
fn parse_algorithm_flags(config: &InvestmentsConfig) -> Result<TablePosAlgorithm, StructuredError> {
    let Some(expression) = config.additional.as_ref().and_then(|a| a.algorithm_flags.as_deref()) else {
        return Ok(TablePosAlgorithm::Default);
    };
    TablePosAlgorithm::from_expression(expression)
        .map_err(|source| StructuredError::AlgorithmFlags { id: config.id.to_string(), source })
}

/// Parses a selection cell already validated by [`super::tables`], naming the column in the error.
fn selection(
    config: &InvestmentsConfig,
    column: &'static str,
    raw: &str,
) -> Result<crate::formats_utils::pdf_extract::select::relative::PdfLineSelection, StructuredError> {
    pdfline_selection_from_str(raw)
        .map_err(|source| StructuredError::LineSelection { id: config.id.to_string(), column, source })
}

/// Adds the three extraction pipes of one investments pipe.
fn add_pdf_extract(pipeline: &mut Pipeline, config: &InvestmentsConfig) -> Result<(), StructuredError> {
    let mut deselection_list = Vec::with_capacity(config.deselection_sets.len());
    for raw in &config.deselection_sets {
        deselection_list.push(selection(config, "Deselection set", raw)?);
    }

    // An empty cell is the empty selection: the pipe is built either way, and it is the
    // repository's business to fill it in if it really matters.
    let body_set = selection(config, "Body set", config.args.body_set.as_deref().unwrap_or(""))?;
    let args = InvestmentsStandardArgs {
        deselection_list,
        algorithm_flags: parse_algorithm_flags(config)?,
        tolerance: config.additional.as_ref().and_then(|a| a.tolerance).unwrap_or(0.0),
        ..InvestmentsStandardArgs::new(body_set)
    };
    pipeline.pdf_extract.push(Arc::new(PdfExtractInvestmentsStandard::new(args)));

    let subfund_set = selection(config, "Subfund set", config.args.subfund_set.as_deref().unwrap_or(""))?;
    pipeline.pdf_extract.push(Arc::new(pdf_extract_fund_standard(subfund_set)));

    let currency_set = selection(config, "Currency set", config.args.currency_set.as_deref().unwrap_or(""))?;
    pipeline.pdf_extract.push(Arc::new(pdf_extract_currency_standard(currency_set)));
    Ok(())
}

/// Adds the filtering pipe of one investments pipe.
fn add_text_filter(pipeline: &mut Pipeline, config: &InvestmentsConfig) -> Result<(), StructuredError> {
    // The only parameter without a default, so its absence is a named error saying which pipe
    // caused it rather than a failure deeper down.
    let market_value_pos =
        config.args.market_value.ok_or_else(|| StructuredError::MissingMarketValue { id: config.id.to_string() })?;
    let additional = config.additional.as_ref();
    let pipe = TextFilterInvestmentsStandard::new(
        market_value_pos as i64,
        config.args.quantity.map(i64::from),
        config.args.perc_net_assets.map(i64::from),
        config.args.acquisition_currency.map(i64::from),
        config.args.acquisition_cost.map(i64::from),
        // The defaults are the **constructor's**, not `false` for both: a cell is only filled in
        // when it is non-empty, and geometric indexing defaults to true. A `false` here would index
        // the fields on the *flat* list of blocks instead of on the grid, and every row whose name
        // wrapped would shift the columns by one.
        additional.and_then(|a| a.geometrical_indexing).unwrap_or(true),
        additional.and_then(|a| a.merge_previous).unwrap_or(false),
    )
    .map_err(|source| StructuredError::TextFilter { id: config.id.to_string(), source })?;
    pipeline.text_filter.push(Arc::new(pipe));
    Ok(())
}

/// Adds the two deserialization pipes of one investments pipe.
fn add_deserialize(pipeline: &mut Pipeline, config: &InvestmentsConfig) {
    let additional = config.additional.as_ref();
    pipeline.deserialize.push(Arc::new(DeserializerInvestmentStandard::new(
        additional.and_then(|a| a.interpret_cost_and_value_as_int).unwrap_or(true),
        additional.and_then(|a| a.interpret_quantity_as_float).unwrap_or(false),
    )));
    pipeline.deserialize.push(Arc::new(DeserializerFundStandard));
}

/// The investments pipelines the repository defines for `format_name`.
pub fn get_pipelines(
    formats_repo_dir: &Path,
    format_name: &str,
) -> Result<HashMap<PipelineName, Pipeline>, StructuredError> {
    let configs = get_investments_configs(formats_repo_dir)?;
    let mut pipelines: HashMap<PipelineName, Pipeline> = HashMap::new();

    for config in configs.into_iter().filter(|c| c.id.format == format_name) {
        let name = PipelineName::new(&config.id.pipeline);
        let pipeline = pipelines.entry(name.clone()).or_insert_with(|| Pipeline::new(name));
        if config.wants_pdf_extract() {
            add_pdf_extract(pipeline, &config)?;
        }
        if config.wants_text_filter() {
            add_text_filter(pipeline, &config)?;
        }
        if config.wants_deserialize() {
            add_deserialize(pipeline, &config);
        }
    }

    tracing::debug!(pipeline_count = pipelines.len(), "built investments pipelines");
    Ok(pipelines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::classes::{BlockType, BlockValue, PdfBlock};
    use crate::core::page::Page;
    use crate::core::pipeline::FilterData;
    use crate::formats_utils::pdf_extract::pdf_line::PdfLine;
    use std::fs;
    use tempfile::TempDir;

    const ARGS_HEADER: &str =
        "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n";
    const ADD_HEADER: &str = "ID,Algorithm flags,Tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Merge previous\n";
    const PARTIAL_HEADER: &str = "ID,pdf_extract,text_filter,deserialize\n";
    const DESEL_HEADER: &str = "ID,Deselection set\n";
    const PAGE_CLASSIFY_HEADER: &str = "ID,Header set,Class\n";

    /// A formats repository with the five structured CSV files, all empty but the ones a test fills
    /// in.
    struct Repo {
        dir: TempDir,
    }

    impl Repo {
        fn new() -> Self {
            let dir = TempDir::new().expect("temp dir");
            let base = dir.path().join(super::super::tables::STRUCTURED_DIR);
            fs::create_dir_all(base.join("investments")).expect("investments dir");
            fs::create_dir_all(base.join("page_classify")).expect("page_classify dir");
            let repo = Self { dir };
            repo.write("investments/args.csv", ARGS_HEADER);
            repo.write("investments/additional_args.csv", ADD_HEADER);
            repo.write("investments/partial_pipes.csv", PARTIAL_HEADER);
            repo.write("investments/deselection_lists.csv", DESEL_HEADER);
            repo.write("page_classify/args.csv", PAGE_CLASSIFY_HEADER);
            repo
        }

        fn write(&self, relative: &str, content: &str) -> &Self {
            fs::write(self.dir.path().join(super::super::tables::STRUCTURED_DIR).join(relative), content)
                .expect("write csv");
            self
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }
    }

    fn only_pipeline(pipelines: HashMap<PipelineName, Pipeline>) -> Pipeline {
        assert_eq!(pipelines.len(), 1, "expected exactly one pipeline");
        pipelines.into_values().next().expect("one pipeline")
    }

    mod segments_built {
        use super::*;
        use pretty_assertions::assert_eq;

        fn one_pipe() -> Repo {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,ArialMT,ArialNarrow,Arial,1,,,,\n"));
            repo
        }

        #[test]
        fn builds_three_pdf_extract_pipes_one_text_filter_and_two_deserializers() {
            let repo = one_pipe();
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").unwrap());
            assert_eq!(pipeline.pdf_extract.len(), 3);
            assert_eq!(pipeline.text_filter.len(), 1);
            assert_eq!(pipeline.deserialize.len(), 2);
        }

        #[test]
        fn the_resulting_pipeline_is_complete() {
            let repo = one_pipe();
            assert!(only_pipeline(get_pipelines(repo.path(), "A-EN24").unwrap()).is_complete());
        }

        #[test]
        fn the_pipeline_is_named_after_the_derived_pipeline_of_its_rows() {
            let repo = one_pipe();
            let pipelines = get_pipelines(repo.path(), "A-EN24").unwrap();
            assert!(pipelines.contains_key(&PipelineName::new("investments")));
        }

        #[test]
        fn another_format_gets_no_pipeline_at_all() {
            let repo = one_pipe();
            assert!(get_pipelines(repo.path(), "B-EN24").unwrap().is_empty());
        }

        #[test]
        fn two_rows_of_one_format_add_their_pipes_to_the_same_pipeline() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\nA-EN24,,,,2,,,,\n"));
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").unwrap());
            assert_eq!(pipeline.pdf_extract.len(), 6);
            assert_eq!(pipeline.text_filter.len(), 2);
        }

        #[test]
        fn a_row_naming_another_pipeline_builds_a_second_pipeline() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\nA-EN24(manco),,,,1,,,,\n"));
            let pipelines = get_pipelines(repo.path(), "A-EN24").unwrap();
            assert_eq!(pipelines.len(), 2);
            assert!(pipelines.contains_key(&PipelineName::new("manco")));
        }
    }

    mod partial_segments {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn a_disabled_pdf_extract_leaves_that_segment_empty() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo.write("investments/partial_pipes.csv", &format!("{PARTIAL_HEADER}A-EN24,FALSE,TRUE,TRUE\n"));
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").unwrap());
            assert!(pipeline.pdf_extract.is_empty());
            assert_eq!(pipeline.text_filter.len(), 1);
            assert!(!pipeline.is_complete());
        }

        #[test]
        fn a_disabled_text_filter_leaves_that_segment_empty() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,,,,,\n"));
            repo.write("investments/partial_pipes.csv", &format!("{PARTIAL_HEADER}A-EN24,TRUE,FALSE,TRUE\n"));
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").unwrap());
            assert!(pipeline.text_filter.is_empty());
            assert_eq!(pipeline.pdf_extract.len(), 3);
        }

        #[test]
        fn a_disabled_deserialize_leaves_that_segment_empty() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo.write("investments/partial_pipes.csv", &format!("{PARTIAL_HEADER}A-EN24,TRUE,TRUE,FALSE\n"));
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").unwrap());
            assert!(pipeline.deserialize.is_empty());
        }

        #[test]
        fn a_pipe_whose_text_filter_is_active_but_has_no_market_value_is_rejected() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,,,,,\n"));
            let err = get_pipelines(repo.path(), "A-EN24").unwrap_err();
            assert!(matches!(err, StructuredError::MissingMarketValue { .. }), "{err}");
        }
    }

    mod parameters_reach_the_pipes {
        use super::*;
        use pretty_assertions::assert_eq;

        /// A page with a title (the fund name), a currency statement, and two table rows side by
        /// side.
        fn page() -> Page {
            Page::new(
                1,
                (300.0, 300.0),
                vec![
                    PdfLine::new("ArialBold", 10.0, "Alpha Fund", (0.0, 0.0, 60.0, 10.0)),
                    PdfLine::new("ArialItalic", 10.0, "Amounts in EUR", (0.0, 10.0, 60.0, 18.0)),
                    PdfLine::new("Arial", 10.0, "Acme Corp", (0.0, 20.0, 40.0, 30.0)),
                    PdfLine::new("Arial", 10.0, "1.000", (50.0, 20.0, 90.0, 30.0)),
                ],
                Vec::new(),
            )
        }

        #[test]
        fn the_body_set_selects_only_the_declared_lines() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,ArialBold,ArialItalic,Arial,1,,,,\n"));
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").unwrap());
            let blocks = pipeline.apply_pdf_extract(&page()).unwrap();
            let table_rows: Vec<_> =
                blocks.iter().filter(|b| b.type_block == BlockType::TABLE_BODY).collect();
            assert_eq!(table_rows.len(), 2);
        }

        #[test]
        fn the_subfund_set_becomes_the_fund_name_block() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,ArialBold,ArialItalic,Arial,1,,,,\n"));
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").unwrap());
            let blocks = pipeline.apply_pdf_extract(&page()).unwrap();
            let fund: Vec<&PdfBlock> = blocks.iter().filter(|b| b.type_block == BlockType::FUND_NAME).collect();
            assert_eq!(fund.len(), 1);
            assert_eq!(fund[0].content, BlockValue::from("Alpha Fund"));
        }

        #[test]
        fn a_deselection_row_removes_lines_from_the_body_set() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,ArialBold,ArialItalic,Arial,1,,,,\n"));
            repo.write("investments/deselection_lists.csv", &format!("{DESEL_HEADER}A-EN24,\"Arial \"\"^1.000$\"\"\"\n"));
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").unwrap());
            let blocks = pipeline.apply_pdf_extract(&page()).unwrap();
            let table_rows: Vec<_> = blocks.iter().filter(|b| b.type_block == BlockType::TABLE_BODY).collect();
            assert_eq!(table_rows.len(), 1);
        }

        #[test]
        fn the_column_positions_reach_the_text_filter_pipe() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,ArialBold,ArialItalic,Arial,1,2,3,,\n"));
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").unwrap());
            // There is no typed accessor on the segment, so the behaviour is checked instead:
            // construction with distinct positions succeeds and with equal ones does not.
            assert_eq!(pipeline.text_filter.len(), 1);
        }

        #[test]
        fn two_equal_column_positions_are_rejected_by_the_text_filter_pipe() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,1,3,,\n"));
            let err = get_pipelines(repo.path(), "A-EN24").unwrap_err();
            assert!(matches!(err, StructuredError::TextFilter { .. }), "{err}");
        }

        #[test]
        fn the_pipeline_runs_end_to_end_on_a_page_without_target_companies() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,ArialBold,ArialItalic,Arial,1,,,,\n"));
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").unwrap());
            // No target companies: the pipeline runs and extracts nothing, without failing.
            let result = pipeline.apply(&page(), &FilterData::TargetCompanies(&[]));
            assert!(result.is_ok(), "{:?}", result.err());
        }
    }

    mod algorithm_flags {
        use super::*;
        use pretty_assertions::assert_eq;

        fn flags_of(expression: &str) -> Result<TablePosAlgorithm, StructuredError> {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}A-EN24,{expression},,,,,\n"));
            let configs = get_investments_configs(repo.path())?;
            parse_algorithm_flags(&configs[0])
        }

        #[test]
        fn an_empty_cell_means_no_flag() {
            assert_eq!(flags_of("").unwrap().bits(), TablePosAlgorithm::Default.bits());
        }

        #[test]
        fn a_single_flag_name_is_resolved() {
            assert!(flags_of("USE_RULER_AREA").unwrap().contains(TablePosAlgorithm::UseRulerArea));
        }

        #[test]
        fn every_flag_name_of_the_enum_is_known() {
            for name in ["RETURN_ROWS", "BIG_CELL_RULE", "USE_RULER_AREA", "USE_TEST_POS"] {
                assert!(flags_of(name).is_ok(), "{name} should be a known flag");
            }
        }

        #[test]
        fn several_flags_can_be_combined() {
            let flags = flags_of("BIG_CELL_RULE | USE_RULER_AREA").unwrap();
            assert!(flags.contains(TablePosAlgorithm::BigCellRule));
            assert!(flags.contains(TablePosAlgorithm::UseRulerArea));
        }

        #[test]
        fn an_unknown_flag_name_is_rejected_naming_the_pipe() {
            let err = flags_of("NOT_A_FLAG").unwrap_err();
            let StructuredError::AlgorithmFlags { id, .. } = err else { panic!("expected AlgorithmFlags") };
            assert_eq!(id, "A-EN24(investments)/0");
        }
    }

    mod merging_with_page_classify {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn the_two_sublevels_merge_into_one_pipeline_when_they_share_a_name() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24(cls),,,,1,,,,\n"));
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24(cls)/0,ArialBold,inv\n"));
            let pipelines = crate::formats_repo::structured::get_pipelines(repo.path(), "A-EN24").unwrap();
            let pipeline = only_pipeline(pipelines);
            // Three pipes from investments plus one classifier.
            assert_eq!(pipeline.pdf_extract.len(), 4);
            assert_eq!(pipeline.text_filter.len(), 2);
            assert_eq!(pipeline.deserialize.len(), 3);
        }

        #[test]
        fn two_sublevels_with_different_names_stay_two_pipelines() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,ArialBold,inv\n"));
            let pipelines = crate::formats_repo::structured::get_pipelines(repo.path(), "A-EN24").unwrap();
            assert_eq!(pipelines.len(), 2);
            assert!(pipelines.contains_key(&PipelineName::new("investments")));
            assert!(pipelines.contains_key(&PipelineName::new("")));
        }

        #[test]
        fn a_page_classify_only_format_still_gets_a_complete_pipeline() {
            let repo = Repo::new();
            repo.write("page_classify/args.csv", &format!("{PAGE_CLASSIFY_HEADER}A-EN24/0,ArialBold,inv\n"));
            let pipeline = only_pipeline(crate::formats_repo::structured::get_pipelines(repo.path(), "A-EN24").unwrap());
            assert!(pipeline.is_complete());
        }
    }
}
