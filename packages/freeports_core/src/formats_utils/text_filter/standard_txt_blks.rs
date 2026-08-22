//! Rust port of `packages/freeports_core/src/freeports/_internals/formats/utils/text_filter/
//! standard_txt_blks.py`.
//!
//! See `agent-memory/fase5-porting-implementation-plan.md`, "Module 1" section, for the full
//! design this file follows.
//!
//! `OneTextBlockType`/`ResultStandardFiltering` are ported as native PyO3 C-like enums, following
//! the exact established pattern already used for `FinancialInstrument`/`SfdrArticle` in
//! `commons/consts.rs` — `#[pyclass(eq, frozen, hash, module = "freeports._native")]`,
//! `__repr__`/`__str__`/`.name` getter/`#[classmethod] fn __class_getitem__`. Per the plan, these
//! two do **not** get a `Currency`-style `#[new]`/value coercion or a `FinancialInstrument`-style
//! `.value` int getter — nothing anywhere constructs them from a raw value, only
//! `EnumName.MEMBER`/`EnumName.MEMBER.name` are ever used.
//!
//! **Judgment call flagged, not silently resolved**: because these two enums deliberately have no
//! `.value`/int concept (unlike `FinancialInstrument`/`SfdrArticle`, whose `__repr__` embeds
//! `int_value()`), `__repr__`/`__str__` below omit any embedded value entirely
//! (`<OneTextBlockType.RELEVANT_BLOCK>` / `OneTextBlockType.RELEVANT_BLOCK`, no `": N"` suffix) —
//! this is a test-writer judgment call, not something the plan itself pins down, made low-risk by
//! the plan's own observation that no real code anywhere calls `repr()`/`str()` on these two enums
//! (every real comparison is `.name`-based). Flagged for `implementer`/`critic` to confirm rather
//! than silently assumed.
//!
//! The three `Standard*TextBlock` Python classes are **not** ported as pyclasses (see the plan's
//! own explanation: PyO3 `#[new]` cannot return a different type than `Self`, and a real
//! `#[pyclass(extends = TextBlock)]` subclass would resurrect the now-dead `subtype_tag`
//! machinery). Instead, this file exposes 6 plain `#[pyfunction]`s — one "primary" (`PdfBlock`-
//! taking) and one "from-content" (string-taking) constructor per class — each building a genuine
//! native `TextBlock` directly via `TextBlock::new`/`TextBlock::from_content`. `standard_txt_blks.py`
//! keeps its 3 classes as thin dispatch shims calling these (not ported/tested here — Rust-only
//! test suite, per the plan's own "ask test-writer to pick one explicitly" note: Python-shim
//! coverage is out of scope for this file, left to a future `freeports_core` pytest smoke test or
//! the `freeports-dev` spot-check the plan's sequencing section already calls for).
//!
//! **Naming judgment call, flagged**: the plan gives the `StandardFundTextBlock` pair's exact
//! names (`standard_fund_text_block`/`standard_fund_text_block_from_content`) but only says "the
//! equivalent pair" for the other two classes, whose Python class names carry typos
//! (`StandardManagmentCompanyTextBlock`, `StandardInvestmentsMangerTextBlock`). These are new,
//! internal-only Rust names (never referenced by dotted-path fixture data, unlike the class names
//! themselves, which stay unchanged) — this file uses the corrected spelling
//! (`standard_management_company_text_block`/`standard_investments_manager_text_block`) rather
//! than mechanically preserving the Python typos. Flagged for confirmation, not silently assumed.
//!
//! **`from_matched_fund`/`from_name` collapse, flagged**: `StandardFundTextBlock` has three Python
//! alternate constructors (`__new__(pdf_blk)`, `from_matched_fund(fund: MatchFund)`,
//! `from_content(fund: str)` == `from_name`) but the plan's own "6 functions total" framing (also
//! restated directly in this task's own instructions) only allows 2 native functions for this
//! class. Resolution used here: `standard_fund_text_block_from_content` takes a plain `&str`;
//! the Python shim's `from_matched_fund` is expected to pass `fund.name` to it at the Python layer
//! (extracting the string before crossing into Rust) rather than a separate native function
//! existing for the `MatchFund`-typed entry point — this reconciles the plan's own illustrative
//! Python-shim code snippet (which names a distinct
//! `_native.core.standard_fund_text_block_from_matched_fund`) with its explicit "6 total" count;
//! the snippet is treated as non-binding shorthand. Flagged for `implementer`/`critic` to confirm.
//!
//! **`funds` argument shape, flagged**: the Python originals take `Set[MatchFund]`. This file
//! uses `Vec<Py<MatchFund>>` (order-independent for the metadata a `set` gets built from) rather
//! than a `PySet`/`PyFrozenSet`-typed argument — a plain, low-risk implementation-shape choice
//! (doesn't change any tested behavior), not mandated by the plan. Flagged, not silently assumed.
//!
//! **Pre-implementation scaffolding note (test-writer phase)**: every function/impl body below is
//! a `todo!()` stub — this file's job at this stage is only to give the test suite below a real
//! type/signature surface to compile against (`cargo test --lib` must compile cleanly even though
//! every test currently panics/fails). `implementer` fills these in; per this workspace's TDD
//! discipline, tests are the contract and must not be edited to make them pass.

use std::collections::HashSet;

use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PySet, PyType};

use crate::core::classes::{PdfBlock, TextBlock};
use crate::core::match_fund::MatchFund;

/// Rust port of `standard_txt_blks.py::OneTextBlockType`. Single member, matching the Python
/// original exactly (`RELEVANT_BLOCK = auto()`).
#[pyclass(eq, frozen, hash, module = "freeports._native")]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[allow(non_camel_case_types)]
pub enum OneTextBlockType {
    RELEVANT_BLOCK,
}

#[pymethods]
impl OneTextBlockType {
    fn __repr__(&self) -> String {
        format!("<OneTextBlockType.{}>", self.name())
    }

    fn __str__(&self) -> String {
        format!("OneTextBlockType.{}", self.name())
    }

    #[getter]
    fn name(&self) -> &'static str {
        match self {
            OneTextBlockType::RELEVANT_BLOCK => "RELEVANT_BLOCK",
        }
    }

    #[classmethod]
    fn __class_getitem__(_cls: &Bound<'_, PyType>, key: &str) -> PyResult<Self> {
        match key {
            "RELEVANT_BLOCK" => Ok(OneTextBlockType::RELEVANT_BLOCK),
            _ => Err(PyKeyError::new_err(key.to_string())),
        }
    }
}

/// Rust port of `standard_txt_blks.py::ResultStandardFiltering`. Seven members, in the Python
/// original's own declaration order (`auto()`-numbered 1..=7, though no `.value` is exposed here
/// — see this file's module doc).
#[pyclass(eq, frozen, hash, module = "freeports._native")]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[allow(non_camel_case_types)]
pub enum ResultStandardFiltering {
    BOND_TARGET,
    EQUITY_TARGET,
    FUND,
    MANAGEMENT_COMPANY,
    INVESTMENTS_MANAGER,
    SFDR_ARTICLE,
    PAGE_CLASS,
}

#[pymethods]
impl ResultStandardFiltering {
    fn __repr__(&self) -> String {
        format!("<ResultStandardFiltering.{}>", self.name())
    }

    fn __str__(&self) -> String {
        format!("ResultStandardFiltering.{}", self.name())
    }

    #[getter]
    fn name(&self) -> &'static str {
        match self {
            ResultStandardFiltering::BOND_TARGET => "BOND_TARGET",
            ResultStandardFiltering::EQUITY_TARGET => "EQUITY_TARGET",
            ResultStandardFiltering::FUND => "FUND",
            ResultStandardFiltering::MANAGEMENT_COMPANY => "MANAGEMENT_COMPANY",
            ResultStandardFiltering::INVESTMENTS_MANAGER => "INVESTMENTS_MANAGER",
            ResultStandardFiltering::SFDR_ARTICLE => "SFDR_ARTICLE",
            ResultStandardFiltering::PAGE_CLASS => "PAGE_CLASS",
        }
    }

    #[classmethod]
    fn __class_getitem__(_cls: &Bound<'_, PyType>, key: &str) -> PyResult<Self> {
        match key {
            "BOND_TARGET" => Ok(ResultStandardFiltering::BOND_TARGET),
            "EQUITY_TARGET" => Ok(ResultStandardFiltering::EQUITY_TARGET),
            "FUND" => Ok(ResultStandardFiltering::FUND),
            "MANAGEMENT_COMPANY" => Ok(ResultStandardFiltering::MANAGEMENT_COMPANY),
            "INVESTMENTS_MANAGER" => Ok(ResultStandardFiltering::INVESTMENTS_MANAGER),
            "SFDR_ARTICLE" => Ok(ResultStandardFiltering::SFDR_ARTICLE),
            "PAGE_CLASS" => Ok(ResultStandardFiltering::PAGE_CLASS),
            _ => Err(PyKeyError::new_err(key.to_string())),
        }
    }
}

/// Builds `{"managed_funds": {f.name for f in funds}}` — shared by the management-company and
/// investments-manager constructor pairs below (their only metadata difference is the
/// `type_block` tag, not this shape).
fn managed_funds_metadata(py: Python<'_>, funds: &[Py<MatchFund>]) -> Py<PyDict> {
    let names: HashSet<String> = funds.iter().map(|f| f.borrow(py).name().to_string()).collect();
    let metadata = PyDict::new(py);
    metadata
        .set_item("managed_funds", PySet::new(py, &names).unwrap())
        .unwrap();
    metadata.unbind()
}

/// Rust port of `StandardFundTextBlock.__new__` — `TextBlock(ResultStandardFiltering.FUND.name,
/// {}, blk)`.
#[pyfunction]
pub fn standard_fund_text_block(py: Python<'_>, pdf_blk: Py<PdfBlock>) -> TextBlock {
    TextBlock::new(
        py,
        ResultStandardFiltering::FUND.name().to_string(),
        PyDict::new(py).unbind(),
        pdf_blk,
    )
}

/// Rust port of `StandardFundTextBlock.from_content`/`.from_matched_fund`/`.from_name` —
/// `TextBlock.from_content(ResultStandardFiltering.FUND.name, {}, fund)`. See this file's module
/// doc for why `from_matched_fund`'s `MatchFund` argument is not a separate native function (the
/// Python shim is expected to pass `fund.name` here).
#[pyfunction]
pub fn standard_fund_text_block_from_content(py: Python<'_>, fund: &str) -> TextBlock {
    let content = fund.into_pyobject(py).unwrap().into_any().unbind();
    TextBlock::from_content(ResultStandardFiltering::FUND.name().to_string(), PyDict::new(py).unbind(), content)
}

/// Rust port of `StandardManagmentCompanyTextBlock.__new__` —
/// `TextBlock(ResultStandardFiltering.MANAGEMENT_COMPANY.name, {"managed_funds": {f.name for f in
/// funds}}, pdf_blk)`.
#[pyfunction]
pub fn standard_management_company_text_block(
    py: Python<'_>,
    pdf_blk: Py<PdfBlock>,
    funds: Vec<Py<MatchFund>>,
) -> TextBlock {
    let metadata = managed_funds_metadata(py, &funds);
    TextBlock::new(py, ResultStandardFiltering::MANAGEMENT_COMPANY.name().to_string(), metadata, pdf_blk)
}

/// Rust port of `StandardManagmentCompanyTextBlock.from_content`/`.from_name` —
/// `TextBlock.from_content(ResultStandardFiltering.MANAGEMENT_COMPANY.name, {"managed_funds":
/// {f.name for f in funds}}, name)`.
#[pyfunction]
pub fn standard_management_company_text_block_from_content(
    py: Python<'_>,
    name: &str,
    funds: Vec<Py<MatchFund>>,
) -> TextBlock {
    let metadata = managed_funds_metadata(py, &funds);
    let content = name.into_pyobject(py).unwrap().into_any().unbind();
    TextBlock::from_content(ResultStandardFiltering::MANAGEMENT_COMPANY.name().to_string(), metadata, content)
}

/// Rust port of `StandardInvestmentsMangerTextBlock.__new__` —
/// `TextBlock(ResultStandardFiltering.INVESTMENTS_MANAGER.name, {"managed_funds": {f.name for f
/// in funds}}, pdf_blk)`.
#[pyfunction]
pub fn standard_investments_manager_text_block(
    py: Python<'_>,
    pdf_blk: Py<PdfBlock>,
    funds: Vec<Py<MatchFund>>,
) -> TextBlock {
    let metadata = managed_funds_metadata(py, &funds);
    TextBlock::new(py, ResultStandardFiltering::INVESTMENTS_MANAGER.name().to_string(), metadata, pdf_blk)
}

/// Rust port of `StandardInvestmentsMangerTextBlock.from_content`/`.from_name` —
/// `TextBlock.from_content(ResultStandardFiltering.INVESTMENTS_MANAGER.name, {"managed_funds":
/// {f.name for f in funds}}, name)`.
#[pyfunction]
pub fn standard_investments_manager_text_block_from_content(
    py: Python<'_>,
    name: &str,
    funds: Vec<Py<MatchFund>>,
) -> TextBlock {
    let metadata = managed_funds_metadata(py, &funds);
    let content = name.into_pyobject(py).unwrap().into_any().unbind();
    TextBlock::from_content(ResultStandardFiltering::INVESTMENTS_MANAGER.name().to_string(), metadata, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use test_case::test_case;

    // ============================================================
    // Test helpers
    // ============================================================

    fn make_pdf_block(py: Python<'_>, type_block: &str, content: &str) -> Py<PdfBlock> {
        let metadata = PyDict::new(py);
        let content_obj = content.into_pyobject(py).unwrap().into_any().unbind();
        Py::new(py, PdfBlock::new(type_block.to_string(), metadata.unbind(), content_obj)).unwrap()
    }

    fn make_match_fund(py: Python<'_>, name: &str) -> Py<MatchFund> {
        Py::new(py, MatchFund::new(name.to_string())).unwrap()
    }

    fn str_attr(py: Python<'_>, obj: &Py<TextBlock>, attr: &str) -> String {
        obj.bind(py).getattr(attr).unwrap().extract().unwrap()
    }

    fn metadata_attr<'py>(py: Python<'py>, obj: &Py<TextBlock>) -> Bound<'py, PyDict> {
        obj.bind(py)
            .getattr("metadata")
            .unwrap()
            .cast_into::<PyDict>()
            .unwrap()
    }

    fn pdf_block_attr(py: Python<'_>, obj: &Py<TextBlock>) -> Option<Py<PdfBlock>> {
        let attr = obj.bind(py).getattr("pdf_block").unwrap();
        if attr.is_none() {
            None
        } else {
            Some(attr.extract().unwrap())
        }
    }

    fn managed_fund_names(py: Python<'_>, obj: &Py<TextBlock>) -> HashSet<String> {
        let metadata = metadata_attr(py, obj);
        metadata
            .get_item("managed_funds")
            .unwrap()
            .expect("metadata should have a managed_funds key")
            .extract::<HashSet<String>>()
            .unwrap()
    }

    fn qualname(py: Python<'_>, obj: &Py<TextBlock>) -> String {
        obj.bind(py).get_type().qualname().unwrap().extract().unwrap()
    }

    // ============================================================
    // OneTextBlockType
    // ============================================================

    #[test]
    fn one_text_block_type_relevant_block_name_is_relevant_block() {
        assert_eq!(OneTextBlockType::RELEVANT_BLOCK.name(), "RELEVANT_BLOCK");
    }

    #[test]
    fn one_text_block_type_repr_format() {
        assert_eq!(
            OneTextBlockType::RELEVANT_BLOCK.__repr__(),
            "<OneTextBlockType.RELEVANT_BLOCK>"
        );
    }

    #[test]
    fn one_text_block_type_str_format() {
        assert_eq!(
            OneTextBlockType::RELEVANT_BLOCK.__str__(),
            "OneTextBlockType.RELEVANT_BLOCK"
        );
    }

    #[test]
    fn one_text_block_type_class_getitem_succeeds_for_relevant_block() {
        Python::attach(|py| {
            let ty = py.get_type::<OneTextBlockType>();
            let value: OneTextBlockType = ty.get_item("RELEVANT_BLOCK").unwrap().extract().unwrap();
            assert_eq!(value, OneTextBlockType::RELEVANT_BLOCK);
        });
    }

    #[test]
    fn one_text_block_type_class_getitem_rejects_unknown_key_with_key_error() {
        Python::attach(|py| {
            let ty = py.get_type::<OneTextBlockType>();
            let err = ty.get_item("NOT_A_MEMBER").unwrap_err();
            assert!(err.is_instance_of::<PyKeyError>(py), "expected PyKeyError, got {err}");
        });
    }

    // ============================================================
    // ResultStandardFiltering
    // ============================================================

    #[test_case(ResultStandardFiltering::BOND_TARGET, "BOND_TARGET"; "bond target")]
    #[test_case(ResultStandardFiltering::EQUITY_TARGET, "EQUITY_TARGET"; "equity target")]
    #[test_case(ResultStandardFiltering::FUND, "FUND"; "fund")]
    #[test_case(ResultStandardFiltering::MANAGEMENT_COMPANY, "MANAGEMENT_COMPANY"; "management company")]
    #[test_case(ResultStandardFiltering::INVESTMENTS_MANAGER, "INVESTMENTS_MANAGER"; "investments manager")]
    #[test_case(ResultStandardFiltering::SFDR_ARTICLE, "SFDR_ARTICLE"; "sfdr article")]
    #[test_case(ResultStandardFiltering::PAGE_CLASS, "PAGE_CLASS"; "page class")]
    fn result_standard_filtering_name_matches_variant(variant: ResultStandardFiltering, expected: &str) {
        assert_eq!(variant.name(), expected);
    }

    #[test_case(ResultStandardFiltering::BOND_TARGET, "BOND_TARGET"; "bond target")]
    #[test_case(ResultStandardFiltering::EQUITY_TARGET, "EQUITY_TARGET"; "equity target")]
    #[test_case(ResultStandardFiltering::FUND, "FUND"; "fund")]
    #[test_case(ResultStandardFiltering::MANAGEMENT_COMPANY, "MANAGEMENT_COMPANY"; "management company")]
    #[test_case(ResultStandardFiltering::INVESTMENTS_MANAGER, "INVESTMENTS_MANAGER"; "investments manager")]
    #[test_case(ResultStandardFiltering::SFDR_ARTICLE, "SFDR_ARTICLE"; "sfdr article")]
    #[test_case(ResultStandardFiltering::PAGE_CLASS, "PAGE_CLASS"; "page class")]
    fn result_standard_filtering_repr_format(variant: ResultStandardFiltering, name: &str) {
        assert_eq!(variant.__repr__(), format!("<ResultStandardFiltering.{name}>"));
    }

    #[test_case(ResultStandardFiltering::BOND_TARGET, "BOND_TARGET"; "bond target")]
    #[test_case(ResultStandardFiltering::EQUITY_TARGET, "EQUITY_TARGET"; "equity target")]
    #[test_case(ResultStandardFiltering::FUND, "FUND"; "fund")]
    #[test_case(ResultStandardFiltering::MANAGEMENT_COMPANY, "MANAGEMENT_COMPANY"; "management company")]
    #[test_case(ResultStandardFiltering::INVESTMENTS_MANAGER, "INVESTMENTS_MANAGER"; "investments manager")]
    #[test_case(ResultStandardFiltering::SFDR_ARTICLE, "SFDR_ARTICLE"; "sfdr article")]
    #[test_case(ResultStandardFiltering::PAGE_CLASS, "PAGE_CLASS"; "page class")]
    fn result_standard_filtering_str_format(variant: ResultStandardFiltering, name: &str) {
        assert_eq!(variant.__str__(), format!("ResultStandardFiltering.{name}"));
    }

    #[test_case("BOND_TARGET"; "bond target")]
    #[test_case("EQUITY_TARGET"; "equity target")]
    #[test_case("FUND"; "fund")]
    #[test_case("MANAGEMENT_COMPANY"; "management company")]
    #[test_case("INVESTMENTS_MANAGER"; "investments manager")]
    #[test_case("SFDR_ARTICLE"; "sfdr article")]
    #[test_case("PAGE_CLASS"; "page class")]
    fn result_standard_filtering_class_getitem_succeeds_for_known_key(key: &str) {
        Python::attach(|py| {
            let ty = py.get_type::<ResultStandardFiltering>();
            let value: ResultStandardFiltering = ty.get_item(key).unwrap().extract().unwrap();
            assert_eq!(value.name(), key);
        });
    }

    #[test]
    fn result_standard_filtering_class_getitem_rejects_unknown_key_with_key_error() {
        Python::attach(|py| {
            let ty = py.get_type::<ResultStandardFiltering>();
            let err = ty.get_item("NOT_A_MEMBER").unwrap_err();
            assert!(err.is_instance_of::<PyKeyError>(py), "expected PyKeyError, got {err}");
        });
    }

    #[test]
    fn result_standard_filtering_has_seven_members() {
        // Regression guard for the plan's own "1 + 7 cases" count -- if a member is ever added or
        // removed, this (and the test_case tables above) should be revisited deliberately, not
        // silently drift.
        let all = [
            ResultStandardFiltering::BOND_TARGET,
            ResultStandardFiltering::EQUITY_TARGET,
            ResultStandardFiltering::FUND,
            ResultStandardFiltering::MANAGEMENT_COMPANY,
            ResultStandardFiltering::INVESTMENTS_MANAGER,
            ResultStandardFiltering::SFDR_ARTICLE,
            ResultStandardFiltering::PAGE_CLASS,
        ];
        assert_eq!(all.len(), 7);
    }

    // ============================================================
    // standard_fund_text_block / standard_fund_text_block_from_content
    // ============================================================

    #[test]
    fn standard_fund_text_block_has_fund_type_block() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Growth Fund");
            let result = Py::new(py, standard_fund_text_block(py, pdf_blk)).unwrap();
            assert_eq!(str_attr(py, &result, "type_block"), "FUND");
        });
    }

    #[test]
    fn standard_fund_text_block_metadata_is_empty() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Growth Fund");
            let result = Py::new(py, standard_fund_text_block(py, pdf_blk)).unwrap();
            assert_eq!(metadata_attr(py, &result).len(), 0);
        });
    }

    #[test]
    fn standard_fund_text_block_derives_content_from_the_given_pdf_block() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Growth Fund");
            let result = Py::new(py, standard_fund_text_block(py, pdf_blk.clone_ref(py))).unwrap();
            assert_eq!(str_attr(py, &result, "content"), "Acme Growth Fund");
        });
    }

    #[test]
    fn standard_fund_text_block_keeps_the_same_pdf_block_object() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Growth Fund");
            let result = Py::new(py, standard_fund_text_block(py, pdf_blk.clone_ref(py))).unwrap();
            let attached = pdf_block_attr(py, &result).expect("pdf_block should be set");
            assert!(attached.bind(py).is(pdf_blk.bind(py)));
        });
    }

    #[test]
    fn standard_fund_text_block_returns_a_plain_text_block_not_a_subtype() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Growth Fund");
            let result = Py::new(py, standard_fund_text_block(py, pdf_blk)).unwrap();
            assert_eq!(qualname(py, &result), "TextBlock");
        });
    }

    #[test]
    fn standard_fund_text_block_from_content_has_fund_type_block() {
        Python::attach(|py| {
            let result = Py::new(py, standard_fund_text_block_from_content(py, "Acme Growth Fund")).unwrap();
            assert_eq!(str_attr(py, &result, "type_block"), "FUND");
        });
    }

    #[test]
    fn standard_fund_text_block_from_content_metadata_is_empty() {
        Python::attach(|py| {
            let result = Py::new(py, standard_fund_text_block_from_content(py, "Acme Growth Fund")).unwrap();
            assert_eq!(metadata_attr(py, &result).len(), 0);
        });
    }

    #[test]
    fn standard_fund_text_block_from_content_uses_the_given_string_as_content() {
        Python::attach(|py| {
            let result = Py::new(py, standard_fund_text_block_from_content(py, "Café Fund")).unwrap();
            assert_eq!(str_attr(py, &result, "content"), "Café Fund");
        });
    }

    #[test]
    fn standard_fund_text_block_from_content_has_no_pdf_block() {
        Python::attach(|py| {
            let result = Py::new(py, standard_fund_text_block_from_content(py, "Acme Growth Fund")).unwrap();
            assert!(pdf_block_attr(py, &result).is_none());
        });
    }

    #[test]
    fn standard_fund_text_block_from_content_returns_a_plain_text_block_not_a_subtype() {
        Python::attach(|py| {
            let result = Py::new(py, standard_fund_text_block_from_content(py, "Acme Growth Fund")).unwrap();
            assert_eq!(qualname(py, &result), "TextBlock");
        });
    }

    // ============================================================
    // standard_management_company_text_block / _from_content
    // ============================================================

    #[test]
    fn standard_management_company_text_block_has_management_company_type_block() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Asset Management");
            let result = Py::new(py, standard_management_company_text_block(py, pdf_blk, vec![])).unwrap();
            assert_eq!(str_attr(py, &result, "type_block"), "MANAGEMENT_COMPANY");
        });
    }

    #[test]
    fn standard_management_company_text_block_metadata_has_empty_managed_funds_for_no_funds() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Asset Management");
            let result = Py::new(py, standard_management_company_text_block(py, pdf_blk, vec![])).unwrap();
            assert_eq!(managed_fund_names(py, &result), HashSet::new());
        });
    }

    #[test]
    fn standard_management_company_text_block_metadata_lists_a_single_managed_fund() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Asset Management");
            let fund = make_match_fund(py, "Acme Growth Fund");
            let result =
                Py::new(py, standard_management_company_text_block(py, pdf_blk, vec![fund])).unwrap();
            let expected: HashSet<String> = ["Acme Growth Fund".to_string()].into_iter().collect();
            assert_eq!(managed_fund_names(py, &result), expected);
        });
    }

    #[test]
    fn standard_management_company_text_block_metadata_lists_multiple_managed_funds() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Asset Management");
            let funds = vec![
                make_match_fund(py, "Acme Growth Fund"),
                make_match_fund(py, "Acme Bond Fund"),
                make_match_fund(py, "Café Balanced Fund"),
            ];
            let result = Py::new(py, standard_management_company_text_block(py, pdf_blk, funds)).unwrap();
            let expected: HashSet<String> = [
                "Acme Growth Fund".to_string(),
                "Acme Bond Fund".to_string(),
                "Café Balanced Fund".to_string(),
            ]
            .into_iter()
            .collect();
            assert_eq!(managed_fund_names(py, &result), expected);
        });
    }

    #[test]
    fn standard_management_company_text_block_derives_content_from_the_given_pdf_block() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Asset Management");
            let result = Py::new(py, standard_management_company_text_block(py, pdf_blk.clone_ref(py), vec![])).unwrap();
            assert_eq!(str_attr(py, &result, "content"), "Acme Asset Management");
        });
    }

    #[test]
    fn standard_management_company_text_block_keeps_the_same_pdf_block_object() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Asset Management");
            let result = Py::new(py, standard_management_company_text_block(py, pdf_blk.clone_ref(py), vec![])).unwrap();
            let attached = pdf_block_attr(py, &result).expect("pdf_block should be set");
            assert!(attached.bind(py).is(pdf_blk.bind(py)));
        });
    }

    #[test]
    fn standard_management_company_text_block_returns_a_plain_text_block_not_a_subtype() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Asset Management");
            let result = Py::new(py, standard_management_company_text_block(py, pdf_blk, vec![])).unwrap();
            assert_eq!(qualname(py, &result), "TextBlock");
        });
    }

    #[test]
    fn standard_management_company_text_block_from_content_has_management_company_type_block() {
        Python::attach(|py| {
            let result = Py::new(
                py,
                standard_management_company_text_block_from_content(py, "Acme Asset Management", vec![]),
            )
            .unwrap();
            assert_eq!(str_attr(py, &result, "type_block"), "MANAGEMENT_COMPANY");
        });
    }

    #[test]
    fn standard_management_company_text_block_from_content_metadata_has_empty_managed_funds_for_no_funds() {
        Python::attach(|py| {
            let result = Py::new(
                py,
                standard_management_company_text_block_from_content(py, "Acme Asset Management", vec![]),
            )
            .unwrap();
            assert_eq!(managed_fund_names(py, &result), HashSet::new());
        });
    }

    #[test]
    fn standard_management_company_text_block_from_content_metadata_lists_multiple_managed_funds() {
        Python::attach(|py| {
            let funds = vec![
                make_match_fund(py, "Acme Growth Fund"),
                make_match_fund(py, "Acme Bond Fund"),
            ];
            let result = Py::new(
                py,
                standard_management_company_text_block_from_content(py, "Acme Asset Management", funds),
            )
            .unwrap();
            let expected: HashSet<String> =
                ["Acme Growth Fund".to_string(), "Acme Bond Fund".to_string()].into_iter().collect();
            assert_eq!(managed_fund_names(py, &result), expected);
        });
    }

    #[test]
    fn standard_management_company_text_block_from_content_uses_the_given_string_as_content() {
        Python::attach(|py| {
            let result = Py::new(
                py,
                standard_management_company_text_block_from_content(py, "Acme Asset Management", vec![]),
            )
            .unwrap();
            assert_eq!(str_attr(py, &result, "content"), "Acme Asset Management");
        });
    }

    #[test]
    fn standard_management_company_text_block_from_content_has_no_pdf_block() {
        Python::attach(|py| {
            let result = Py::new(
                py,
                standard_management_company_text_block_from_content(py, "Acme Asset Management", vec![]),
            )
            .unwrap();
            assert!(pdf_block_attr(py, &result).is_none());
        });
    }

    #[test]
    fn standard_management_company_text_block_from_content_returns_a_plain_text_block_not_a_subtype() {
        Python::attach(|py| {
            let result = Py::new(
                py,
                standard_management_company_text_block_from_content(py, "Acme Asset Management", vec![]),
            )
            .unwrap();
            assert_eq!(qualname(py, &result), "TextBlock");
        });
    }

    // ============================================================
    // standard_investments_manager_text_block / _from_content
    // ============================================================

    #[test]
    fn standard_investments_manager_text_block_has_investments_manager_type_block() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Investments Manager");
            let result = Py::new(py, standard_investments_manager_text_block(py, pdf_blk, vec![])).unwrap();
            assert_eq!(str_attr(py, &result, "type_block"), "INVESTMENTS_MANAGER");
        });
    }

    #[test]
    fn standard_investments_manager_text_block_metadata_has_empty_managed_funds_for_no_funds() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Investments Manager");
            let result = Py::new(py, standard_investments_manager_text_block(py, pdf_blk, vec![])).unwrap();
            assert_eq!(managed_fund_names(py, &result), HashSet::new());
        });
    }

    #[test]
    fn standard_investments_manager_text_block_metadata_lists_a_single_managed_fund() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Investments Manager");
            let fund = make_match_fund(py, "Acme Growth Fund");
            let result =
                Py::new(py, standard_investments_manager_text_block(py, pdf_blk, vec![fund])).unwrap();
            let expected: HashSet<String> = ["Acme Growth Fund".to_string()].into_iter().collect();
            assert_eq!(managed_fund_names(py, &result), expected);
        });
    }

    #[test]
    fn standard_investments_manager_text_block_metadata_lists_multiple_managed_funds() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Investments Manager");
            let funds = vec![
                make_match_fund(py, "Acme Growth Fund"),
                make_match_fund(py, "Acme Bond Fund"),
                make_match_fund(py, "Café Balanced Fund"),
            ];
            let result = Py::new(py, standard_investments_manager_text_block(py, pdf_blk, funds)).unwrap();
            let expected: HashSet<String> = [
                "Acme Growth Fund".to_string(),
                "Acme Bond Fund".to_string(),
                "Café Balanced Fund".to_string(),
            ]
            .into_iter()
            .collect();
            assert_eq!(managed_fund_names(py, &result), expected);
        });
    }

    #[test]
    fn standard_investments_manager_text_block_derives_content_from_the_given_pdf_block() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Investments Manager");
            let result = Py::new(py, standard_investments_manager_text_block(py, pdf_blk.clone_ref(py), vec![])).unwrap();
            assert_eq!(str_attr(py, &result, "content"), "Acme Investments Manager");
        });
    }

    #[test]
    fn standard_investments_manager_text_block_keeps_the_same_pdf_block_object() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Investments Manager");
            let result = Py::new(py, standard_investments_manager_text_block(py, pdf_blk.clone_ref(py), vec![])).unwrap();
            let attached = pdf_block_attr(py, &result).expect("pdf_block should be set");
            assert!(attached.bind(py).is(pdf_blk.bind(py)));
        });
    }

    #[test]
    fn standard_investments_manager_text_block_returns_a_plain_text_block_not_a_subtype() {
        Python::attach(|py| {
            let pdf_blk = make_pdf_block(py, "SOME_PDF_TYPE", "Acme Investments Manager");
            let result = Py::new(py, standard_investments_manager_text_block(py, pdf_blk, vec![])).unwrap();
            assert_eq!(qualname(py, &result), "TextBlock");
        });
    }

    #[test]
    fn standard_investments_manager_text_block_from_content_has_investments_manager_type_block() {
        Python::attach(|py| {
            let result = Py::new(
                py,
                standard_investments_manager_text_block_from_content(py, "Acme Investments Manager", vec![]),
            )
            .unwrap();
            assert_eq!(str_attr(py, &result, "type_block"), "INVESTMENTS_MANAGER");
        });
    }

    #[test]
    fn standard_investments_manager_text_block_from_content_metadata_has_empty_managed_funds_for_no_funds() {
        Python::attach(|py| {
            let result = Py::new(
                py,
                standard_investments_manager_text_block_from_content(py, "Acme Investments Manager", vec![]),
            )
            .unwrap();
            assert_eq!(managed_fund_names(py, &result), HashSet::new());
        });
    }

    #[test]
    fn standard_investments_manager_text_block_from_content_metadata_lists_multiple_managed_funds() {
        Python::attach(|py| {
            let funds = vec![
                make_match_fund(py, "Acme Growth Fund"),
                make_match_fund(py, "Acme Bond Fund"),
            ];
            let result = Py::new(
                py,
                standard_investments_manager_text_block_from_content(py, "Acme Investments Manager", funds),
            )
            .unwrap();
            let expected: HashSet<String> =
                ["Acme Growth Fund".to_string(), "Acme Bond Fund".to_string()].into_iter().collect();
            assert_eq!(managed_fund_names(py, &result), expected);
        });
    }

    #[test]
    fn standard_investments_manager_text_block_from_content_uses_the_given_string_as_content() {
        Python::attach(|py| {
            let result = Py::new(
                py,
                standard_investments_manager_text_block_from_content(py, "Acme Investments Manager", vec![]),
            )
            .unwrap();
            assert_eq!(str_attr(py, &result, "content"), "Acme Investments Manager");
        });
    }

    #[test]
    fn standard_investments_manager_text_block_from_content_has_no_pdf_block() {
        Python::attach(|py| {
            let result = Py::new(
                py,
                standard_investments_manager_text_block_from_content(py, "Acme Investments Manager", vec![]),
            )
            .unwrap();
            assert!(pdf_block_attr(py, &result).is_none());
        });
    }

    #[test]
    fn standard_investments_manager_text_block_from_content_returns_a_plain_text_block_not_a_subtype() {
        Python::attach(|py| {
            let result = Py::new(
                py,
                standard_investments_manager_text_block_from_content(py, "Acme Investments Manager", vec![]),
            )
            .unwrap();
            assert_eq!(qualname(py, &result), "TextBlock");
        });
    }
}
