// Rust source layout mirrors `packages/freeports_core/src/freeports/_internals/`'s own split
// (`commons/`, `core/`, `output/`, `formats/utils/`, `formats/repo/`) rather than one flat module,
// so folders stay a handful of files each and a new output class doesn't have to be added next to
// unrelated engine plumbing:
// - `commons`: implementation-independent constants/i18n (mirrors `_internals/commons/`).
// - `core`: core domain concepts — normalization, fund matching, promises, date bridging,
//   casting, tracing setup (mirrors `_internals/core/`, plus a couple of infra modules that
//   don't have a Python-side file of their own yet).
// - `output`: the output/report layer (`_internals/output/`'s Rust port) — `output::classes` is
//   the objects themselves (`classes_schema.py`), `output::files_schema`/`output::routines` is
//   the accumulation/validation/IO built around them.
// - `formats_utils`: what format-author code is built out of — PDF selection/extraction, text
//   filtering, deserialization into output classes (mirrors `_internals/formats/utils/`).
// - `formats_repo`: the formats-repository acquisition layer — reading/validating a format
//   author's on-disk repo (`metadata/formats.csv`, `content/orchestration/*.csv`,
//   `content/algorithms/semistructured/formats_mapping.csv`), mirrors `_internals/formats/repo/`.
//   `formats_repo::id_format` is the shared ID-string regex/derivation layer
//   (`_internals/formats/repo/algorithms/pipelines_definition.py`'s regex section plus
//   `_internals/formats/repo/metadata.py`'s `FORMAT_NAME_REGEXP`) every other `formats_repo`
//   submodule builds on. See `agent-memory/detect-format-metadata-rust-port-implementation-plan.md`.
/// `cargo test --lib` runs `#[test]` functions in parallel OS threads sharing one embedded
/// interpreter (`pyo3`'s `auto-initialize`). CPython's import system isn't safe against two
/// threads independently triggering the *first* import of the same package tree at the same
/// time — one can observe the other's partially-initialized module and raise a spurious
/// `ImportError: ... partially initialized module ... (most likely due to a circular import)`.
/// This bit real tests once two modules (`pdf_extract::common`, `pdf_extract::position`) each did
/// their own first `py.import("freeports...")`. `ensure_freeports_imported`, called at the top of
/// any test that imports a `freeports` submodule, serializes that one first import behind a
/// `Once` so every later `py.import(...)` — from any thread — is a cheap `sys.modules` cache hit
/// instead of a race.
///
/// The `Once` wait itself must happen with the GIL released (`py.detach`), not held: a
/// naive `INIT.call_once(|| py.import(...))` deadlocked under `--test-threads` > 1 the first time
/// this was tried — `freeports`'s own imports (numpy/pandas/scikit-image/PyMuPDF, all real C
/// extensions) can transiently release and need to reacquire the GIL mid-import, but a second
/// thread parked on the plain OS-level `Once` futex while still holding the GIL (entering this
/// function requires a `Python<'_>` token, i.e. the GIL) blocks that reacquisition forever —
/// classic foreign-lock-inside-GIL-held-region deadlock, confirmed via bisection (hangs
/// specifically once ≥2 threads both reach this function concurrently; single-threaded and
/// single-caller cases were both fine, which is what let the naive version pass its first,
/// too-narrow manual check).
#[cfg(test)]
mod test_support {
    use pyo3::prelude::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    pub fn ensure_freeports_imported(py: Python<'_>) {
        py.detach(|| {
            INIT.call_once(|| {
                Python::attach(|py| {
                    py.import("freeports").expect("failed to import freeports package for tests");
                });
            });
        });
    }
}

pub mod commons {
    pub mod consts;
    pub mod i18n;
    pub mod flag_expr;
    // Merged in from the former `freeports_lib` crate (Fase E, punto 4) — geometry/set-relation
    // primitives `pdf_extract::select` builds on. See `agent-memory/rust-native-binary-plan.md`.
    pub mod geometry;
    pub mod sets;
}

pub mod core {
    pub mod normalization;
    pub mod match_fund;
    pub mod tracing_setup;
    pub mod promise;
    pub mod promisable;
    pub mod py_date;
    pub mod promise_resolution;
    pub mod classes;
}

pub mod output {
    // The output/report classes (`_internals/output/classes_schema.py`'s Rust port) — grouped in
    // their own subfolder, distinct from `files_schema.rs`/`routines.rs` (the accumulation/IO
    // layer around them, `_internals/output/{files_schema,routines}.py`'s ports), so it's obvious
    // at a glance which files in `output/` *are* output objects vs. which ones process them.
    pub mod classes {
        pub mod fund_change_name;
        pub mod assets_manager;
        pub mod fund;
        pub mod fund_sfdr_classification;
        pub mod fund_esg_indicator;
        pub mod fund_assets;
        pub mod investment;
    }
    pub mod files_schema;
    pub mod routines;
}

// Mirrors `_internals/formats/utils/{pdf_extract,text_filter,deserialize}/` — the utilities format
// authors' code is built out of (selecting/extracting PDF content, filtering it, deserializing it
// into output classes), grouped together as a distinct theme from `core`/`output`/`pipeline`.
pub mod formats_utils {
    pub mod deserialize {
        pub mod cast;
        pub mod standard_funcs;
    }

    pub mod pdf_extract {
        pub mod common;
        pub mod position;
        pub mod standard_funcs;
        // Merged in from the former `freeports_lib` crate (Fase E, punto 4).
        pub mod tabularizer;
        pub mod select;
    }

    pub mod text_filter {
        pub mod standard_funcs;
        // Merged in from the former `freeports_lib` crate (Fase E, punto 4) — `CompanyMatchInfos`
        // and its matching logic; was the whole reason Fase D needed a `py.import("freeports_lib")`
        // cross-module workaround instead of a native call (see `input/companies_db.rs`'s doc
        // comment) — now unnecessary, everything is the same compiled module.
        pub mod matcher;
    }
}

// Mirrors `_internals/formats/repo/` — the formats-repository acquisition layer (see the
// top-of-file module doc comment above). Declared inline here, not via an out-of-line
// `pub mod formats_repo;` pointing at a `formats_repo/mod.rs` file, matching every other
// multi-file module group in this crate (`output`, `formats_utils`, `cli`) — none of them has a
// `mod.rs` either; Rust resolves `id_format`'s path to `formats_repo/id_format.rs` off of this
// inline module's own name.
pub mod formats_repo {
    pub mod id_format;
    pub mod metadata;
    pub mod orchestration;
    // Mirrors `_internals/formats/repo/algorithms/semistructured/` (Milestone 2, sequencing item
    // 3). Unlike every other multi-file module group in this crate (`formats_repo` itself,
    // `output`, `formats_utils`, `cli` — all declared inline, with braces, directly in this file),
    // `semistructured` is declared **out-of-line** here (`pub mod semistructured;`, no braces)
    // because the module needs its own file to hold real content of its own —
    // `SegmentKind`/`AlgorithmSource`/`SemistructuredError`/`resolve`/`get_pipelines`, the
    // dispatch layer itself, replacing `algorithms/semistructured/acquisition.py`'s `get_pipelines`
    // — in addition to declaring its two child submodules (`formats_mapping`, `native`). An inline
    // `pub mod semistructured { pub mod formats_mapping; ... }` block, as used before this step,
    // has nowhere to put that content except directly inside this file, which would make `lib.rs`
    // carry business logic rather than just module wiring. `pipeline/mod.rs` (`pub mod pipeline;`
    // just below) is this crate's one existing precedent for exactly this shape — a module with
    // its own file for real content, declared out-of-line. Resolves to
    // `formats_repo/semistructured/mod.rs` per Rust's own file-resolution rule for an out-of-line
    // `pub mod x;` nested inside an already-inline parent module block.
    pub mod semistructured;
}

pub mod pipeline;

// Mirrors `_internals/cli/` — CLI argument/config parsing, ported incrementally (Fase E, see
// `agent-memory/rust-native-binary-plan.md`). `conf_parse` starts with `DocumentSpec` only; the
// config-source precedence chain and `FreeportsConfig` follow.
pub mod cli {
    pub mod conf_parse;
    pub mod partial_config;
    pub mod file_config;
    pub mod env_config;
    pub mod cmd_config;
    pub mod job_config;
    pub mod freeports_config;
    pub mod cmd;

    // `job`/`batch`/`output` are the `freeports` binary's own modules (`src/main.rs`), not part of
    // format-author code's public surface — mirroring `_internals/cli/main.py`, which holds this
    // exact job-dispatch/batch/output-writing logic next to `conf_parse.py`/`cmd.py` in the same
    // `_internals/cli/` folder. Declared `pub` here (rather than duplicated into the binary's own
    // module tree via a `#[path]` attribute) because the binary is a second compiled crate that
    // reaches this one only through its normal public dependency surface, same as `cmd`/
    // `freeports_config` above.
    pub mod job;
    pub mod batch;
    pub mod output;
    // The whole CLI run (`CliArgs` in, output written) built out of the four modules above plus
    // `cmd`/`freeports_config` — the two testable pieces the `freeports` binary's `src/main.rs`
    // used to contain directly (moved here so `cargo test --lib` can reach them).
    pub mod run;
}

pub mod input {
    pub mod download;
    pub mod companies_db;
}

/// Incremental Rust rewrite of `freeports_core`. See
/// `analysis_finance_reports/agent-memory/rust-rewrite-plan.md` for the migration plan this
/// module is the first slice of. The name of this function must match the `lib.name` setting
/// in `Cargo.toml`, else Python will not be able to import the module.
#[pyo3::pymodule]
mod freeports_engine {
    // `Currency`/`SfdrArticle`/`FinancialInstrument`, and every `output` class, are exported at
    // THIS top level, not nested under `core` like the rest — deliberately, so their
    // `module = "freeports_engine"` (see e.g. commons/consts.rs) resolves via plain
    // `importlib.import_module("freeports_engine")` + `getattr(mod, "X")`.
    // `core/serialization.py`'s tag-based (de)serialization (`_enum_to_tag`/`_tag_to_enum`, and
    // the `__rust_model_fields__`-based extension for output classes) needs that generic
    // importlib round-trip to work; nested PyO3 pymodules like `freeports_engine.core` are
    // attributes, not real entries in `sys.modules`, so
    // `importlib.import_module("freeports_engine.core")` fails (`ModuleNotFoundError:
    // 'freeports_engine' is not a package`) even though plain attribute access
    // (`freeports_engine.core.whatever`) works fine.
    #[pymodule_export]
    use crate::commons::consts::{Currency, SfdrArticle, FinancialInstrument};
    #[pymodule_export]
    use crate::output::classes::fund_change_name::{FundRename, FundMerge};
    #[pymodule_export]
    use crate::output::classes::assets_manager::{ManagementCompany, InvestmentsManager};
    #[pymodule_export]
    use crate::output::classes::fund::Fund;
    #[pymodule_export]
    use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
    #[pymodule_export]
    use crate::output::classes::fund_esg_indicator::FundEsgIndicator;
    #[pymodule_export]
    use crate::output::classes::fund_assets::FundAssets;
    #[pymodule_export]
    use crate::output::classes::investment::{Equity, Bond};

    #[pyo3::pymodule]
    mod core {
        #[pymodule_export]
        use crate::core::normalization::{
            py_deep_normalize_string,
            py_normalize_string,
            py_normalize_word,
        };
        #[pymodule_export]
        use crate::core::match_fund::MatchFund;
        #[pymodule_export]
        use crate::commons::flag_expr::py_evaluate_flag_expression;
        #[pymodule_export]
        use crate::core::tracing_setup::py_init_tracing;
        #[pymodule_export]
        use crate::commons::i18n::Translator;
        #[pymodule_export]
        use crate::core::promise::Promise;
        #[pymodule_export]
        use crate::core::promise_resolution::{
            CircularPromisesChain,
            py_build_promise_multimap,
            py_merge_into_multimap,
            py_flatten_promise_map,
        };
        #[pymodule_export]
        use crate::core::classes::{
            PdfBlock,
            TextBlock,
            ExpectedPdfBlockNotFound,
            ExpectedTextBlockNotFound,
            PageParseFail,
            LineParseFail,
            ExtractionFieldFail,
        };
        #[pymodule_export]
        use crate::formats_utils::deserialize::cast::{
            py_to_float,
            py_to_int,
            py_to_str,
            py_to_currency,
            py_perc_to_float,
            py_to_date,
            py_to_date_with_en_month,
            py_to_date_with_it_month,
            py_to_int_en_month,
            py_to_int_it_month,
            py_is_numeric_shape,
        };
        #[pymodule_export]
        use crate::formats_utils::deserialize::standard_funcs::{
            DeserializeSfdrArticleStandard,
            DeserializerPageClassifyStandard,
            DeserializerFundStandard,
            DeserializerManagmentCompanyStandard,
            DeserializerInvestmentsManagerFromManco,
            DeserializerInvestmentsManagerStandard,
        };
        #[pymodule_export]
        use crate::formats_utils::pdf_extract::common::{SelectExpectedText, ExtractTextPdfBlockOrFailPage};
        #[pymodule_export]
        use crate::formats_utils::pdf_extract::position::{InputArea, CellGeometry, RowConfig, ColumnConfig, TableConfig, py_get_groups};
        #[pymodule_export]
        use crate::formats_utils::pdf_extract::standard_funcs::{
            PdfExtractSfdrArticleStandard,
            PdfExtractCurrencyConstant,
            PdfExtractPageClassifyStandard,
            PdfExtractInvestmentsStandard,
            PdfExtractAssetsStandard,
        };
        #[pymodule_export]
        use crate::formats_utils::pdf_extract::tabularizer::{py_get_table_coordinates, py_collapse_table_rows};
        #[pymodule_export]
        use crate::formats_utils::pdf_extract::select::{PyPdfLineSelection, PyPdfLine};
        #[pymodule_export]
        use crate::formats_utils::text_filter::standard_funcs::{
            FilterBlockType,
            FilterBlockTypeApplied,
            FilterBlockTypes,
            FilterBlockTypesApplied,
            FilterBlockTypeCall,
            FilterBlockTypeCallApplied,
            FilterBlockTypesCall,
            FilterBlockTypesCallApplied,
            FundFilterData,
            FundFilterDataCall,
            InvestmentFundFilterData,
            InvestmentFundFilterDataCall,
            py_extract_currency_from_text,
            TextFilterSfdrArticleStandard,
            TextFilterPageClassifyStandard,
            TextFilterManagmentCompanyStandard,
            TextFilterAssetsStandard,
            TextFilterInvestmentsStandard,
        };
        #[pymodule_export]
        use crate::formats_utils::text_filter::matcher::{py_match_company, CompanyMatchInfos};
        #[pymodule_export]
        use crate::pipeline::{
            PdfExtractSegment,
            TextFilterSegment,
            DeserializeSegment,
            Pipeline,
            PipelinesBundle,
            Algorithm,
        };
        #[pymodule_export]
        use crate::output::routines::{
            PageResults,
            DocumentResults,
            TransformedTables,
            py_transform_to_files_schema,
            py_write_files,
        };
        #[pymodule_export]
        use crate::input::download::py_download_pdf;
        #[pymodule_export]
        use crate::input::companies_db::py_get_target_companies;
        #[pymodule_export]
        use crate::formats_repo::metadata::{py_get_formats, py_url_to_format};
        #[pymodule_export]
        use crate::formats_repo::semistructured::py_get_semistructured_pipelines;
    }
}
