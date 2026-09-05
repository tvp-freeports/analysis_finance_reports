//! The Python binding layer: `import freeports` from Python arrives here.
//!
//! # What it is, and what it is not
//!
//! This tree is **shim only**: newtypes wrapping the crate's types, and functions converting Python
//! arguments into Rust ones and calling the native routines. No domain logic lives here. That is
//! why nothing elsewhere in the crate is annotated with PyO3 attributes — the boundary is meant to
//! be one place, and this is it.
//!
//! # No private layers underneath
//!
//! The Python package **is** the compiled extension: `freeports` imported from Python is this
//! module, not a pure-Python package re-exporting its symbols. The two private layers an earlier
//! design interposed are gone — the first because the indirection buys nothing when there is no
//! Python code to hide, the second because that implementation is now in Rust.
//!
//! # Submodules and the import machinery
//!
//! `from freeports.core import PdfBlock` asks Python to import the **module** `freeports.core`. A
//! compiled extension is not a package: it has no directory for Python to fetch a submodule from,
//! so the only way that import can work is to register every nested submodule under its full dotted
//! name. `register_submodules` does that recursively at initialisation, so that even two levels
//! down works with no special case.

pub mod api;
pub mod consts;
pub mod convert;
pub mod core;
pub mod input;
pub mod interfaces;
pub mod pipes;
pub mod output;
pub mod standard_funcs;
pub mod utils;

use pyo3::prelude::*;

/// Recursively registers every submodule of `module` under its full dotted name, and gives each its
/// own `__name__`.
///
/// Without the correct name, pickling and error messages would show the short name the macro built
/// the submodule with, rather than the dotted one.
fn register_submodules(py: Python<'_>, module: &Bound<'_, PyModule>, prefix: &str) -> PyResult<()> {
    let sys_modules = py.import("sys")?.getattr("modules")?;
    let contents: Vec<(String, Py<PyAny>)> = module
        .dict()
        .iter()
        .map(|(key, value)| PyResult::Ok((key.extract::<String>()?, value.unbind())))
        .collect::<PyResult<Vec<_>>>()?;

    for (name, value) in contents {
        let Ok(submodule) = value.into_bound(py).cast_into::<PyModule>() else { continue };
        let dotted = format!("{prefix}.{name}");
        submodule.setattr("__name__", &dotted)?;
        sys_modules.set_item(&dotted, &submodule)?;
        register_submodules(py, &submodule, &dotted)?;
    }
    Ok(())
}

/// Registers the `freeports` compiled **into this artefact**, if there is not one already.
///
/// # The trap this disarms
///
/// PyO3 registers a class's Python type **per compiled artefact**. This crate is compiled twice —
/// as the shared library Python imports, and as a library inside the binary and inside the test
/// binaries — so there are two distinct `PdfBlock` types, and they do not recognise each other.
///
/// When the binary is running, a format's author module does `from freeports.core import PdfBlock`
/// and Python hands it the class of the **installed** extension, while the block the engine passes
/// it comes from the library inside the binary. The result is the memorable `'PdfBlock' object
/// cannot be cast as 'PdfBlock'`.
///
/// Seeding the module table *before* any author module is imported makes `freeports` resolve to
/// this artefact's module, and the two sides coincide again.
///
/// **Idempotent, and never overwrites.** If `freeports` is already registered we are inside the
/// extension — Python imported it to get here — and that is already the right artefact: replacing
/// it would recreate the mismatch the other way round.
pub fn install(py: Python<'_>) -> PyResult<()> {
    let sys_modules = py.import("sys")?.getattr("modules")?;
    if sys_modules.contains("freeports")? {
        return Ok(());
    }
    // The qualified path, not the bare name: in doc-tests the crate itself is passed to rustdoc
    // under its own name, and the bare form becomes ambiguous between the crate and the module
    // below. The qualified path says which, and holds in every context.
    let module = pyo3::wrap_pymodule!(self::freeports)(py).into_bound(py).cast_into::<PyModule>()?;
    module.setattr("__name__", "freeports")?;
    sys_modules.set_item("freeports", &module)?;
    register_submodules(py, &module, "freeports")?;
    Ok(())
}

/// The Python module `freeports`.
///
/// The Rust module's name must be exactly this: the macro derives the C initialisation symbol from
/// it, and that symbol is what Python's import machinery looks for in the shared library.
#[pyo3::pymodule]
pub mod freeports {
    use super::*;

    #[pyo3::pymodule]
    mod consts {
        #[pymodule_export]
        use crate::python::consts::{PyCurrency, PyFinancialInstrument, PySfdrArticle};
    }

    #[pyo3::pymodule]
    mod core {
        #[pymodule_export]
        use crate::python::api::PyAlgorithm;
        #[pymodule_export]
        use crate::python::core::{PageParseFail, PyPdfBlock, PyPipeline, PyPromise, PyTextBlock};
    }

    #[pyo3::pymodule]
    mod cli {
        #[pymodule_export]
        use crate::python::api::{PyFreeportsFileConfig, py_run_job};
    }

    #[pyo3::pymodule]
    mod formats_repo {
        #[pymodule_export]
        use crate::python::api::{py_get_formats, py_url_to_format};
    }

    #[pyo3::pymodule]
    mod input {
        #[pymodule_export]
        use crate::python::input::{
            PyCompanyMatchInfos, py_get_target_companies, py_load_target_companies,
        };
    }

    #[pyo3::pymodule]
    mod interfaces {
        #[pyo3::pymodule]
        mod pdf_blks {
            #[pymodule_export]
            use crate::python::interfaces::{PyOnePdfBlockType, PyResultStandardExtraction};
        }

        #[pyo3::pymodule]
        mod text_blks {
            #[pymodule_export]
            use crate::python::interfaces::{
                PyOneTextBlockType, PyResultStandardFiltering, PyStandardFundTextBlock,
                PyStandardInvestmentsMangerTextBlock, PyStandardManagmentCompanyTextBlock,
            };
        }
    }

    #[pyo3::pymodule]
    mod utils {
        #[pyo3::pymodule]
        mod pdf_extract {
            #[pymodule_export]
            use crate::python::standard_funcs::pdf_extract::py_extract_text_pdf_block_or_fail_page;
            #[pymodule_export]
            use crate::python::utils::pdf_extract::{
                PyCollapseAlgorithm, PyColumnConfig, PyLimits, PyPageImage, PyPdfLine,
                PyPdfLineSelection, PyPdfLineSet, PyRowConfig, PySplittingState, PyTableConfig,
                PyTablePosAlgorithm, PyTablePosMeasureUnit, py_get_groups, py_get_table_coordinates,
                py_pdfimages_from_pagedict, py_pdfline_selection_from_dict,
                py_pdfline_selection_from_str, py_pdflines_from_pagedict,
            };
        }

        #[pyo3::pymodule]
        mod text_filter {
            #[pymodule_export]
            use crate::python::utils::text_filter::{
                PyFundFilterData, PyInvestmentFundFilterData, PyMatchFund,
                py_deep_normalize_string, py_extract_currency_from_text, py_normalize_string,
                py_normalize_word,
            };
        }

        #[pyo3::pymodule]
        mod deserialize {
            #[pymodule_export]
            use crate::python::utils::deserialize::{
                PyDeserializeBlockType, PyDeserializeBlockTypes, py_is_numeric_shape,
                py_perc_to_float, py_to_currency, py_to_date, py_to_date_with_en_month,
                py_to_date_with_it_month, py_to_float, py_to_int, py_to_int_en_month,
                py_to_int_it_month, py_to_str,
            };
        }
    }

    #[pyo3::pymodule]
    mod standard_funcs {
        #[pyo3::pymodule]
        mod pdf_extract {
            #[pymodule_export]
            use crate::python::standard_funcs::pdf_extract::{
                py_pdf_extract_assets_standard, py_pdf_extract_currency_constant,
                py_pdf_extract_currency_standard, py_pdf_extract_fund_standard,
                py_pdf_extract_investments_standard, py_pdf_extract_managment_company_standard,
                py_pdf_extract_page_classify_standard, py_pdf_extract_sfdr_article_standard,
            };
        }

        #[pyo3::pymodule]
        mod text_filter {
            #[pymodule_export]
            use crate::python::standard_funcs::text_filter::{
                py_text_filter_assets_standard, py_text_filter_investments_standard,
                py_text_filter_managment_company_standard, py_text_filter_page_classify_standard,
                py_text_filter_sfdr_article_standard,
            };
        }

        #[pyo3::pymodule]
        mod deserialize {
            #[pymodule_export]
            use crate::python::standard_funcs::deserialize::{
                py_deserialize_sfdr_article_standard, py_deserializer_assets_standard,
                py_deserializer_fund_standard, py_deserializer_investment_standard,
                py_deserializer_investments_manager_from_manco,
                py_deserializer_investments_manager_standard,
                py_deserializer_managment_company_standard, py_deserializer_page_classify_standard,
            };
        }
    }

    #[pyo3::pymodule]
    mod output {
        #[pymodule_export]
        use crate::python::output::{
            PyBond, PyEquity, PyFund, PyFundAssets, PyFundEsgIndicator, PyFundMerge, PyFundRename,
            PyFundSfdrClassification, PyInvestmentsManager, PyManagementCompany,
        };
    }

    #[pymodule_init]
    fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
        let py = module.py();
        register_submodules(py, module, "freeports")?;
        crate::python::consts::init(&module.getattr("consts")?.cast_into::<PyModule>()?)?;
        crate::python::output::init(&module.getattr("output")?.cast_into::<PyModule>()?)?;
        let interfaces = module.getattr("interfaces")?.cast_into::<PyModule>()?;
        crate::python::interfaces::init(
            &interfaces.getattr("pdf_blks")?.cast_into::<PyModule>()?,
            &interfaces.getattr("text_blks")?.cast_into::<PyModule>()?,
        )?;
        let utils = module.getattr("utils")?.cast_into::<PyModule>()?;
        crate::python::utils::pdf_extract::init(&utils.getattr("pdf_extract")?.cast_into::<PyModule>()?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Injects the `freeports` compiled **into this test binary** into the module table.
    ///
    /// Needed because a test run does not go through an installed extension: without it, importing
    /// `freeports` would look for the package on the filesystem and find — or fail to find — an
    /// artefact other than the one under test. It is the same cross-artefact identity trap
    /// [`install`] disarms.
    fn seed(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
        install(py)?;
        py.import("sys")?.getattr("modules")?.get_item("freeports")?.cast_into::<PyModule>().map_err(PyErr::from)
    }

    /// Runs `code` with `freeports` already seeded, returning the Python error if there is one.
    fn run(code: &str) -> PyResult<()> {
        Python::attach(|py| {
            seed(py)?;
            py.run(&std::ffi::CString::new(code).unwrap(), None, None)
        })
    }

    mod module_layout {
        use super::*;

        #[test]
        fn the_package_is_importable_under_its_own_name() {
            run("import freeports").unwrap();
        }

        #[test]
        fn nested_submodules_are_importable_with_a_dotted_path() {
            run("from freeports.core import PdfBlock, TextBlock, Promise").unwrap();
            run("from freeports.consts import Currency, SfdrArticle, FinancialInstrument").unwrap();
        }

        #[test]
        fn a_submodule_knows_its_own_dotted_name() {
            run("import freeports.core; assert freeports.core.__name__ == 'freeports.core', freeports.core.__name__").unwrap();
        }

        /// The two private layers of the earlier package must not exist any more, in any form.
        #[test]
        fn there_is_no_native_and_no_internals_submodule() {
            run("import freeports; assert not hasattr(freeports, '_native'), 'freeports._native must not exist'").unwrap();
            run("import freeports; assert not hasattr(freeports, '_internals'), 'freeports._internals must not exist'").unwrap();
        }
    }

    mod consts_shim {
        use super::*;

        /// The count is taken from the native list rather than written twice: a literal here would
        /// have to be edited every time a currency is added, and would be wrong in the meantime.
        #[test]
        fn every_currency_is_reachable_as_a_class_attribute() {
            run("from freeports.consts import Currency; assert Currency.EUR.name == 'EUR'").unwrap();
            run("from freeports.consts import Currency; assert Currency.NGN.name == 'NGN'").unwrap();
            let expected = crate::commons::consts::Currency::variants().len();
            run(&format!(
                "from freeports.consts import Currency; assert len(Currency.__members__) == {expected}"
            ))
            .unwrap();
        }

        #[test]
        fn the_other_two_enums_expose_their_variants_too() {
            run("from freeports.consts import SfdrArticle; assert SfdrArticle.ART_8.name == 'ART_8'").unwrap();
            run("from freeports.consts import FinancialInstrument as F; assert F.BOND.name == 'BOND'").unwrap();
        }

        #[test]
        fn the_same_variant_is_equal_to_itself_and_hashable() {
            run("from freeports.consts import Currency; assert Currency.EUR == Currency.EUR").unwrap();
            run("from freeports.consts import Currency; assert len({Currency.EUR, Currency.EUR, Currency.USD}) == 2").unwrap();
        }

        #[test]
        fn a_variant_is_reachable_by_name_too() {
            run("from freeports.consts import Currency; assert Currency['GBP'] == Currency.GBP").unwrap();
        }
    }

    mod output_shims {
        use super::*;

        #[test]
        fn a_fund_uppercases_its_normalized_name() {
            run("from freeports.output import Fund; assert Fund('Alpha  Fund').name == 'ALPHA FUND', Fund('Alpha  Fund').name").unwrap();
        }

        #[test]
        fn a_fund_built_from_a_promise_reports_the_promise_as_its_name() {
            run(
                "from freeports.output import Fund\n\
                 from freeports.core import Promise\n\
                 f = Fund(Promise('fund-name'))\n\
                 assert f.name == Promise('fund-name'), f.name",
            )
            .unwrap();
        }

        /// The three aliases are tuples of types rather than base classes: that is what
        /// `isinstance` needs, and what a formats repository uses.
        #[test]
        fn the_three_aliases_are_usable_as_isinstance_targets() {
            run(
                "from freeports.output import Investment, Equity, Bond, Fund\n\
                 from freeports.consts import Currency\n\
                 e = Equity(company='ACME', company_match='acme', fund='F', market_value=10.0, currency=Currency.EUR)\n\
                 assert isinstance(e, Investment)\n\
                 assert not isinstance(Fund('x'), Investment)",
            )
            .unwrap();
        }

        #[test]
        fn an_equity_exposes_the_fields_it_was_built_with() {
            run(
                "from freeports.output import Equity\n\
                 from freeports.consts import Currency\n\
                 e = Equity(company='ACME', company_match='acme', fund='Alpha', market_value=1234.5, currency=Currency.EUR)\n\
                 assert e.company == 'ACME'\n\
                 assert e.fund == 'Alpha'\n\
                 assert e.market_value == 1234.5\n\
                 assert e.currency == Currency.EUR, e.currency",
            )
            .unwrap();
        }

        #[test]
        fn a_bond_carries_its_maturity_and_interest_rate() {
            run(
                "import datetime\n\
                 from freeports.output import Bond\n\
                 from freeports.consts import Currency\n\
                 b = Bond(company='ACME', company_match='acme', fund='Alpha', market_value=1.0,\n\
                 \x20        currency=Currency.EUR, maturity=datetime.date(2030, 6, 1), interest_rate=0.05)\n\
                 assert b.maturity == datetime.date(2030, 6, 1), b.maturity\n\
                 assert b.interest_rate == 0.05",
            )
            .unwrap();
        }

        /// The accounting constraint is checked in Rust: the shim must surface it as a
        /// `ValueError`, not as a panic.
        #[test]
        fn an_unbalanced_fund_assets_is_a_value_error_not_a_panic() {
            run(
                "from freeports.output import FundAssets\n\
                 from freeports.consts import Currency\n\
                 FundAssets(fund='A', tot_assets=1000.0, liabilities=200.0, net_assets=800.0, currency=Currency.EUR)\n\
                 try:\n\
                 \x20   FundAssets(fund='A', tot_assets=1000.5, liabilities=200.0, net_assets=800.0, currency=Currency.EUR)\n\
                 except ValueError:\n\
                 \x20   pass\n\
                 else:\n\
                 \x20   raise AssertionError('expected a ValueError')",
            )
            .unwrap();
        }

        #[test]
        fn a_rename_and_a_merge_read_back_their_three_fields() {
            run(
                "import datetime\n\
                 from freeports.output import FundRename, FundMerge\n\
                 for cls in (FundRename, FundMerge):\n\
                 \x20   c = cls(old_name='Old', current_name='New', date=datetime.date(2024, 1, 2))\n\
                 \x20   assert c.old_name == 'Old'\n\
                 \x20   assert c.current_name == 'New'\n\
                 \x20   assert c.date == datetime.date(2024, 1, 2), c.date",
            )
            .unwrap();
        }

        #[test]
        fn an_esg_indicator_accepts_a_promised_fund() {
            run(
                "from freeports.output import FundEsgIndicator\n\
                 from freeports.core import Promise\n\
                 i = FundEsgIndicator(fund=Promise('esg-fund'), name='GHG', value='12.3')\n\
                 assert i.name == 'GHG' and i.value == '12.3'\n\
                 assert i.fund == Promise('esg-fund'), i.fund",
            )
            .unwrap();
        }

        #[test]
        fn an_sfdr_classification_accepts_the_article_as_a_shim_or_a_promise() {
            run(
                "from freeports.output import FundSfdrClassification as C\n\
                 from freeports.consts import SfdrArticle\n\
                 from freeports.core import Promise\n\
                 assert C(fund='A', article=SfdrArticle.ART_8).article == SfdrArticle.ART_8\n\
                 assert C(fund='A', article=Promise('a')).article == Promise('a')",
            )
            .unwrap();
        }

        /// The managed funds accept any iterable, not only a set: an author's deserializer is not
        /// obliged to build one.
        #[test]
        fn an_assets_manager_coerces_any_iterable_of_names() {
            run(
                "from freeports.output import ManagementCompany, InvestmentsManager\n\
                 for cls in (ManagementCompany, InvestmentsManager):\n\
                 \x20   m = cls(name='Manco', managed_funds=['B', 'A', 'A'])\n\
                 \x20   assert m.name == 'Manco'\n\
                 \x20   assert m.managed_funds == {'A', 'B'}, m.managed_funds",
            )
            .unwrap();
        }

        #[test]
        fn entities_compare_and_hash_by_value() {
            run(
                "from freeports.output import Fund\n\
                 assert Fund('a') == Fund('a')\n\
                 assert Fund('a') != Fund('b')\n\
                 assert len({Fund('a'), Fund('a'), Fund('b')}) == 2",
            )
            .unwrap();
        }
    }

    mod utils_and_interfaces {
        use super::*;

        #[test]
        fn the_two_level_submodules_are_importable() {
            run("from freeports.utils.pdf_extract import PdfLineSelection, get_groups").unwrap();
            run("from freeports.utils.text_filter import MatchFund, normalize_string").unwrap();
            run("from freeports.utils.deserialize import to_int, deserialize_block_type").unwrap();
            run("from freeports.interfaces.pdf_blks import OnePdfBlockType, ResultStandardExtraction").unwrap();
            run("from freeports.interfaces.text_blks import OneTextBlockType, StandardFundTextBlock").unwrap();
        }

        #[test]
        fn a_block_type_catalog_member_carries_its_name() {
            run("from freeports.interfaces.pdf_blks import ResultStandardExtraction as R; assert R.FUND_NAME.name == 'FUND_NAME'").unwrap();
            run("from freeports.interfaces.text_blks import ResultStandardFiltering as R; assert R.BOND_TARGET.name == 'BOND_TARGET'").unwrap();
        }

        #[test]
        fn match_fund_compares_after_deep_normalization() {
            run(
                "from freeports.utils.text_filter import MatchFund\n\
                 assert MatchFund('Alpha  Fund').matches('alpha fund')\n\
                 assert MatchFund(name='Alpha Fund').name == 'Alpha Fund'",
            )
            .unwrap();
        }

        #[test]
        fn the_casts_convert_and_raise_a_value_error_on_junk() {
            run(
                "from freeports.utils.deserialize import to_int, to_float, to_currency\n\
                 from freeports.consts import Currency\n\
                 assert to_int('1.234') == 1234, to_int('1.234')\n\
                 assert to_currency('EUR') == Currency.EUR\n\
                 try:\n\
                 \x20   to_float('not a number')\n\
                 except ValueError:\n\
                 \x20   pass\n\
                 else:\n\
                 \x20   raise AssertionError('expected a ValueError')",
            )
            .unwrap();
        }

        /// The decorators are the part of the utilities with no native counterpart: they restrict a
        /// Python callable, so they exist only in the shim.
        #[test]
        fn a_block_type_decorator_skips_blocks_of_another_type() {
            run(
                "from freeports.utils.deserialize import deserialize_block_type\n\
                 from freeports.core import TextBlock\n\
                 @deserialize_block_type('FUND')\n\
                 def d(blk):\n\
                 \x20   return blk.content\n\
                 assert d(TextBlock.from_content('FUND', {}, 'yes')) == 'yes'\n\
                 assert d(TextBlock.from_content('TABLE_BODY', {}, 'no')) is None",
            )
            .unwrap();
        }

        #[test]
        fn the_plural_block_type_decorator_accepts_any_of_its_types() {
            run(
                "from freeports.utils.deserialize import deserialize_block_types\n\
                 from freeports.core import TextBlock\n\
                 @deserialize_block_types('FUND', 'TABLE_BODY')\n\
                 def d(blk):\n\
                 \x20   return blk.type_block\n\
                 assert d(TextBlock.from_content('FUND', {}, 'x')) == 'FUND'\n\
                 assert d(TextBlock.from_content('TABLE_BODY', {}, 'x')) == 'TABLE_BODY'\n\
                 assert d(TextBlock.from_content('PAGE_CLASS', {}, 'x')) is None",
            )
            .unwrap();
        }

        #[test]
        fn a_text_selection_picks_the_lines_that_match() {
            run(
                "from freeports.utils.pdf_extract import PdfLine, PdfLineSelection\n\
                 lines = [PdfLine('Helv', 10.0, 'Fund name: Alpha', (0.0, 0.0, 100.0, 10.0)),\n\
                 \x20        PdfLine('Helv', 10.0, 'something else', (0.0, 20.0, 100.0, 30.0))]\n\
                 picked = PdfLineSelection.text('Fund name: ').select(lines)\n\
                 assert len(picked) == 1, picked\n\
                 assert picked[0].text == 'Fund name: Alpha'",
            )
            .unwrap();
        }

        #[test]
        fn selections_combine_with_the_set_operators() {
            run(
                "from freeports.utils.pdf_extract import PdfLine, PdfLineSelection\n\
                 lines = [PdfLine('Bold', 10.0, 'a', (0.0, 0.0, 10.0, 10.0)),\n\
                 \x20        PdfLine('Plain', 10.0, 'a', (0.0, 20.0, 10.0, 30.0))]\n\
                 both = PdfLineSelection.text('a')\n\
                 bold = PdfLineSelection.font('Bold')\n\
                 assert len((both & bold).select(lines)) == 1\n\
                 assert len((both | bold).select(lines)) == 2\n\
                 assert len((both / bold).select(lines)) == 1",
            )
            .unwrap();
        }

        /// The table algorithm flags are a set of flags rather than a catalogue of names: a formats
        /// repository combines them with `|`, and that is the form to preserve.
        #[test]
        fn the_table_algorithm_flags_combine_with_or() {
            run(
                "from freeports.utils.pdf_extract import TablePosAlgorithm as T\n\
                 combined = T.USE_RULER_AREA | T.BIG_CELL_RULE\n\
                 assert T.USE_RULER_AREA in combined\n\
                 assert T.USE_TEST_POS not in combined",
            )
            .unwrap();
        }

        #[test]
        fn a_table_config_accepts_one_column_or_many() {
            run(
                "from freeports.utils.pdf_extract import TableConfig, ColumnConfig\n\
                 assert len(TableConfig(ColumnConfig(limits=(0.0, 10.0))).cols) == 1\n\
                 assert len(TableConfig([ColumnConfig(), ColumnConfig()]).cols) == 2",
            )
            .unwrap();
        }

        /// A formats repository sets the splitting **after** construction: without a setter that
        /// line would stop working.
        #[test]
        fn a_column_config_splitting_is_settable_after_construction() {
            run(
                "from freeports.utils.pdf_extract import TableConfig, ColumnConfig, SplittingState\n\
                 cfg = TableConfig(ColumnConfig())\n\
                 col = ColumnConfig()\n\
                 col.splitting = None\n\
                 assert col.splitting is None\n\
                 col.splitting = SplittingState.DISALLOW\n\
                 assert col.splitting.name == 'DISALLOW'",
            )
            .unwrap();
        }

        #[test]
        fn get_groups_splits_lines_by_proximity() {
            run(
                "from freeports.utils.pdf_extract import PdfLine, get_groups\n\
                 lines = [PdfLine('H', 10.0, 'a', (0.0, 0.0, 10.0, 10.0)),\n\
                 \x20        PdfLine('H', 10.0, 'b', (0.0, 2.0, 10.0, 12.0)),\n\
                 \x20        PdfLine('H', 10.0, 'c', (0.0, 80.0, 10.0, 90.0))]\n\
                 groups = get_groups(lines, 15.0)\n\
                 assert len(set(groups)) == 2, groups",
            )
            .unwrap();
        }

        #[test]
        fn the_standard_fund_text_block_builder_produces_a_text_block() {
            run(
                "from freeports.core import PdfBlock, TextBlock\n\
                 from freeports.interfaces.text_blks import StandardFundTextBlock\n\
                 blk = StandardFundTextBlock(PdfBlock('FUND_NAME', content='Alpha'))\n\
                 assert isinstance(blk, TextBlock), type(blk)\n\
                 assert blk.type_block == 'FUND', blk.type_block",
            )
            .unwrap();
        }

        #[test]
        fn the_manager_text_block_builders_carry_their_managed_funds() {
            run(
                "from freeports.core import PdfBlock\n\
                 from freeports.utils.text_filter import MatchFund\n\
                 from freeports.interfaces.text_blks import StandardManagmentCompanyTextBlock as S\n\
                 blk = S.from_content('Manco', {MatchFund('Alpha')})\n\
                 assert blk.type_block == 'MANAGEMENT_COMPANY', blk.type_block",
            )
            .unwrap();
        }
    }

    mod block_shims {
        use super::*;

        #[test]
        fn a_pdf_block_round_trips_its_three_fields() {
            run(
                "from freeports.core import PdfBlock\n\
                 b = PdfBlock('row', content=['a', 1], metadata={'page': 3})\n\
                 assert b.type_block == 'row'\n\
                 assert b.content == ['a', 1], b.content\n\
                 assert b.metadata == {'page': 3}, b.metadata",
            )
            .unwrap();
        }

        #[test]
        fn blocks_are_hashable_and_compare_by_value() {
            run(
                "from freeports.core import PdfBlock\n\
                 assert PdfBlock('r', content='x') == PdfBlock('r', content='x')\n\
                 assert len({PdfBlock('r', content='x'), PdfBlock('r', content='x')}) == 1",
            )
            .unwrap();
        }

        #[test]
        fn a_text_block_built_from_a_pdf_block_inherits_its_content() {
            run(
                "from freeports.core import PdfBlock, TextBlock\n\
                 p = PdfBlock('row', content='inherited')\n\
                 t = TextBlock('fund', {}, p)\n\
                 assert t.content == 'inherited', t.content\n\
                 assert t.pdf_block == p",
            )
            .unwrap();
        }

        /// The two constructors are separate precisely because a content and a PDF block given
        /// together could contradict each other: one does not take a block, the other does not take
        /// a content.
        #[test]
        fn the_two_text_block_constructors_are_separate() {
            run(
                "from freeports.core import PdfBlock, TextBlock\n\
                 t = TextBlock.from_content('fund', {'k': 1}, 'standalone')\n\
                 assert t.content == 'standalone'\n\
                 assert t.pdf_block is None\n\
                 assert t.metadata == {'k': 1}\n\
                 try:\n\
                 \x20   TextBlock('fund', {}, content='x')\n\
                 except TypeError:\n\
                 \x20   pass\n\
                 else:\n\
                 \x20   raise AssertionError('the constructor must not take a content')",
            )
            .unwrap();
        }

        /// Author code rewrites the content of an already-built block, which is why a text block is
        /// not frozen.
        #[test]
        fn a_text_block_content_is_rewritable() {
            run(
                "from freeports.core import TextBlock\n\
                 t = TextBlock.from_content('fund', {}, 'Alpha Fund *')\n\
                 t.content = t.content.replace('*', '').strip()\n\
                 assert t.content == 'Alpha Fund'",
            )
            .unwrap();
        }

        /// The positional order is the one author modules actually write: type, metadata, content.
        #[test]
        fn the_blocks_take_their_arguments_in_the_reference_order() {
            run(
                "from freeports.core import PdfBlock, TextBlock\n\
                 b = PdfBlock('RELEVANT_BLOCK', {'page': 3}, 'testo')\n\
                 assert b.metadata == {'page': 3}\n\
                 assert b.content == 'testo'\n\
                 t = TextBlock('FUND', {'page': 3}, b)\n\
                 assert t.metadata == {'page': 3}\n\
                 assert t.content == 'testo'",
            )
            .unwrap();
        }

        #[test]
        fn typed_values_survive_the_round_trip_as_shims_not_strings() {
            run(
                "from freeports.core import PdfBlock\n\
                 from freeports.consts import Currency\n\
                 b = PdfBlock('amount', content=Currency.EUR)\n\
                 assert b.content == Currency.EUR, b.content",
            )
            .unwrap();
        }

        #[test]
        fn a_promise_keeps_its_flags_and_its_suffix_derived_defaults() {
            run(
                "from freeports.core import Promise\n\
                 assert Promise('fund-id').id == 'fund-id'\n\
                 assert Promise('x', strict=True, multiple=True).strict\n\
                 assert Promise('x', strict=True, multiple=True).multiple",
            )
            .unwrap();
        }

        #[test]
        fn a_promise_nested_in_a_block_comes_back_as_a_promise() {
            run(
                "from freeports.core import PdfBlock, Promise\n\
                 b = PdfBlock('ref', content=Promise('some-id'))\n\
                 assert b.content == Promise('some-id'), b.content",
            )
            .unwrap();
        }

        #[test]
        fn a_date_round_trips_through_the_iso_bridge() {
            run(
                "import datetime\n\
                 from freeports.core import PdfBlock\n\
                 b = PdfBlock('when', content=datetime.date(2024, 2, 29))\n\
                 assert b.content == datetime.date(2024, 2, 29), b.content",
            )
            .unwrap();
        }
    }
}
