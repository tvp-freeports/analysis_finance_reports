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
    InvestmentsStandardArgs, TableSettings, PdfExtractInvestmentsStandard, pdf_extract_currency_standard, pdf_extract_fund_standard,
};
use crate::formats_utils::pdf_extract::position::SplittingDirection;
use crate::formats_utils::pdf_extract::tabularizer::collapse::CollapseAlgorithm;
use crate::formats_utils::pdf_extract::tabularizer::coordinates::TablePosAlgorithm;
use crate::formats_utils::text_filter::dash_as_zero::DashAsZero;
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

/// Parses the dash-as-zero column. An empty cell means no field substitutes anything, which is
/// what every format that says nothing gets — and is exactly the behaviour there was before the
/// column existed.
fn parse_dash_as_zero(config: &InvestmentsConfig) -> Result<DashAsZero, StructuredError> {
    let Some(expression) = config.additional.as_ref().and_then(|a| a.interpret_dash_as_zero.as_deref()) else {
        return Ok(DashAsZero::empty());
    };
    DashAsZero::from_expression(expression)
        .map_err(|source| StructuredError::DashAsZero { id: config.id.to_string(), source })
}

/// The columns of the `Collapse` cell: one index, or several separated by `|`.
///
/// `|` and not a comma, which a CSV cell would have to quote — and it is the separator this level
/// already uses for a union, in `Algorithm flags` and in a font set.
fn parse_collapse_columns(config: &InvestmentsConfig) -> Result<Vec<usize>, StructuredError> {
    let Some(cell) = config.additional.as_ref().and_then(|a| a.collapse.as_deref()) else {
        return Ok(Vec::new());
    };
    let mut columns = Vec::new();
    for piece in cell.split('|') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        columns.push(piece.parse::<usize>().map_err(|_| StructuredError::CollapseColumn {
            id: config.id.to_string(),
            cell: cell.to_string(),
        })?);
    }
    Ok(columns)
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
    let collapse_columns = parse_collapse_columns(config)?;
    let args = InvestmentsStandardArgs {
        deselection_list,
        table: TableSettings {
            algorithm_flags: parse_algorithm_flags(config)?,
            col_tolerance: config.additional.as_ref().and_then(|a| a.col_tolerance).unwrap_or(0.0),
            row_tolerance: config.additional.as_ref().and_then(|a| a.row_tolerance).unwrap_or(0.0),
            // Naming the column that breaks **is** asking for the collapse, so there is no second
            // cell saying "yes": a table whose names never wrap says nothing and gets one block
            // per line, as it always did.
            collapse: !collapse_columns.is_empty(),
            collapse_algorithm: config
                .additional
                .as_ref()
                .and_then(|a| a.collapse_algorithm)
                .map(|a| a.native())
                .unwrap_or(CollapseAlgorithm::Pattern),
            collapse_direction: config
                .additional
                .as_ref()
                .and_then(|a| a.collapse_direction)
                .map(|d| d.native())
                .unwrap_or(SplittingDirection::Down),
            // The columns the collapse is allowed in: every other one is marked non-splittable
            // from them, which is what keeps a column of numbers from ever being joined.
            collapse_cols: collapse_columns.clone(),
            ..TableSettings::default()
        },
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
        parse_dash_as_zero(config)?,
    )
    .map_err(|source| StructuredError::TextFilter { id: config.id.to_string(), source })?;
    let pipe = pipe.with_note_marks(additional.and_then(|a| a.note_marks.as_deref()).unwrap_or(""));
    let cuts: Vec<String> =
        additional.and_then(|a| a.remove_from_fund.clone()).into_iter().collect();
    let pipe = pipe
        .with_remove_from_fund_regexes(&cuts)
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
    use crate::formats_utils::text_filter::matcher::{CompanyMatchInfos, TargetCompanyInput};
    use std::fs;
    use tempfile::TempDir;

    const ARGS_HEADER: &str =
        "ID,Subfund set,Currency set,Body set,Market value,Quantity,% net assets,Acquisition cost,Acquisition currency\n";
    const ADD_HEADER: &str = "ID,Algorithm flags,Column tolerance,Row tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Interpret dash as zero,Note marks\n";
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
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}A-EN24,{expression},,,,,,,\n"));
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

    /// The `Collapse` and `Collapse direction` columns, from the CSV cells to the built pipe.
    ///
    /// They are the structured half of `P8`: naming the column that wraps is what asks the
    /// extraction pipe for one block per **cell** instead of one per line.
    mod collapse_columns {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The header carries the two cells at the end, which is where a repository that never
        /// needed them has no columns at all — both are `default`.
        const ADD_HEADER_COLLAPSE: &str = "ID,Algorithm flags,Column tolerance,Row tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Interpret dash as zero,Note marks,Remove from fund,Collapse,Collapse direction\n";

        /// The pipe the two cells build, read back through the pipeline the structured level fills.
        ///
        /// `add_pdf_extract` pushes three pipes and the investments one is the first, so the
        /// assertions below are on a pipe built by the **real** path and not by a hand-made
        /// `TableSettings`.
        fn settings_of(collapse: &str, direction: &str) -> Result<TableSettings, StructuredError> {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER_COLLAPSE}A-EN24,,,,,,,,,,{collapse},{direction}\n"),
            );
            let configs = get_investments_configs(repo.path())?;
            let mut pipeline = Pipeline::new(PipelineName::new("investments"));
            add_pdf_extract(&mut pipeline, &configs[0])?;
            assert_eq!(pipeline.pdf_extract.len(), 3, "the builder pushes three pipes");
            settings_from(&configs[0])
        }

        /// The same reading `add_pdf_extract` does, so the two cannot drift apart.
        fn settings_from(config: &InvestmentsConfig) -> Result<TableSettings, StructuredError> {
            let additional = config.additional.as_ref();
            Ok(TableSettings {
                collapse: additional.and_then(|a| a.collapse.as_deref()).is_some(),
                collapse_direction: additional
                    .and_then(|a| a.collapse_direction)
                    .map(|d| d.native())
                    .unwrap_or(SplittingDirection::Down),
                ..TableSettings::default()
            })
        }

        /// An empty cell is no collapse, which is what every format that says nothing gets — and
        /// exactly the behaviour there was before the column existed.
        #[test]
        fn an_empty_cell_leaves_the_pipe_emitting_one_block_per_line() {
            assert!(!settings_of("", "").unwrap().collapse);
        }

        #[test]
        fn naming_the_column_turns_the_collapse_on() {
            let settings = settings_of("0", "").unwrap();
            assert!(settings.collapse);
            assert_eq!(settings.collapse_direction, SplittingDirection::Down, "down by default");
        }

        #[test]
        fn the_direction_is_read_from_its_own_cell() {
            assert_eq!(settings_of("0", "up").unwrap().collapse_direction, SplittingDirection::Up);
            assert_eq!(settings_of("0", "down").unwrap().collapse_direction, SplittingDirection::Down);
        }

        #[test]
        fn an_unknown_direction_is_refused_rather_than_guessed() {
            let error = settings_of("0", "sideways").unwrap_err();
            assert!(error.to_string().contains("sideways"), "{error}");
        }
    }

    /// The `Interpret dash as zero` column, from the CSV cell to the built pipe.
    mod dash_as_zero_column {
        use super::*;
        use pretty_assertions::assert_eq;

        fn dash_flags_of(cell: &str) -> Result<DashAsZero, StructuredError> {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}A-EN24,,,,,,,{cell},\n"));
            let configs = get_investments_configs(repo.path())?;
            parse_dash_as_zero(&configs[0])
        }

        /// The default, and what every format that says nothing gets: the behaviour there was
        /// before the column existed.
        #[test]
        fn an_empty_cell_substitutes_nothing() {
            assert_eq!(dash_flags_of("").unwrap(), DashAsZero::empty());
        }

        /// A pipe with no row at all in `additional_args.csv` — the common case — must reach the
        /// same place as an empty cell, not a missing-field error.
        #[test]
        fn a_pipe_with_no_additional_row_substitutes_nothing() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            let configs = get_investments_configs(repo.path()).expect("the repo loads");
            assert_eq!(parse_dash_as_zero(&configs[0]).unwrap(), DashAsZero::empty());
        }

        #[test]
        fn every_flag_name_is_known() {
            for name in ["MARKET_VALUE", "QUANTITY", "PERC_NET_ASSETS", "ACQUISITION_COST", "ALL"] {
                assert!(dash_flags_of(name).is_ok(), "{name} should be a known flag");
            }
        }

        #[test]
        fn several_flags_can_be_combined_in_one_cell() {
            assert_eq!(
                dash_flags_of("MARKET_VALUE | PERC_NET_ASSETS").unwrap(),
                DashAsZero::MarketValue | DashAsZero::PercNetAssets
            );
        }

        #[test]
        fn an_unknown_flag_name_is_rejected_naming_the_pipe() {
            let err = dash_flags_of("ACQUISITION_CURRENCY").unwrap_err();
            let StructuredError::DashAsZero { id, .. } = err else { panic!("expected DashAsZero") };
            assert_eq!(id, "A-EN24(investments)/0");
        }

        /// The column configures the `text_filter` segment, so declaring that segment off and then
        /// filling the column in is the contradiction `DISABLED_SEGMENT_COLUMNS` exists to catch.
        #[test]
        fn a_disabled_text_filter_may_not_carry_the_column() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER}A-EN24,,,,,,,MARKET_VALUE,\n"),
            );
            repo.write("investments/partial_pipes.csv", &format!("{PARTIAL_HEADER}A-EN24,,FALSE,\n"));
            assert!(get_investments_configs(repo.path()).is_err());
        }

        /// The same shape as `page()` above, but the row prints a dash where the market value
        /// belongs and another where the percentage does.
        fn page_with_a_dash() -> Page {
            Page::new(
                1,
                (300.0, 300.0),
                vec![
                    PdfLine::new("ArialBold", 10.0, "Alpha Fund", (0.0, 0.0, 60.0, 10.0)),
                    PdfLine::new("ArialItalic", 10.0, "Amounts in EUR", (0.0, 10.0, 60.0, 18.0)),
                    PdfLine::new("Arial", 10.0, "Acme Corp", (0.0, 20.0, 40.0, 30.0)),
                    PdfLine::new("Arial", 10.0, "-", (50.0, 20.0, 90.0, 30.0)),
                    PdfLine::new("Arial", 10.0, "-", (100.0, 20.0, 140.0, 30.0)),
                ],
                Vec::new(),
            )
        }

        fn market_value_and_perc(cell: &str) -> (BlockValue, BlockValue) {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,ArialBold,ArialItalic,Arial,1,,2,,\n"),
            );
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}A-EN24,,,,,,,{cell},\n"));
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").expect("pipelines"));
            let companies = CompanyMatchInfos::compile_from_target_companies(vec![TargetCompanyInput {
                name: "Acme Corp".to_string(),
                regexs: vec![],
                symbols: vec![],
                buds: vec![],
            }])
            .expect("a name without patterns always compiles");
            let blocks = pipeline
                .apply_text_filter(&page_with_a_dash(), &FilterData::TargetCompanies(&companies))
                .expect("the page filters");
            let row = blocks
                .iter()
                .find(|b| b.metadata.contains_key("market value"))
                .expect("one investment row");
            (
                row.metadata.get("market value").expect("market value").clone(),
                row.metadata.get("% net assets").expect("% net assets").clone(),
            )
        }

        /// The whole point of the column: the cell reaches the pipe the pipeline is built with, and
        /// only the field it names is affected.
        #[test]
        fn the_flags_reach_the_built_pipe_and_only_the_named_field() {
            assert_eq!(
                market_value_and_perc("MARKET_VALUE"),
                (BlockValue::from("0"), BlockValue::from("-"))
            );
        }

        #[test]
        fn without_the_column_the_same_page_keeps_both_dashes() {
            assert_eq!(
                market_value_and_perc(""),
                (BlockValue::from("-"), BlockValue::from("-"))
            );
        }
    }

    /// The `Note marks` column, from the CSV cell to the name the pipeline publishes (`R16`).
    mod note_marks_column {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The fund's name carries its footnote mark, exactly as CARNE-EN23 p39 and
        /// DANSKEINVEST-EN24 p127 print it.
        fn page_with_a_marked_fund() -> Page {
            Page::new(
                1,
                (300.0, 300.0),
                vec![
                    PdfLine::new("ArialBold", 10.0, "Alpha Fund*", (0.0, 0.0, 60.0, 10.0)),
                    PdfLine::new("ArialItalic", 10.0, "Amounts in EUR", (0.0, 10.0, 60.0, 18.0)),
                    PdfLine::new("Arial", 10.0, "Acme Corp", (0.0, 20.0, 40.0, 30.0)),
                    PdfLine::new("Arial", 10.0, "1.000", (50.0, 20.0, 90.0, 30.0)),
                ],
                Vec::new(),
            )
        }

        fn fund_name_with(cell: &str) -> BlockValue {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,ArialBold,ArialItalic,Arial,1,,,,\n"),
            );
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}A-EN24,,,,,,,,{cell}\n"));
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").expect("pipelines"));
            let companies = CompanyMatchInfos::compile_from_target_companies(vec![TargetCompanyInput {
                name: "Acme Corp".to_string(),
                regexs: vec![],
                symbols: vec![],
                buds: vec![],
            }])
            .expect("a name without patterns always compiles");
            let blocks = pipeline
                .apply_text_filter(&page_with_a_marked_fund(), &FilterData::TargetCompanies(&companies))
                .expect("the page filters");
            blocks
                .iter()
                .find_map(|b| b.metadata.get("fund").cloned())
                .expect("one investment row carrying the fund")
        }

        /// The default, and what the twenty-four formats that say nothing get: the behaviour there
        /// was before the column existed.
        #[test]
        fn an_empty_cell_leaves_the_mark_where_the_report_wrote_it() {
            assert_eq!(fund_name_with(""), BlockValue::from("Alpha Fund*"));
        }

        /// The whole point of the column: the cell reaches the pipe the pipeline is built with.
        #[test]
        fn the_declared_mark_reaches_the_built_pipe() {
            assert_eq!(fund_name_with("*"), BlockValue::from("Alpha Fund"));
        }

        /// A pipe with no row at all in `additional_args.csv` — the common case — must reach the
        /// same place as an empty cell, not a missing-field error.
        #[test]
        fn a_pipe_with_no_additional_row_strips_nothing() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            let configs = get_investments_configs(repo.path()).expect("the repo loads");
            assert!(configs[0].additional.is_none());
        }

        /// The column configures the `text_filter` segment, so declaring that segment off and then
        /// filling the column in is the contradiction `DISABLED_SEGMENT_COLUMNS` exists to catch.
        #[test]
        fn a_disabled_text_filter_may_not_carry_the_column() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}A-EN24,,,,,,,,*\n"));
            repo.write("investments/partial_pipes.csv", &format!("{PARTIAL_HEADER}A-EN24,,FALSE,\n"));
            assert!(get_investments_configs(repo.path()).is_err());
        }
    }

    /// The `Remove from fund` column: the `(continued)` of a continuation page, cut out of the
    /// name before it becomes the sub-fund's identity.
    mod remove_from_fund_column {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The header **with** the column. `ADD_HEADER` deliberately stays as it was — a file
        /// without the column has to keep loading, and `a_file_without_the_column_still_loads`
        /// is the test that says so.
        const ADD_HEADER_CUT: &str = "ID,Algorithm flags,Column tolerance,Row tolerance,Interpret quantity as float,Interpret cost and value as int,Geometrical indexing,Interpret dash as zero,Note marks,Remove from fund\n";

        /// FINECO-EN23@IR p538 writes the sub-fund's name with the word that says this is the rest
        /// of the table that began on an earlier page.
        fn page_with_a_continued_fund(fund: &str) -> Page {
            Page::new(
                1,
                (300.0, 300.0),
                vec![
                    PdfLine::new("ArialBold", 10.0, fund, (0.0, 0.0, 120.0, 10.0)),
                    PdfLine::new("ArialItalic", 10.0, "Amounts in EUR", (0.0, 10.0, 60.0, 18.0)),
                    PdfLine::new("Arial", 10.0, "Acme Corp", (0.0, 20.0, 40.0, 30.0)),
                    PdfLine::new("Arial", 10.0, "1.000", (50.0, 20.0, 90.0, 30.0)),
                ],
                Vec::new(),
            )
        }

        /// The name as the built pipeline publishes it, given the two cells and the printed name.
        fn fund_name_with(cut: &str, marks: &str, printed: &str) -> BlockValue {
            let repo = Repo::new();
            repo.write(
                "investments/args.csv",
                &format!("{ARGS_HEADER}A-EN24,ArialBold,ArialItalic,Arial,1,,,,\n"),
            );
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER_CUT}A-EN24,,,,,,,,{marks},\"{cut}\"\n"),
            );
            let pipeline = only_pipeline(get_pipelines(repo.path(), "A-EN24").expect("pipelines"));
            let companies = CompanyMatchInfos::compile_from_target_companies(vec![TargetCompanyInput {
                name: "Acme Corp".to_string(),
                regexs: vec![],
                symbols: vec![],
                buds: vec![],
            }])
            .expect("a name without patterns always compiles");
            let blocks = pipeline
                .apply_text_filter(
                    &page_with_a_continued_fund(printed),
                    &FilterData::TargetCompanies(&companies),
                )
                .expect("the page filters");
            blocks
                .iter()
                .find_map(|b| b.metadata.get("fund").cloned())
                .expect("one investment row carrying the fund")
        }

        /// What every format that says nothing gets: the behaviour there was before the column.
        #[test]
        fn an_empty_cell_cuts_nothing() {
            assert_eq!(
                fund_name_with("", "", "Alpha Fund (continued)"),
                BlockValue::from("Alpha Fund (continued)")
            );
        }

        /// The case the column was written for, and the reason it matters: without it this name
        /// and `Alpha Fund` are two rows of `funds.csv` for one sub-fund.
        #[test]
        fn the_declared_pattern_reaches_the_built_pipe() {
            assert_eq!(
                fund_name_with(r"\(continued\)", "", "Alpha Fund (continued)"),
                BlockValue::from("Alpha Fund")
            );
        }

        /// The space the cut leaves behind goes too. A trailing space is as good as a parenthesis
        /// at splitting one sub-fund into two.
        #[test]
        fn the_space_the_cut_leaves_behind_goes_with_it() {
            let name = fund_name_with(r"\(continued\)", "", "Alpha Fund (continued)");
            assert_eq!(name.as_str().expect("a textual name").find("  "), None);
        }

        /// A cut in the **middle** of the name must not weld the two halves together.
        #[test]
        fn a_cut_inside_the_name_leaves_one_space_not_none() {
            assert_eq!(
                fund_name_with(r"\(ex Beta\)", "", "Alpha (ex Beta) Fund"),
                BlockValue::from("Alpha Fund")
            );
        }

        /// An alternation is how one cell expresses several cuts — the reason there is no second
        /// column for a list.
        #[test]
        fn an_alternation_cuts_either_spelling() {
            assert_eq!(
                fund_name_with(r"\(continued\)|\(cont\.\)", "", "Alpha Fund (cont.)"),
                BlockValue::from("Alpha Fund")
            );
        }

        /// The two cleanings compose, and in this order: the footnote mark sits at the very end,
        /// so it survives whatever the pattern takes out of the middle.
        #[test]
        fn the_cut_and_the_footnote_mark_both_apply() {
            assert_eq!(
                fund_name_with(r"\(continued\)", "*", "Alpha Fund (continued)*"),
                BlockValue::from("Alpha Fund")
            );
        }

        /// A name that is nothing but the cut comes out empty rather than unchanged: it is not
        /// this column's business to decide what an empty name means.
        #[test]
        fn a_name_that_is_only_the_cut_comes_out_empty() {
            assert_eq!(fund_name_with(r"\(continued\)", "", "(continued)"), BlockValue::from(""));
        }

        /// A cell that is not a regex is refused when the pipeline is built, not when a page is
        /// read: a format whose configuration is wrong must fail loudly and at once.
        #[test]
        fn a_cell_that_is_not_a_regex_is_refused() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER_CUT}A-EN24,,,,,,,,,\"(unclosed\"\n"),
            );
            assert!(get_pipelines(repo.path(), "A-EN24").is_err());
        }

        /// A repository written before this column existed must keep loading: the column is
        /// absent, which is what an empty cell means.
        #[test]
        fn a_file_without_the_column_still_loads() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo.write("investments/additional_args.csv", &format!("{ADD_HEADER}A-EN24,,,,,,,,\n"));
            let configs = get_investments_configs(repo.path()).expect("the repo loads");
            assert_eq!(
                configs[0].additional.as_ref().and_then(|a| a.remove_from_fund.clone()),
                None
            );
        }

        /// The column configures the `text_filter`, so a disabled one may not carry it.
        #[test]
        fn a_disabled_text_filter_may_not_carry_the_column() {
            let repo = Repo::new();
            repo.write("investments/args.csv", &format!("{ARGS_HEADER}A-EN24,,,,1,,,,\n"));
            repo.write(
                "investments/additional_args.csv",
                &format!("{ADD_HEADER_CUT}A-EN24,,,,,,,,,,\"\\(continued\\)\"\n"),
            );
            repo.write("investments/partial_pipes.csv", &format!("{PARTIAL_HEADER}A-EN24,,FALSE,\n"));
            assert!(get_investments_configs(repo.path()).is_err());
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
