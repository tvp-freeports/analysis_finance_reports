//! Rust port of the simple deserializer classes in
//! `packages/freeports_core/src/freeports/_internals/formats/utils/deserialize/standard_funcs.py`:
//! `DeserializeSfdrArticleStandard`, `DeserializerPageClassifyStandard`,
//! `DeserializerFundStandard`, `DeserializerManagmentCompanyStandard`,
//! `DeserializerInvestmentsManagerFromManco`, `DeserializerInvestmentsManagerStandard`. Each
//! just extracts a couple of fields off a `TextBlock`
//! (`_internals/core/classes.py`, not itself ported — blocked by subclassing, see
//! `agent-memory/rust-rewrite-plan.md`) and constructs an already-Rust output class, so `TextBlock`
//! is accessed generically (`&Bound<'_, PyAny>` + `getattr`/`get_item`), not through a typed
//! binding.
//!
//! **Deliberately NOT ported**: `DeserializerInvestmentStandard`/`DeserializerAssetsStandard`
//! (the two remaining classes in that Python file). Their `__call__` bodies are mostly per-field
//! `try`/`except` + translated logging (`logger.error`/`logger.warning`, `_()`) wrapped around
//! calls into the already-Rust `cast.*` functions and `Equity`/`Bond`/`FundAssets`
//! constructors — the actual computation they orchestrate is already Rust; what is left is
//! OS/i18n/logging glue, which this migration keeps in Python throughout (same reasoning as
//! `cast.py`'s thin wrapper functions). Porting them further would mean moving that logging
//! orchestration into Rust, which is exactly the kind of complication the final-cleanup
//! guidance in `agent-memory/rust-rewrite-plan.md` warns against introducing.
//!
//! Also NOT ported: the four `deserialize_block_type*`/`deserialize_block_type*_call` decorator
//! factories from that same Python file. They are generic higher-order functions that wrap
//! *arbitrary* Python callables (applied across the whole formats repo, not just this file) —
//! same category as `enum_utils.py`'s Python-only parts (Fase 1). A Rust pyclass's `__call__`
//! can't be decorated by Python source the way a `def` can anyway, so each class below inlines
//! the equivalent type-filtering check instead: return `None` when `txt_blk.type_block` doesn't
//! match, exactly like the decorators' `new_f` does.

use pyo3::prelude::*;
use pyo3::types::PyAny;

use crate::output::assets_manager::{InvestmentsManager, ManagementCompany};
use crate::output::fund::Fund;
use crate::output::fund_sfdr_classification::FundSfdrClassification;

fn type_block(txt_blk: &Bound<'_, PyAny>) -> PyResult<String> {
    txt_blk.getattr("type_block")?.extract()
}

/// `" ".join(txt_blk.content.strip().split())` — collapses all whitespace runs to single
/// spaces and trims the ends. Rust's `split_whitespace` already ignores leading/trailing
/// whitespace and splits on any run of it, so `.split_whitespace().collect().join(" ")` matches
/// the Python original exactly without needing a separate `.strip()` step.
fn normalized_content(txt_blk: &Bound<'_, PyAny>) -> PyResult<String> {
    let content: String = txt_blk.getattr("content")?.extract()?;
    Ok(content.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn managed_funds<'py>(txt_blk: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    txt_blk.getattr("metadata")?.get_item("managed_funds")
}

#[pyclass(module = "freeports._native")]
#[derive(Clone)]
pub struct DeserializeSfdrArticleStandard;

#[pymethods]
impl DeserializeSfdrArticleStandard {
    #[new]
    fn new() -> Self {
        Self
    }

    fn __call__(&self, txt_blk: &Bound<'_, PyAny>) -> PyResult<FundSfdrClassification> {
        let fund: String = txt_blk.getattr("content")?.extract()?;
        let article = txt_blk.getattr("metadata")?.get_item("article")?;
        FundSfdrClassification::new(fund, &article)
    }
}

#[pyclass(module = "freeports._native")]
#[derive(Clone)]
pub struct DeserializerPageClassifyStandard;

#[pymethods]
impl DeserializerPageClassifyStandard {
    #[new]
    fn new() -> Self {
        Self
    }

    fn __call__<'py>(&self, txt_blk: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        txt_blk.getattr("metadata")?.get_item("page_type")
    }
}

#[pyclass(module = "freeports._native")]
#[derive(Clone)]
pub struct DeserializerFundStandard;

#[pymethods]
impl DeserializerFundStandard {
    #[new]
    fn new() -> Self {
        Self
    }

    fn __call__(&self, txt_blk: &Bound<'_, PyAny>) -> PyResult<Option<Fund>> {
        if type_block(txt_blk)? != "FUND" {
            return Ok(None);
        }
        let content = txt_blk.getattr("content")?;
        Fund::new(&content).map(Some)
    }
}

#[pyclass(module = "freeports._native")]
#[derive(Clone)]
pub struct DeserializerManagmentCompanyStandard;

#[pymethods]
impl DeserializerManagmentCompanyStandard {
    #[new]
    fn new() -> Self {
        Self
    }

    fn __call__(&self, txt_blk: &Bound<'_, PyAny>) -> PyResult<Option<ManagementCompany>> {
        if type_block(txt_blk)? != "MANAGEMENT_COMPANY" {
            return Ok(None);
        }
        let name = normalized_content(txt_blk)?;
        let funds = managed_funds(txt_blk)?;
        ManagementCompany::new(name, &funds).map(Some)
    }
}

/// Same construction as `DeserializerManagmentCompanyStandard`, but builds an
/// `InvestmentsManager` from a `MANAGEMENT_COMPANY`-typed block — matches the Python original,
/// which reads exactly this way (a management-company text block doubling as the source for an
/// investments-manager record in some formats).
#[pyclass(module = "freeports._native")]
#[derive(Clone)]
pub struct DeserializerInvestmentsManagerFromManco;

#[pymethods]
impl DeserializerInvestmentsManagerFromManco {
    #[new]
    fn new() -> Self {
        Self
    }

    fn __call__(&self, txt_blk: &Bound<'_, PyAny>) -> PyResult<Option<InvestmentsManager>> {
        if type_block(txt_blk)? != "MANAGEMENT_COMPANY" {
            return Ok(None);
        }
        let name = normalized_content(txt_blk)?;
        let funds = managed_funds(txt_blk)?;
        InvestmentsManager::new(name, &funds).map(Some)
    }
}

#[pyclass(module = "freeports._native")]
#[derive(Clone)]
pub struct DeserializerInvestmentsManagerStandard;

#[pymethods]
impl DeserializerInvestmentsManagerStandard {
    #[new]
    fn new() -> Self {
        Self
    }

    fn __call__(&self, txt_blk: &Bound<'_, PyAny>) -> PyResult<Option<InvestmentsManager>> {
        if type_block(txt_blk)? != "INVESTMENTS_MANAGER" {
            return Ok(None);
        }
        let name = normalized_content(txt_blk)?;
        let funds = managed_funds(txt_blk)?;
        InvestmentsManager::new(name, &funds).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::{PyDict, PyList};

    fn make_txt_blk<'py>(py: Python<'py>, type_block: &str, content: &str, metadata: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyAny>> {
        // A tiny stand-in for TextBlock: any object with `.type_block`/`.content`/`.metadata`
        // attributes works, since these deserializers only ever use generic attribute access.
        let kwargs = PyDict::new(py);
        kwargs.set_item("type_block", type_block)?;
        kwargs.set_item("content", content)?;
        kwargs.set_item("metadata", metadata)?;
        py.import("types")?
            .getattr("SimpleNamespace")?
            .call((), Some(&kwargs))
    }

    #[test]
    fn sfdr_article_deserializer_constructs_classification() {
        Python::attach(|py| {
            let metadata = PyDict::new(py);
            metadata.set_item("article", crate::commons::consts::SfdrArticle::ART_8).unwrap();
            let txt_blk = make_txt_blk(py, "SFDR_ARTICLE", "Fund X", &metadata).unwrap();
            let deser = DeserializeSfdrArticleStandard::new();
            let result = deser.__call__(&txt_blk).unwrap();
            let fund: String = Py::new(py, result).unwrap().bind(py).getattr("fund").unwrap().extract().unwrap();
            assert_eq!(fund, "Fund X");
        });
    }

    #[test]
    fn page_classify_deserializer_reads_page_type() {
        Python::attach(|py| {
            let metadata = PyDict::new(py);
            metadata.set_item("page_type", "investments").unwrap();
            let txt_blk = make_txt_blk(py, "PAGE_CLASS", "", &metadata).unwrap();
            let deser = DeserializerPageClassifyStandard::new();
            let result: String = deser.__call__(&txt_blk).unwrap().extract().unwrap();
            assert_eq!(result, "investments");
        });
    }

    #[test]
    fn fund_deserializer_skips_wrong_block_type() {
        Python::attach(|py| {
            let metadata = PyDict::new(py);
            let txt_blk = make_txt_blk(py, "SOMETHING_ELSE", "Fund X", &metadata).unwrap();
            let deser = DeserializerFundStandard::new();
            assert!(deser.__call__(&txt_blk).unwrap().is_none());
        });
    }

    #[test]
    fn fund_deserializer_constructs_fund_for_matching_block_type() {
        Python::attach(|py| {
            let metadata = PyDict::new(py);
            let txt_blk = make_txt_blk(py, "FUND", "Café   Fund", &metadata).unwrap();
            let deser = DeserializerFundStandard::new();
            let fund = deser.__call__(&txt_blk).unwrap().unwrap();
            let name: String = Py::new(py, fund).unwrap().bind(py).getattr("name").unwrap().extract().unwrap();
            assert_eq!(name, "CAFE FUND");
        });
    }

    #[test]
    fn management_company_deserializer_normalizes_whitespace_and_reads_managed_funds() {
        Python::attach(|py| {
            let metadata = PyDict::new(py);
            let funds = PyList::new(py, ["Fund A", "Fund B"]).unwrap();
            metadata.set_item("managed_funds", funds).unwrap();
            let txt_blk = make_txt_blk(py, "MANAGEMENT_COMPANY", "  Acme   Manager  \n", &metadata).unwrap();
            let deser = DeserializerManagmentCompanyStandard::new();
            let manager = deser.__call__(&txt_blk).unwrap().unwrap();
            let name: String = Py::new(py, manager).unwrap().bind(py).getattr("name").unwrap().extract().unwrap();
            assert_eq!(name, "Acme Manager");
        });
    }

    #[test]
    fn investments_manager_from_manco_matches_management_company_block_type() {
        Python::attach(|py| {
            let metadata = PyDict::new(py);
            let funds = PyList::new(py, ["Fund A"]).unwrap();
            metadata.set_item("managed_funds", funds).unwrap();
            let txt_blk = make_txt_blk(py, "MANAGEMENT_COMPANY", "Acme Manager", &metadata).unwrap();
            let deser = DeserializerInvestmentsManagerFromManco::new();
            let manager = deser.__call__(&txt_blk).unwrap();
            assert!(manager.is_some());
        });
    }

    #[test]
    fn investments_manager_standard_matches_investments_manager_block_type() {
        Python::attach(|py| {
            let metadata = PyDict::new(py);
            let funds = PyList::new(py, ["Fund A"]).unwrap();
            metadata.set_item("managed_funds", funds).unwrap();
            let txt_blk = make_txt_blk(py, "INVESTMENTS_MANAGER", "Acme Manager", &metadata).unwrap();
            let deser = DeserializerInvestmentsManagerStandard::new();
            let manager = deser.__call__(&txt_blk).unwrap();
            assert!(manager.is_some());

            let wrong_type = make_txt_blk(py, "MANAGEMENT_COMPANY", "Acme Manager", &metadata).unwrap();
            assert!(deser.__call__(&wrong_type).unwrap().is_none());
        });
    }
}
