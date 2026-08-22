#[cfg(test)]
mod test_support {
    use pyo3::prelude::*;
    use pyo3::wrap_pymodule;
    use std::sync::Once;

    static INIT: Once = Once::new();

    pub fn ensure_freeports_imported(py: Python<'_>) {
        py.detach(|| {
            INIT.call_once(|| {
                Python::attach(|py| {
                    seed_native_module(py);
                    py.import("freeports").expect("failed to import freeports package for tests");
                });
            });
        });
    }

    /// Pre-seeds `sys.modules["freeports._native"]` (and its `core`/`cli` submodules) with *this
    /// test binary's own* statically-linked compiled pymodule, before `import freeports` (which
    /// `freeports/__init__.py` — real Python — immediately follows with `from freeports import
    /// _native`) ever runs.
    ///
    /// Without this, that `from freeports import _native` would resolve to the separately-built,
    /// on-disk `_native.abi3.so` cdylib — a *different compiled artifact* than this test binary,
    /// even though both come from identical Rust source. PyO3 registers a `#[pyclass]`'s Python
    /// type per compiled artifact (the same "cross-module identity trap" already documented
    /// elsewhere in this crate, e.g. `formats_utils/text_filter/matcher.rs`'s `CompanyMatchInfos`
    /// history) — empirically confirmed here too: a `DocumentResults` built by
    /// `pipeline::Algorithm::run_documents` while `Algorithm` was obtained via the on-disk cdylib
    /// (dlopen'd through a bare `py.import("freeports._native")`) failed `.cast::<DocumentResults>()`
    /// from this same test binary's own `output::routines::transform_to_files_schema`, with exactly
    /// the `'DocumentResults' object cannot be cast as 'DocumentResults'` `TypeError` that trap
    /// always produces.
    ///
    /// Python checks `sys.modules` before touching the filesystem/dlopen path, so seeding it here
    /// makes every `py.import("freeports._native")` anywhere in this process (this crate's own
    /// `cli::job`/`pipeline` code, and any Python — including format fixtures — that does `from
    /// freeports import _native`) resolve to *this* binary's own type registry consistently, for
    /// the lifetime of the process. Only needed for `#[cfg(test)]`: the real, separately-shipped
    /// `freeports` CLI binary has this exact same trap for its own reasons (see
    /// `agent-memory/pytest-plugin-rust-swap-implementation-plan.md`'s notes) — fixing that one is
    /// a deliberately deferred, separate task, not something this test-only seed touches.
    fn seed_native_module(py: Python<'_>) {
        let native = wrap_pymodule!(crate::_native)(py).into_bound(py);
        native.setattr("__name__", "freeports._native").expect("set __name__ on the seeded native module");
        let sys_modules = py.import("sys").expect("import sys").getattr("modules").expect("sys.modules");
        sys_modules.set_item("freeports._native", &native).expect("seed freeports._native");
        for sub in ["core", "cli"] {
            if let Ok(submodule) = native.getattr(sub) {
                sys_modules.set_item(format!("freeports._native.{sub}"), submodule).expect("seed freeports._native submodule");
            }
        }
    }
}

pub mod pyerr;

pub mod commons {
    pub mod consts;
    pub(crate) mod i18n;
    pub mod flag_expr;
    pub mod geometry;
    pub mod sets;
}

pub mod core {
    pub mod normalization;
    pub(crate) mod match_fund;
    pub(crate) mod tracing_setup;
    pub mod promise;
    pub mod promisable;
    pub mod py_date;
    pub mod promise_resolution;
    pub mod classes;
    pub use classes::{PdfBlock,TextBlock};
}

pub mod output {
    mod classes;
    pub use classes::*;
    mod files_schema;
    pub mod routines;
}

pub mod formats_utils {
    pub mod deserialize {
        pub mod cast;
        pub mod standard_funcs;
    }

    pub mod pdf_extract {
        pub mod common;
        pub(crate) mod position;
        pub mod standard_funcs;
        pub mod tabularizer;
        pub(crate) mod select;
    }

    pub mod text_filter {
        pub mod standard_funcs;
        pub mod matcher;
        // Fase-5 Module 1 port of `standard_txt_blks.py` -- see that file's own module doc for
        // the design. `OneTextBlockType`/`ResultStandardFiltering` are exported top-level below
        // (matching their own `module = "freeports._native"` pyclass attribute, same placement
        // as `FinancialInstrument`/`SfdrArticle`); the 6 `standard_*_text_block*` functions are
        // exported under the nested `core` pymodule (matching the plan's own
        // `_native.core.standard_fund_text_block(...)` illustrative call).
        pub mod standard_txt_blks;
    }
}


pub mod formats_repo {
    mod id_format;
    pub mod metadata;
    pub mod orchestration;
    pub(crate) mod semistructured;
}

mod pipeline;


pub mod cli {
    pub(crate) mod conf_parse;
    mod partial_config;
    mod config_locations;
    mod freeports_config;
    mod cmd;
    mod job;
    mod batch;
    mod output;
    mod run;
    // Declared here (test-writer phase, pytest-plugin-rust-swap) purely so `cargo test --lib`
    // actually compiles and runs `py_run_job.rs`'s own `#[cfg(test)] mod tests` -- an undeclared
    // `.rs` file is simply invisible to the compiler, which would make its tests silently absent
    // rather than failing/red. The `pub use py_run_job::py_run_job;` re-export (needed for the
    // `#[pymodule_export]` in the `cli` nested pymodule below) and that pymodule addition itself
    // are `implementer`'s job per the implementation plan's File 1/File 5 -- not added here.
    pub(crate) mod py_run_job;
    pub use config_locations::cmd::CliArgs;
    pub use run::execute;

}

mod input {
    pub mod download;
    pub(crate) mod companies_db;
}


/// Named `_native` (not `freeports`, despite `Cargo.toml`'s `[lib] name = "freeports"`): imported
/// from Python as `freeports._native`, a private submodule of the pure-Python `freeports` package
/// (`python/freeports/__init__.py`). The `#[pyo3::pymodule]` macro derives this module's C-level
/// `PyInit_*` init symbol from its own Rust name, which must match the Python-facing module name
/// for Python's import machinery to load it correctly — confirmed empirically both ways: named
/// `freeports` (matching only the Cargo crate name), `maturin develop` places the compiled
/// artifact at `python/freeports/freeports.abi3.so` (importable only as the awkward
/// `freeports.freeports.*`, since `python/freeports/__init__.py` already owns the bare `freeports`
/// name as a real pure-Python package); named `_native`, it lands at
/// `python/freeports/_native.abi3.so` and imports exactly as `freeports._native.*`, matching every
/// existing `python/freeports/_internals/**/*.py` reference. Collapsing this indirection so the
/// compiled module can be bare `freeports` requires first retiring the pure-Python
/// `python/freeports/__init__.py` package (still live — `_internals/**` has real, not-yet-ported
/// Python logic) — a separate, larger task, not done here.
#[pyo3::pymodule]
mod _native {
    #[pymodule_export]
    use crate::commons::consts::{Currency, SfdrArticle, FinancialInstrument};
    #[pymodule_export]
    use crate::formats_utils::text_filter::standard_txt_blks::{OneTextBlockType, ResultStandardFiltering};
    #[pymodule_export]
    use crate::output::fund_change_name::{FundRename, FundMerge};
    #[pymodule_export]
    use crate::output::assets_manager::{ManagementCompany, InvestmentsManager};
    #[pymodule_export]
    use crate::output::fund::Fund;
    #[pymodule_export]
    use crate::output::fund_sfdr_classification::FundSfdrClassification;
    #[pymodule_export]
    use crate::output::fund_esg_indicator::FundEsgIndicator;
    #[pymodule_export]
    use crate::output::fund_assets::FundAssets;
    #[pymodule_export]
    use crate::output::investment::{Equity, Bond};

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
        use crate::formats_utils::text_filter::standard_txt_blks::{
            standard_fund_text_block,
            standard_fund_text_block_from_content,
            standard_management_company_text_block,
            standard_management_company_text_block_from_content,
            standard_investments_manager_text_block,
            standard_investments_manager_text_block_from_content,
        };
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
        use crate::input::download::py_download_pdf;
        #[pymodule_export]
        use crate::input::companies_db::py_get_target_companies;
        #[pymodule_export]
        use crate::formats_repo::metadata::{py_get_formats, py_url_to_format};
        #[pymodule_export]
        use crate::formats_repo::semistructured::py_get_semistructured_pipelines;
        // `PageResults`/`DocumentResults`/`TransformedTables`/`transform_to_files_schema`/
        // `write_files` are deliberately NOT exported here (unlike the pre-restore snapshot this
        // block was restored from): their only Python-facing caller anywhere in the workspace was
        // `python/freeports/_internals/cli/main.py` (confirmed by grep — nothing else references
        // any of the five), which this same task deletes outright (see
        // `agent-memory/pytest-plugin-rust-swap-implementation-plan.md`, File 6). `write_files`
        // and `transform_to_files_schema` also stopped being `#[pyfunction]`-shaped in the same
        // session that dropped `TransformedTables`'s `#[pyclass]` (see `output/routines.rs`'s own
        // doc comments) — both are called natively from `cli::output::write_results` now, so
        // exporting them would mean re-adding PyO3 surface nothing needs, the exact anti-pattern
        // this crate has been deliberately removing elsewhere.
    }

    // Bridge for `freeports_dev`'s `pytest_plugin.py` — see
    // `agent-memory/pytest-plugin-rust-swap-implementation-plan.md`, File 1/File 2. Kept as its
    // own nested pymodule rather than folded into `core` above: `core` mirrors this crate's own
    // `core::*`/pipeline-mechanics module tree, while `run_job` is conceptually a `cli::*` item
    // (job resolution/config/orchestration), matching this crate's own internal split.
    #[pyo3::pymodule]
    mod cli {
        #[pymodule_export]
        use crate::cli::py_run_job::py_run_job;
    }
}
