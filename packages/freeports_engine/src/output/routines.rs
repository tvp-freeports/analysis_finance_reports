//! Rust port of `output/routines.py`: `PageResults`/`DocumentResults` (the per-page/per-document
//! result containers `cli/main.py` builds while dispatching `Algorithm.__call__`'s output), the
//! `transform_to_files_schema` accumulator (ID assignment + `Fund`/`AssetsManager` deduplication
//! across every page of every document), and `write_files` (CSV/YAML/archive output).
//!
//! **Not ported, confirmed dead code (grepped across this repo and `analysis_finance_reports_formats`
//! before deciding, not assumed)**:
//! - The 7 `PageIndexable`-returning properties on `DocumentResults` (`.investment`,
//!   `.assets_managers`, ...) — never called anywhere, and actually broken if they ever were
//!   (`map(lambda x: x.investment)` is missing its iterable argument, and `PageResults` has no
//!   `.investment` singular attribute in the first place — `TypeError` either way).
//! - `CompanyValidator` — defined, never referenced anywhere.
//!
//! **Design decision (user confirmed, 2026-08-19, same conversation as `files_schema.rs`'s)**:
//! `transform_to_files_schema` returns an opaque [`TransformedTables`] instead of a
//! `Dict[str, pd.DataFrame]` — grepped both repos and confirmed `write_files` (via `cli/main.py`)
//! is the *only* caller, so there's no external shape to preserve. The accumulator itself reads
//! `PageResults`/`DocumentResults` contents through the existing output pyclasses' already-public
//! getters (`.company`, `.market_value`, `.fund`, ...) rather than round-tripping through
//! `model_dump()` dicts — same reasoning as `files_schema.rs`: those getters already return
//! properly-typed values (`Currency`, `SfdrArticle`, `SimpleDate`, resolved `f64`s), so there's
//! nothing to re-parse.
//!
//! **CSV/archive output uses `polars`** (user confirmed 2026-08-19): each table is built as a
//! `polars::DataFrame` from the already-validated typed rows purely as an IO/join engine at the
//! boundary — `_write_single_file`'s `instruments.merge(info_df, on="ID", how="left")` is a real
//! join, not hypothetical future need, so polars pulls its weight here directly, not just for
//! future input-side (Phase D) work.
//!
//! Date columns are written as plain `YYYY-MM-DD` strings rather than reproducing pandas'
//! `Timestamp` `.to_csv()` formatting byte-for-byte — nothing in either repo's test suite reads
//! these CSVs back and compares them against a fixed expectation (grepped both repos before
//! deciding this wasn't a fidelity requirement), so a clean ISO format was chosen over chasing
//! pandas' exact string representation.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use polars::prelude::*;

use crate::commons::consts::{Currency, FinancialInstrument, SfdrArticle};
use crate::core::py_date::SimpleDate;
use crate::output::classes::assets_manager::{InvestmentsManager, ManagementCompany};
use crate::output::files_schema::{
    AssetsManagerRow, BondAdditionalInfoRow, ChangeNameEventType, FundAssetsRow, FundChangeNameRow,
    FundEsgIndicatorRow, FundRow, FundSfdrClassificationRow, InvestmentRow, InvestmentsManagerRow, SchemaError,
    UniqueTable,
};
use crate::output::classes::fund::Fund;
use crate::output::classes::fund_change_name::{FundMerge, FundRename};
use crate::output::classes::investment::{Bond, Equity};

fn schema_err(e: SchemaError) -> PyErr {
    PyValueError::new_err(e.to_string())
}

fn extract_optional<'a, 'py, T>(value: &'a Bound<'py, PyAny>) -> PyResult<Option<T>>
where
    T: FromPyObject<'a, 'py>,
    T::Error: Into<PyErr>,
{
    if value.is_none() {
        Ok(None)
    } else {
        Some(value.extract::<T>()).transpose().map_err(Into::into)
    }
}

// ---------------------------------------------------------------------------------------------
// PageResults / DocumentResults
// ---------------------------------------------------------------------------------------------

fn fulfill_and_filter<'py>(
    py: Python<'py>,
    old_list: &Bound<'py, PyList>,
    mapping: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyList>> {
    let new_list = PyList::empty(py);
    for item in old_list.try_iter()? {
        let item = item?;
        let result = match item.call_method1("fulfill_promises", (mapping,)) {
            Ok(r) => r,
            Err(e) if e.is_instance_of::<pyo3::exceptions::PyKeyError>(py) => continue,
            Err(e) => return Err(e),
        };
        if result.is_none() {
            new_list.append(&item)?;
        } else {
            let result_list = result.cast::<PyList>().map_err(PyErr::from)?;
            if result_list.is_empty() {
                continue;
            }
            for r in result_list.try_iter()? {
                new_list.append(r?)?;
            }
        }
    }
    Ok(new_list)
}

/// Rust port of `PageResults`. Every `Xs: List[...]` attribute is a live, mutable `PyList` — same
/// object identity across gets, so Python code doing `pr.investments.append(r)` (as
/// `cli/main.py`'s dispatch loop does) mutates this instance in place, matching Python attribute
/// access into a list field exactly.
#[pyclass(module = "freeports_engine")]
pub struct PageResults {
    #[pyo3(get, set)]
    page_number: i64,
    investments: Py<PyList>,
    assets_managers: Py<PyList>,
    funds: Py<PyList>,
    funds_sfdr_classification: Py<PyList>,
    funds_esg_indicators: Py<PyList>,
    funds_assets: Py<PyList>,
    funds_change_name: Py<PyList>,
}

#[pymethods]
impl PageResults {
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            page_number: 0,
            investments: PyList::empty(py).unbind(),
            assets_managers: PyList::empty(py).unbind(),
            funds: PyList::empty(py).unbind(),
            funds_sfdr_classification: PyList::empty(py).unbind(),
            funds_esg_indicators: PyList::empty(py).unbind(),
            funds_assets: PyList::empty(py).unbind(),
            funds_change_name: PyList::empty(py).unbind(),
        })
    }

    #[getter]
    fn investments(&self, py: Python<'_>) -> Py<PyList> {
        self.investments.clone_ref(py)
    }
    #[getter]
    fn assets_managers(&self, py: Python<'_>) -> Py<PyList> {
        self.assets_managers.clone_ref(py)
    }
    #[getter]
    fn funds(&self, py: Python<'_>) -> Py<PyList> {
        self.funds.clone_ref(py)
    }
    #[getter]
    fn funds_sfdr_classification(&self, py: Python<'_>) -> Py<PyList> {
        self.funds_sfdr_classification.clone_ref(py)
    }
    #[getter]
    fn funds_esg_indicators(&self, py: Python<'_>) -> Py<PyList> {
        self.funds_esg_indicators.clone_ref(py)
    }
    #[getter]
    fn funds_assets(&self, py: Python<'_>) -> Py<PyList> {
        self.funds_assets.clone_ref(py)
    }
    #[getter]
    fn funds_change_name(&self, py: Python<'_>) -> Py<PyList> {
        self.funds_change_name.clone_ref(py)
    }

    fn fulfill_promises(&mut self, py: Python<'_>, mapping: &Bound<'_, PyDict>) -> PyResult<()> {
        self.investments = fulfill_and_filter(py, self.investments.bind(py), mapping)?.unbind();
        self.assets_managers = fulfill_and_filter(py, self.assets_managers.bind(py), mapping)?.unbind();
        self.funds = fulfill_and_filter(py, self.funds.bind(py), mapping)?.unbind();
        self.funds_assets = fulfill_and_filter(py, self.funds_assets.bind(py), mapping)?.unbind();
        self.funds_change_name = fulfill_and_filter(py, self.funds_change_name.bind(py), mapping)?.unbind();
        self.funds_sfdr_classification = fulfill_and_filter(py, self.funds_sfdr_classification.bind(py), mapping)?.unbind();
        self.funds_esg_indicators = fulfill_and_filter(py, self.funds_esg_indicators.bind(py), mapping)?.unbind();
        Ok(())
    }
}

/// Rust port of `DocumentResults`.
#[pyclass(module = "freeports_engine")]
pub struct DocumentResults {
    #[pyo3(get)]
    report_name: String,
    #[pyo3(get)]
    algorithm: String,
    results: Py<PyList>,
}

#[pymethods]
impl DocumentResults {
    #[new]
    fn new(py: Python<'_>, report_name: String, algorithm: String) -> PyResult<Self> {
        Ok(Self { report_name, algorithm, results: PyList::empty(py).unbind() })
    }

    #[getter]
    fn results(&self, py: Python<'_>) -> Py<PyList> {
        self.results.clone_ref(py)
    }

    fn __getitem__<'py>(&self, py: Python<'py>, page_n: isize) -> PyResult<Bound<'py, PyAny>> {
        let idx = usize::try_from(page_n - 1).map_err(|_| PyValueError::new_err("page_n must be >= 1"))?;
        self.results.bind(py).get_item(idx)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(self.results.bind(py).try_iter()?.unbind().into_any())
    }

    fn add_report_infos<'py>(&self, d: &Bound<'py, PyDict>) -> PyResult<Bound<'py, PyDict>> {
        d.set_item("Report", &self.report_name)?;
        d.set_item("Format", &self.algorithm)?;
        Ok(d.clone())
    }

    fn fulfill_promises(&self, py: Python<'_>, mapping: &Bound<'_, PyDict>) -> PyResult<()> {
        for pr in self.results.bind(py).try_iter()? {
            let pr = pr?;
            let pr = pr.cast::<PageResults>().map_err(PyErr::from)?;
            pr.borrow_mut().fulfill_promises(py, mapping)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// transform_to_files_schema
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
struct FundEntryBuilder {
    id: u32,
    name: String,
    management_company_id: Option<u32>,
    report_page: Option<i32>,
    report: Option<String>,
    format: Option<String>,
}

#[derive(Default)]
struct Accumulator {
    investments: Vec<InvestmentRow>,
    fund_index: HashMap<String, usize>,
    funds: Vec<FundEntryBuilder>,
    manager_index: HashMap<String, usize>,
    assets_managers: Vec<AssetsManagerRow>,
    funds_change_name: Vec<FundChangeNameRow>,
    funds_assets: Vec<FundAssetsRow>,
    funds_sfdr_classification: Vec<FundSfdrClassificationRow>,
    funds_esg_indicators: Vec<FundEsgIndicatorRow>,
    investments_managers_to_funds: Vec<InvestmentsManagerRow>,
    add_infos: HashMap<u32, BondAdditionalInfoRow>,
}

impl Accumulator {
    fn get_or_create_fund(&mut self, name: &str) -> usize {
        if let Some(&idx) = self.fund_index.get(name) {
            idx
        } else {
            let id = self.funds.len() as u32 + 1;
            self.funds.push(FundEntryBuilder {
                id,
                name: name.to_string(),
                management_company_id: None,
                report_page: None,
                report: None,
                format: None,
            });
            let idx = self.funds.len() - 1;
            self.fund_index.insert(name.to_string(), idx);
            idx
        }
    }

    fn finalize(self) -> PyResult<TransformedTables> {
        let funds: Vec<FundRow> = self
            .funds
            .into_iter()
            .map(|b| FundRow::new(b.id as i64, b.name, b.management_company_id.map(|v| v as i64), b.report_page, b.report, b.format))
            .collect::<Result<_, _>>()
            .map_err(schema_err)?;

        let mut fcn_table: UniqueTable<FundChangeNameRow> = UniqueTable::new("funds_change_name", "Fund ID|From|Type of event|Old name");
        for row in self.funds_change_name {
            let key = format!("{}|{:?}|{:?}|{}", row.fund_id, row.from_date, row.event_type, row.old_name);
            fcn_table.push(key, row).map_err(schema_err)?;
        }

        let mut fa_table: UniqueTable<FundAssetsRow> = UniqueTable::new("funds_assets", "Fund ID|Date");
        for row in self.funds_assets {
            let key = format!("{}|{:?}", row.fund_id, row.date);
            fa_table.push(key, row).map_err(schema_err)?;
        }

        let mut sfdr_table: UniqueTable<FundSfdrClassificationRow> = UniqueTable::new("funds_sfdr_classification", "Fund ID");
        for row in self.funds_sfdr_classification {
            let key = row.fund_id.to_string();
            sfdr_table.push(key, row).map_err(schema_err)?;
        }

        let mut im_table: UniqueTable<InvestmentsManagerRow> = UniqueTable::new("investments_managers", "Investment manager ID|Fund ID");
        for row in self.investments_managers_to_funds {
            let key = format!("{}|{}", row.investment_manager_id, row.fund_id);
            im_table.push(key, row).map_err(schema_err)?;
        }

        Ok(TransformedTables {
            investments: self.investments,
            assets_managers: self.assets_managers,
            funds,
            investments_managers: im_table.into_rows(),
            funds_sfdr_classification: sfdr_table.into_rows(),
            funds_esg_indicators: self.funds_esg_indicators,
            funds_change_name: fcn_table.into_rows(),
            funds_assets: fa_table.into_rows(),
            additional_infos: self.add_infos,
        })
    }
}

/// `Fund(name=raw).name` without needing a throwaway pyclass round-trip result — reuses the real
/// `Fund` normalization (`Fund::new`/`.name`) so this always agrees with whatever key a directly
/// encountered `Fund` object (`page_results.funds`) would produce. Idempotent on an
/// already-normalized name (see `fund.rs`'s own doc comment), so it's safe to call uniformly.
fn normalized_fund_key(py: Python<'_>, raw_name: &str) -> PyResult<String> {
    let bound = raw_name.into_pyobject(py)?.into_any();
    let fund = Fund::new(&bound)?;
    fund.name(py)?.extract::<String>()
}

/// Rust port of `transform_to_files_schema`. `batch_mode` is accepted for interface parity with
/// the Python original but unused — the original's own docstring says as much (`add_debug_infos`
/// always adds report info regardless of the flag; verified reading the source, not assumed).
#[pyfunction]
#[pyo3(name = "transform_to_files_schema", signature = (results, batch_mode))]
pub fn py_transform_to_files_schema(py: Python<'_>, results: &Bound<'_, PyList>, batch_mode: bool) -> PyResult<TransformedTables> {
    let _ = batch_mode;
    let mut acc = Accumulator::default();

    for document_results in results.try_iter()? {
        let document_results = document_results?;
        let doc = document_results.cast::<DocumentResults>().map_err(PyErr::from)?;
        let doc = doc.borrow();
        let report_name = doc.report_name.clone();
        let algorithm = doc.algorithm.clone();

        for page_results in doc.results.bind(py).try_iter()? {
            let page_results = page_results?;
            let pr = page_results.cast::<PageResults>().map_err(PyErr::from)?;
            let pr = pr.borrow();
            let page_n = pr.page_number;

            for f in pr.funds.bind(py).try_iter()? {
                let f = f?;
                if f.is_none() {
                    continue;
                }
                let name: String = f.getattr("name")?.extract()?;
                let idx = acc.get_or_create_fund(&name);
                if acc.funds[idx].report_page.is_none() {
                    acc.funds[idx].report_page = Some(page_n as i32);
                    acc.funds[idx].report = Some(report_name.clone());
                    acc.funds[idx].format = Some(algorithm.clone());
                }
            }

            for fcm in pr.funds_change_name.bind(py).try_iter()? {
                let fcm = fcm?;
                if fcm.is_none() {
                    continue;
                }
                let current_name: String = fcm.getattr("current_name")?.extract()?;
                let key = normalized_fund_key(py, &current_name)?;
                acc.get_or_create_fund(&key);
                let fund_id = acc.funds[acc.fund_index[&key]].id;

                let old_name: String = fcm.getattr("old_name")?.extract()?;
                let from_date: SimpleDate = fcm.getattr("date")?.extract()?;
                let event_type = if fcm.is_instance_of::<FundRename>() {
                    ChangeNameEventType::Renaming
                } else if fcm.is_instance_of::<FundMerge>() {
                    ChangeNameEventType::Merging
                } else {
                    return Err(PyTypeError::new_err("funds_change_name entry is neither FundRename nor FundMerge"));
                };
                let id = acc.funds_change_name.len() as i64 + 1;
                acc.funds_change_name.push(
                    FundChangeNameRow::new(id, page_n as i32, report_name.clone(), algorithm.clone(), fund_id as i64, from_date, event_type, old_name)
                        .map_err(schema_err)?,
                );
            }

            for fa in pr.funds_assets.bind(py).try_iter()? {
                let fa = fa?;
                if fa.is_none() {
                    continue;
                }
                let fund_raw: String = fa.getattr("fund")?.extract()?;
                let key = normalized_fund_key(py, &fund_raw)?;
                acc.get_or_create_fund(&key);
                let fund_id = acc.funds[acc.fund_index[&key]].id;

                let date: Option<SimpleDate> = extract_optional(&fa.getattr("date")?)?;
                let total_assets: f64 = fa.getattr("tot_assets")?.extract()?;
                let total_liabilities: f64 = fa.getattr("liabilities")?.extract()?;
                let total_net_assets: f64 = fa.getattr("net_assets")?.extract()?;
                let currency: Currency = fa.getattr("currency")?.extract()?;

                let id = acc.funds_assets.len() as i64 + 1;
                acc.funds_assets.push(
                    FundAssetsRow::new(
                        id,
                        page_n as i32,
                        report_name.clone(),
                        algorithm.clone(),
                        fund_id as i64,
                        date,
                        total_assets as f32,
                        total_liabilities as f32,
                        total_net_assets as f32,
                        currency,
                    )
                    .map_err(schema_err)?,
                );
            }

            for fsc in pr.funds_sfdr_classification.bind(py).try_iter()? {
                let fsc = fsc?;
                if fsc.is_none() {
                    continue;
                }
                let article: SfdrArticle = fsc.getattr("article")?.extract()?;
                let fund_raw: String = fsc.getattr("fund")?.extract()?;
                let key = normalized_fund_key(py, &fund_raw)?;
                acc.get_or_create_fund(&key);
                let fund_id = acc.funds[acc.fund_index[&key]].id;
                acc.funds_sfdr_classification.push(
                    FundSfdrClassificationRow::new(fund_id as i64, article, page_n as i32, report_name.clone(), algorithm.clone())
                        .map_err(schema_err)?,
                );
            }

            for fei in pr.funds_esg_indicators.bind(py).try_iter()? {
                let fei = fei?;
                if fei.is_none() {
                    continue;
                }
                let fund_raw: String = fei.getattr("fund")?.extract()?;
                let key = normalized_fund_key(py, &fund_raw)?;
                acc.get_or_create_fund(&key);
                let fund_id = acc.funds[acc.fund_index[&key]].id;
                let indicator: String = fei.getattr("name")?.extract()?;
                let value: String = fei.getattr("value")?.extract()?;
                acc.funds_esg_indicators.push(
                    FundEsgIndicatorRow::new(fund_id as i64, indicator, value, page_n as i32, report_name.clone(), algorithm.clone())
                        .map_err(schema_err)?,
                );
            }

            for inv in pr.investments.bind(py).try_iter()? {
                let inv = inv?;
                if inv.is_none() {
                    continue;
                }
                let fund_raw: String = inv.getattr("fund")?.extract()?;
                let key = normalized_fund_key(py, &fund_raw)?;
                acc.get_or_create_fund(&key);
                let fund_id = acc.funds[acc.fund_index[&key]].id;

                let triggering_text: String = inv.getattr("company_match")?.extract()?;
                let investee: String = inv.getattr("company")?.extract()?;
                let nominal_quantity: Option<f64> = extract_optional(&inv.getattr("nominal_quantity")?)?;
                let market_value: f64 = inv.getattr("market_value")?.extract()?;
                let currency: Currency = inv.getattr("currency")?.extract()?;
                let perc_net_assets: Option<f64> = extract_optional(&inv.getattr("perc_net_assets")?)?;
                let acquisition_cost: Option<f64> = extract_optional(&inv.getattr("acquisition_cost")?)?;
                let acquisition_currency: Option<Currency> = extract_optional(&inv.getattr("acquisition_currency")?)?;

                let financial_instrument = if inv.is_instance_of::<Equity>() {
                    FinancialInstrument::EQUITY
                } else if inv.is_instance_of::<Bond>() {
                    FinancialInstrument::BOND
                } else {
                    return Err(PyTypeError::new_err("investments entry is neither Equity nor Bond"));
                };

                let id = acc.investments.len() as i64 + 1;
                let row = InvestmentRow::new(
                    id,
                    page_n as i32,
                    report_name.clone(),
                    algorithm.clone(),
                    triggering_text,
                    investee,
                    financial_instrument,
                    nominal_quantity.map(|v| v as f32),
                    market_value as f32,
                    currency,
                    perc_net_assets.map(|v| v as f32),
                    fund_id as i64,
                    acquisition_cost.map(|v| v as f32),
                    acquisition_currency,
                )
                .map_err(schema_err)?;

                if financial_instrument == FinancialInstrument::BOND {
                    let maturity: Option<SimpleDate> = extract_optional(&inv.getattr("maturity")?)?;
                    let interest_rate: Option<f64> = extract_optional(&inv.getattr("interest_rate")?)?;
                    let bond_info = BondAdditionalInfoRow::new(maturity, interest_rate).map_err(schema_err)?;
                    acc.add_infos.insert(row.id, bond_info);
                }
                acc.investments.push(row);
            }

            for am in pr.assets_managers.bind(py).try_iter()? {
                let am = am?;
                if am.is_none() {
                    continue;
                }
                let am_name: String = am.getattr("name")?.extract()?;
                let am_idx = if let Some(&idx) = acc.manager_index.get(&am_name) {
                    idx
                } else {
                    let id = acc.assets_managers.len() as i64 + 1;
                    let row =
                        AssetsManagerRow::new(id, page_n as i32, report_name.clone(), algorithm.clone(), am_name.clone()).map_err(schema_err)?;
                    acc.assets_managers.push(row);
                    let idx = acc.assets_managers.len() - 1;
                    acc.manager_index.insert(am_name.clone(), idx);
                    idx
                };
                let am_id = acc.assets_managers[am_idx].id;

                let is_management_company = am.is_instance_of::<ManagementCompany>();
                let is_investments_manager = am.is_instance_of::<InvestmentsManager>();
                for s in am.getattr("managed_funds")?.try_iter()? {
                    let s: String = s?.extract()?;
                    let key = normalized_fund_key(py, &s)?;
                    let fund_idx = acc.get_or_create_fund(&key);
                    if is_management_company {
                        acc.funds[fund_idx].management_company_id = Some(am_id);
                    }
                    if is_investments_manager {
                        let fund_id = acc.funds[fund_idx].id;
                        acc.investments_managers_to_funds
                            .push(InvestmentsManagerRow::new(am_id as i64, fund_id as i64).map_err(schema_err)?);
                    }
                }
            }
        }
    }

    acc.finalize()
}

// ---------------------------------------------------------------------------------------------
// TransformedTables + write_files
// ---------------------------------------------------------------------------------------------

/// Opaque carrier handed from [`py_transform_to_files_schema`] to [`py_write_files`] — not a
/// literal port of the Python `Dict[str, pd.DataFrame]` return shape (see module doc: nothing
/// else consumes it, so there's no external shape to preserve).
#[derive(Debug)]
#[pyclass(module = "freeports_engine")]
pub struct TransformedTables {
    investments: Vec<InvestmentRow>,
    assets_managers: Vec<AssetsManagerRow>,
    funds: Vec<FundRow>,
    investments_managers: Vec<InvestmentsManagerRow>,
    funds_sfdr_classification: Vec<FundSfdrClassificationRow>,
    funds_esg_indicators: Vec<FundEsgIndicatorRow>,
    funds_change_name: Vec<FundChangeNameRow>,
    funds_assets: Vec<FundAssetsRow>,
    additional_infos: HashMap<u32, BondAdditionalInfoRow>,
}

#[pymethods]
impl TransformedTables {}

fn opt_date_col(dates: impl Iterator<Item = Option<SimpleDate>>) -> Vec<Option<String>> {
    dates.map(|d| d.map(|d| format!("{:04}-{:02}-{:02}", d.year, d.month, d.day))).collect()
}

fn investments_df(rows: &[InvestmentRow]) -> PolarsResult<DataFrame> {
    df!(
        "ID" => rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        "Format" => rows.iter().map(|r| r.format.as_str()).collect::<Vec<_>>(),
        "Report" => rows.iter().map(|r| r.report.as_str()).collect::<Vec<_>>(),
        "Report page" => rows.iter().map(|r| r.report_page as u32).collect::<Vec<_>>(),
        "Triggering text" => rows.iter().map(|r| r.triggering_text.as_str()).collect::<Vec<_>>(),
        "Investee" => rows.iter().map(|r| r.investee.as_str()).collect::<Vec<_>>(),
        "Financial instrument" => rows.iter().map(|r| match r.financial_instrument {
            FinancialInstrument::EQUITY => "EQUITY",
            FinancialInstrument::BOND => "BOND",
        }).collect::<Vec<_>>(),
        "Nominal/Quantity" => rows.iter().map(|r| r.nominal_quantity).collect::<Vec<_>>(),
        "Market value" => rows.iter().map(|r| r.market_value).collect::<Vec<_>>(),
        "Currency" => rows.iter().map(|r| r.currency.code()).collect::<Vec<_>>(),
        "% net assets" => rows.iter().map(|r| r.perc_net_assets).collect::<Vec<_>>(),
        "Fund ID" => rows.iter().map(|r| r.fund_id).collect::<Vec<_>>(),
        "Acquisition cost" => rows.iter().map(|r| r.acquisition_cost).collect::<Vec<_>>(),
        "Acquisition currency" => rows.iter().map(|r| r.acquisition_currency.map(|c| c.code())).collect::<Vec<_>>(),
    )
}

fn assets_managers_df(rows: &[AssetsManagerRow]) -> PolarsResult<DataFrame> {
    df!(
        "ID" => rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        "Format" => rows.iter().map(|r| r.format.as_str()).collect::<Vec<_>>(),
        "Report" => rows.iter().map(|r| r.report.as_str()).collect::<Vec<_>>(),
        "Report page" => rows.iter().map(|r| r.report_page as u32).collect::<Vec<_>>(),
        "Name" => rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
    )
}

fn funds_df(rows: &[FundRow]) -> PolarsResult<DataFrame> {
    df!(
        "ID" => rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        "Format" => rows.iter().map(|r| r.format.clone()).collect::<Vec<_>>(),
        "Report" => rows.iter().map(|r| r.report.clone()).collect::<Vec<_>>(),
        "Report page" => rows.iter().map(|r| r.report_page.map(|v| v as u32)).collect::<Vec<_>>(),
        "Name" => rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        "Managment company ID" => rows.iter().map(|r| r.management_company_id).collect::<Vec<_>>(),
    )
}

fn investments_managers_df(rows: &[InvestmentsManagerRow]) -> PolarsResult<DataFrame> {
    df!(
        "Investment manager ID" => rows.iter().map(|r| r.investment_manager_id).collect::<Vec<_>>(),
        "Fund ID" => rows.iter().map(|r| r.fund_id).collect::<Vec<_>>(),
    )
}

fn funds_sfdr_classification_df(rows: &[FundSfdrClassificationRow]) -> PolarsResult<DataFrame> {
    df!(
        "Fund ID" => rows.iter().map(|r| r.fund_id).collect::<Vec<_>>(),
        "SFDR classification" => rows.iter().map(|r| match r.sfdr_classification {
            SfdrArticle::ART_6 => "Art. 6",
            SfdrArticle::ART_8 => "Art. 8",
            SfdrArticle::ART_9 => "Art. 9",
        }).collect::<Vec<_>>(),
        "Report page" => rows.iter().map(|r| r.report_page as u32).collect::<Vec<_>>(),
        "Format" => rows.iter().map(|r| r.format.as_str()).collect::<Vec<_>>(),
        "Report" => rows.iter().map(|r| r.report.as_str()).collect::<Vec<_>>(),
    )
}

fn funds_esg_indicators_df(rows: &[FundEsgIndicatorRow]) -> PolarsResult<DataFrame> {
    df!(
        "Fund ID" => rows.iter().map(|r| r.fund_id).collect::<Vec<_>>(),
        "Indicator" => rows.iter().map(|r| r.indicator.as_str()).collect::<Vec<_>>(),
        "Value" => rows.iter().map(|r| r.value.as_str()).collect::<Vec<_>>(),
        "Report page" => rows.iter().map(|r| r.report_page as u32).collect::<Vec<_>>(),
        "Format" => rows.iter().map(|r| r.format.as_str()).collect::<Vec<_>>(),
        "Report" => rows.iter().map(|r| r.report.as_str()).collect::<Vec<_>>(),
    )
}

fn funds_change_name_df(rows: &[FundChangeNameRow]) -> PolarsResult<DataFrame> {
    df!(
        "ID" => rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        "Format" => rows.iter().map(|r| r.format.as_str()).collect::<Vec<_>>(),
        "Report" => rows.iter().map(|r| r.report.as_str()).collect::<Vec<_>>(),
        "Report page" => rows.iter().map(|r| r.report_page as u32).collect::<Vec<_>>(),
        "Fund ID" => rows.iter().map(|r| r.fund_id).collect::<Vec<_>>(),
        "From" => opt_date_col(rows.iter().map(|r| Some(r.from_date))),
        "Type of event" => rows.iter().map(|r| match r.event_type {
            ChangeNameEventType::Renaming => "RENAMING",
            ChangeNameEventType::Merging => "MERGING",
        }).collect::<Vec<_>>(),
        "Old name" => rows.iter().map(|r| r.old_name.as_str()).collect::<Vec<_>>(),
    )
}

fn funds_assets_df(rows: &[FundAssetsRow]) -> PolarsResult<DataFrame> {
    df!(
        "ID" => rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        "Format" => rows.iter().map(|r| r.format.as_str()).collect::<Vec<_>>(),
        "Report" => rows.iter().map(|r| r.report.as_str()).collect::<Vec<_>>(),
        "Report page" => rows.iter().map(|r| r.report_page as u32).collect::<Vec<_>>(),
        "Fund ID" => rows.iter().map(|r| r.fund_id).collect::<Vec<_>>(),
        "Date" => opt_date_col(rows.iter().map(|r| r.date)),
        "Total assets" => rows.iter().map(|r| r.total_assets).collect::<Vec<_>>(),
        "Total liabilities" => rows.iter().map(|r| r.total_liabilities).collect::<Vec<_>>(),
        "Total net assets" => rows.iter().map(|r| r.total_net_assets).collect::<Vec<_>>(),
        "Currency" => rows.iter().map(|r| r.currency.code()).collect::<Vec<_>>(),
    )
}

fn write_csv(df: &mut DataFrame, path: &Path) -> PyResult<()> {
    let mut file = File::create(path).map_err(|e| PyValueError::new_err(format!("cannot create {}: {e}", path.display())))?;
    CsvWriter::new(&mut file)
        .finish(df)
        .map_err(|e| PyValueError::new_err(format!("cannot write CSV {}: {e}", path.display())))
}

/// Python's `repr(float)`/`str(float)` (what PyYAML's dumper calls on every float scalar) always
/// keeps a decimal point (`repr(50.0) == "50.0"`), unlike Rust's `Display for f64` (`format!("{}",
/// 50.0_f64) == "50"`) — the values here are fractional interest rates so this rarely bites, but
/// matching it exactly costs one `if`.
fn python_repr_float(v: f64) -> String {
    let s = format!("{v}");
    if s.contains('.') || s.contains('e') || s.contains('E') || s.contains("inf") || s.contains("nan") {
        s
    } else {
        format!("{s}.0")
    }
}

/// Hand-built rather than routed through `serde_yaml`: PyYAML's dumper (a) sorts mapping keys
/// alphabetically by default (`interest_rate` before `maturity`) and (b) single-quotes a plain
/// scalar that would otherwise resolve to a different type on reload — an unquoted
/// `2028-03-30` is `!!timestamp` under YAML's core schema, so PyYAML quotes it to keep it a
/// `str` on the Python side, matching `BondAdditionalInfos.model_dump(mode="json")`'s already-
/// stringified date. `serde_yaml` 0.9's public `Value`/emitter API doesn't expose per-scalar
/// style control to force that same quoting, so this reproduces PyYAML's exact output shape
/// directly — caught by the full `analysis_finance_reports_formats` suite comparing this file's
/// re-loaded dict against a real Python-`yaml.dump`-produced fixture, not a hypothetical.
fn write_yaml_dicts(additional_infos: &HashMap<u32, BondAdditionalInfoRow>, path: &Path, keys: Option<&[u32]>) -> PyResult<()> {
    let mut ordered: Vec<(&u32, &BondAdditionalInfoRow)> = match keys {
        Some(keys) => keys.iter().filter_map(|k| additional_infos.get_key_value(k)).collect(),
        None => additional_infos.iter().collect(),
    };
    ordered.sort_by_key(|(k, _)| **k);

    if ordered.is_empty() {
        // PyYAML's flow-style representation of an empty dict — a bare `for` loop below would
        // instead emit nothing (0 bytes), which is not valid YAML and doesn't match `yaml.dump({})`.
        return std::fs::write(path, "{}\n").map_err(|e| PyValueError::new_err(format!("cannot write {}: {e}", path.display())));
    }

    let mut out = String::new();
    for (id, info) in ordered {
        out.push_str(&format!("{id}:\n"));
        match info.interest_rate {
            None => out.push_str("  interest_rate: null\n"),
            Some(v) => out.push_str(&format!("  interest_rate: {}\n", python_repr_float(v))),
        }
        match info.maturity {
            None => out.push_str("  maturity: null\n"),
            Some(d) => out.push_str(&format!("  maturity: '{:04}-{:02}-{:02}'\n", d.year, d.month, d.day)),
        }
    }
    std::fs::write(path, out).map_err(|e| PyValueError::new_err(format!("cannot write {}: {e}", path.display())))
}

fn write_regular(data: &TransformedTables, out_dir: &Path) -> PyResult<()> {
    std::fs::create_dir_all(out_dir).map_err(|e| PyValueError::new_err(format!("cannot create {}: {e}", out_dir.display())))?;
    write_csv(&mut investments_df(&data.investments).map_err(polars_err)?, &out_dir.join("investments.csv"))?;
    write_csv(&mut funds_assets_df(&data.funds_assets).map_err(polars_err)?, &out_dir.join("funds_assets.csv"))?;
    write_csv(&mut funds_df(&data.funds).map_err(polars_err)?, &out_dir.join("funds.csv"))?;
    write_csv(
        &mut funds_sfdr_classification_df(&data.funds_sfdr_classification).map_err(polars_err)?,
        &out_dir.join("funds_sfdr_classification.csv"),
    )?;
    write_csv(&mut funds_esg_indicators_df(&data.funds_esg_indicators).map_err(polars_err)?, &out_dir.join("funds_esg_indicators.csv"))?;
    write_csv(&mut assets_managers_df(&data.assets_managers).map_err(polars_err)?, &out_dir.join("assets_managers.csv"))?;
    write_csv(&mut investments_managers_df(&data.investments_managers).map_err(polars_err)?, &out_dir.join("investments_managers_to_funds.csv"))?;
    write_csv(&mut funds_change_name_df(&data.funds_change_name).map_err(polars_err)?, &out_dir.join("funds_change_name.csv"))?;
    write_yaml_dicts(&data.additional_infos, &out_dir.join("investments_add_infos.yaml"), None)
}

fn polars_err(e: PolarsError) -> PyErr {
    PyValueError::new_err(format!("polars error: {e}"))
}

/// `_write_single_file`: investments left-joined with `additional_infos` on `ID`, bond-only
/// columns renamed to their CSV aliases (`maturity` -> `Maturity`, `interest_rate` -> `Interest
/// rate`), written as one CSV.
fn write_single_file(data: &TransformedTables, out_path: &Path) -> PyResult<()> {
    let mut instruments = investments_df(&data.investments).map_err(polars_err)?;
    let ids: Vec<u32> = data.investments.iter().map(|r| r.id).collect();
    let maturities = opt_date_col(ids.iter().map(|id| data.additional_infos.get(id).and_then(|i| i.maturity)));
    let interest_rates: Vec<Option<f64>> = ids.iter().map(|id| data.additional_infos.get(id).and_then(|i| i.interest_rate)).collect();
    instruments
        .with_column(Series::new("Maturity".into(), maturities))
        .map_err(polars_err)?;
    instruments
        .with_column(Series::new("Interest rate".into(), interest_rates))
        .map_err(polars_err)?;
    write_csv(&mut instruments, out_path)
}

/// `_write_structured`: only `investments` + `additional_infos`, in a `investments/` subdirectory
/// (`table.csv` + `dicts.yaml`), matching the Python original's fixed `data_name="investments"`.
fn write_structured(data: &TransformedTables, out_dir: &Path) -> PyResult<()> {
    std::fs::create_dir_all(out_dir).map_err(|e| PyValueError::new_err(format!("cannot create {}: {e}", out_dir.display())))?;
    let sub = out_dir.join("investments");
    std::fs::create_dir_all(&sub).map_err(|e| PyValueError::new_err(format!("cannot create {}: {e}", sub.display())))?;
    write_csv(&mut investments_df(&data.investments).map_err(polars_err)?, &sub.join("table.csv"))?;
    write_yaml_dicts(&data.additional_infos, &sub.join("dicts.yaml"), None)
}

fn compress_single_file(path: &Path) -> PyResult<()> {
    let archive_name = format!("{}.gz", path.file_name().and_then(|n| n.to_str()).unwrap_or_default());
    let archive_path = path.with_file_name(archive_name);
    let mut input = File::open(path).map_err(|e| PyValueError::new_err(format!("cannot open {}: {e}", path.display())))?;
    let output = File::create(&archive_path).map_err(|e| PyValueError::new_err(format!("cannot create {}: {e}", archive_path.display())))?;
    let mut encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
    std::io::copy(&mut input, &mut encoder).map_err(|e| PyValueError::new_err(format!("cannot gzip {}: {e}", path.display())))?;
    encoder.finish().map_err(|e| PyValueError::new_err(format!("cannot finish gzip {}: {e}", path.display())))?;
    Ok(())
}

fn compress_directory(dir: &Path) -> PyResult<()> {
    let archive_name = format!("{}.tar.gz", dir.file_name().and_then(|n| n.to_str()).unwrap_or_default());
    let archive_path = dir.with_file_name(archive_name);
    let output = File::create(&archive_path).map_err(|e| PyValueError::new_err(format!("cannot create {}: {e}", archive_path.display())))?;
    let encoder = flate2::write::GzEncoder::new(output, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let arcname = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    builder
        .append_dir_all(arcname, dir)
        .map_err(|e| PyValueError::new_err(format!("cannot tar {}: {e}", dir.display())))?;
    builder.into_inner().and_then(|mut e| e.flush()).map_err(|e| PyValueError::new_err(format!("cannot finish tar.gz: {e}")))?;
    Ok(())
}

/// Rust port of `write_files`. `profile`/`flags` stay Python `Enum`/`Flag` objects
/// (`cli/conf_parse.py`'s `OutStructureNormalMode`/`OutStructureBatchMode`/`OutFlagsNormalMode`/
/// `OutFlagsBatchMode`) — read generically via `.name`/`__contains__` rather than importing those
/// 4 classes here, since `REGULAR`/`SINGLE_FILE`/`STRUCTURED`/`COMPRESSED` are shared member names
/// across both Normal/Batch variants (verified in `conf_parse.py`), so there's nothing
/// mode-specific for this function to actually branch on beyond what's already generic.
#[pyfunction]
#[pyo3(name = "write_files")]
pub fn py_write_files(data: &TransformedTables, out_path: PathBuf, profile: &Bound<'_, PyAny>, flags: &Bound<'_, PyAny>) -> PyResult<()> {
    let profile_name: String = profile.getattr("name")?.extract()?;
    let remove_uncompressed_out = !out_path.exists();

    match profile_name.as_str() {
        "REGULAR" => write_regular(data, &out_path)?,
        "SINGLE_FILE" => write_single_file(data, &out_path)?,
        "STRUCTURED" => write_structured(data, &out_path)?,
        other => return Err(PyValueError::new_err(format!("Profile {other} not known"))),
    }

    let compressed_member = flags.get_type().getattr("COMPRESSED")?;
    let is_compressed = flags.contains(&compressed_member)?;
    if is_compressed {
        if profile_name == "SINGLE_FILE" {
            compress_single_file(&out_path)?;
            if remove_uncompressed_out {
                std::fs::remove_file(&out_path).map_err(|e| PyValueError::new_err(format!("cannot remove {}: {e}", out_path.display())))?;
            }
        } else {
            compress_directory(&out_path)?;
            if remove_uncompressed_out {
                std::fs::remove_dir_all(&out_path).map_err(|e| PyValueError::new_err(format!("cannot remove {}: {e}", out_path.display())))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::promise::Promise;
    use crate::output::classes::fund_assets::FundAssets;
    use crate::output::classes::fund_esg_indicator::FundEsgIndicator;
    use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
    use pyo3::types::PyList as PyListType;

    // -------------------------------------------------------------------------------------
    // Fixture builders — go through the real Python-visible `#[new]` constructor generically
    // (`py.get_type::<T>().call(...)`) rather than reaching for each pyclass's Rust-level `new`
    // associated function directly, most of which aren't `pub` outside their own module. This
    // also means these fixtures are built exactly the way real format-authoring code builds them.
    // -------------------------------------------------------------------------------------

    fn make_equity<'py>(py: Python<'py>, company: &str, company_match: &str, fund: &str, market_value: f64, currency: Currency) -> Bound<'py, PyAny> {
        let kwargs = PyDict::new(py);
        kwargs.set_item("company", company).unwrap();
        kwargs.set_item("company_match", company_match).unwrap();
        kwargs.set_item("fund", fund).unwrap();
        kwargs.set_item("market_value", market_value).unwrap();
        kwargs.set_item("currency", currency).unwrap();
        py.get_type::<Equity>().call((), Some(&kwargs)).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn make_bond<'py>(
        py: Python<'py>,
        company: &str,
        company_match: &str,
        fund: &str,
        market_value: f64,
        currency: Currency,
        maturity: Option<SimpleDate>,
        interest_rate: Option<f64>,
    ) -> Bound<'py, PyAny> {
        let kwargs = PyDict::new(py);
        kwargs.set_item("company", company).unwrap();
        kwargs.set_item("company_match", company_match).unwrap();
        kwargs.set_item("fund", fund).unwrap();
        kwargs.set_item("market_value", market_value).unwrap();
        kwargs.set_item("currency", currency).unwrap();
        if let Some(m) = maturity {
            kwargs.set_item("maturity", m).unwrap();
        }
        if let Some(r) = interest_rate {
            kwargs.set_item("interest_rate", r).unwrap();
        }
        py.get_type::<Bond>().call((), Some(&kwargs)).unwrap()
    }

    fn make_fund<'py>(py: Python<'py>, name: &str) -> Bound<'py, PyAny> {
        py.get_type::<Fund>().call1((name,)).unwrap()
    }

    fn make_management_company<'py>(py: Python<'py>, name: &str, managed_funds: &[&str]) -> Bound<'py, PyAny> {
        let funds = PyListType::new(py, managed_funds).unwrap();
        py.get_type::<ManagementCompany>().call1((name, funds)).unwrap()
    }

    fn make_investments_manager<'py>(py: Python<'py>, name: &str, managed_funds: &[&str]) -> Bound<'py, PyAny> {
        let funds = PyListType::new(py, managed_funds).unwrap();
        py.get_type::<InvestmentsManager>().call1((name, funds)).unwrap()
    }

    fn make_fund_rename<'py>(py: Python<'py>, old_name: &str, current_name: &str, date: SimpleDate) -> Bound<'py, PyAny> {
        py.get_type::<FundRename>().call1((old_name, current_name, date)).unwrap()
    }

    fn make_fund_merge<'py>(py: Python<'py>, old_name: &str, current_name: &str, date: SimpleDate) -> Bound<'py, PyAny> {
        py.get_type::<FundMerge>().call1((old_name, current_name, date)).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn make_fund_assets<'py>(
        py: Python<'py>,
        fund: &str,
        tot_assets: f64,
        liabilities: f64,
        net_assets: f64,
        currency: Currency,
        date: Option<SimpleDate>,
    ) -> Bound<'py, PyAny> {
        let kwargs = PyDict::new(py);
        kwargs.set_item("fund", fund).unwrap();
        kwargs.set_item("tot_assets", tot_assets).unwrap();
        kwargs.set_item("liabilities", liabilities).unwrap();
        kwargs.set_item("net_assets", net_assets).unwrap();
        kwargs.set_item("currency", currency).unwrap();
        if let Some(d) = date {
            kwargs.set_item("date", d).unwrap();
        }
        py.get_type::<FundAssets>().call((), Some(&kwargs)).unwrap()
    }

    fn make_fund_sfdr<'py>(py: Python<'py>, fund: &str, article: SfdrArticle) -> Bound<'py, PyAny> {
        py.get_type::<FundSfdrClassification>().call1((fund, article)).unwrap()
    }

    fn make_fund_esg<'py>(py: Python<'py>, fund: &str, indicator: &str, value: &str) -> Bound<'py, PyAny> {
        py.get_type::<FundEsgIndicator>().call1((fund, indicator, value)).unwrap()
    }

    /// Assembles a `DocumentResults` out of `(page_number, items)` pairs, dispatching each item
    /// into the right `PageResults` sub-list by type — mirrors `cli/main.py`'s own dispatch loop.
    fn build_document<'py>(py: Python<'py>, report_name: &str, algorithm: &str, pages: Vec<(i64, Vec<Bound<'py, PyAny>>)>) -> Py<DocumentResults> {
        let doc = Py::new(py, DocumentResults::new(py, report_name.to_string(), algorithm.to_string()).unwrap()).unwrap();
        let doc_ref = doc.bind(py).borrow();
        for (page_n, items) in pages {
            let pr = Py::new(py, PageResults::new(py).unwrap()).unwrap();
            pr.bind(py).borrow_mut().page_number = page_n;
            {
                let pr_ref = pr.bind(py).borrow();
                for item in items {
                    if item.is_instance_of::<Equity>() || item.is_instance_of::<Bond>() {
                        pr_ref.investments.bind(py).append(&item).unwrap();
                    } else if item.is_instance_of::<ManagementCompany>() || item.is_instance_of::<InvestmentsManager>() {
                        pr_ref.assets_managers.bind(py).append(&item).unwrap();
                    } else if item.is_instance_of::<Fund>() {
                        pr_ref.funds.bind(py).append(&item).unwrap();
                    } else if item.is_instance_of::<FundSfdrClassification>() {
                        pr_ref.funds_sfdr_classification.bind(py).append(&item).unwrap();
                    } else if item.is_instance_of::<FundEsgIndicator>() {
                        pr_ref.funds_esg_indicators.bind(py).append(&item).unwrap();
                    } else if item.is_instance_of::<FundAssets>() {
                        pr_ref.funds_assets.bind(py).append(&item).unwrap();
                    } else if item.is_instance_of::<FundRename>() || item.is_instance_of::<FundMerge>() {
                        pr_ref.funds_change_name.bind(py).append(&item).unwrap();
                    } else {
                        panic!("build_document: unrecognized fixture type {}", item.get_type());
                    }
                }
            }
            doc_ref.results.bind(py).append(pr).unwrap();
        }
        drop(doc_ref);
        doc
    }

    fn transform<'py>(py: Python<'py>, docs: Vec<Py<DocumentResults>>) -> TransformedTables {
        let list = PyListType::new(py, docs).unwrap();
        py_transform_to_files_schema(py, &list, false).unwrap()
    }

    // -------------------------------------------------------------------------------------
    // PageResults / DocumentResults
    // -------------------------------------------------------------------------------------

    #[test]
    fn page_results_starts_empty_with_page_number_zero() {
        Python::attach(|py| {
            let pr = PageResults::new(py).unwrap();
            assert_eq!(pr.page_number, 0);
            assert_eq!(pr.investments.bind(py).len(), 0);
        });
    }

    #[test]
    fn document_results_getitem_is_one_based() {
        Python::attach(|py| {
            let doc = build_document(py, "R", "F", vec![(1, vec![]), (2, vec![])]);
            let first = doc.bind(py).borrow().__getitem__(py, 1).unwrap();
            let second = doc.bind(py).borrow().__getitem__(py, 2).unwrap();
            assert!(!first.is(&second));
        });
    }

    #[test]
    fn document_results_getitem_rejects_page_zero() {
        Python::attach(|py| {
            let doc = build_document(py, "R", "F", vec![(1, vec![])]);
            assert!(doc.bind(py).borrow().__getitem__(py, 0).is_err());
        });
    }

    #[test]
    fn document_results_add_report_infos_sets_both_keys() {
        Python::attach(|py| {
            let doc = DocumentResults::new(py, "MyReport".into(), "MyFormat".into()).unwrap();
            let d = PyDict::new(py);
            doc.add_report_infos(&d).unwrap();
            assert_eq!(d.get_item("Report").unwrap().unwrap().extract::<String>().unwrap(), "MyReport");
            assert_eq!(d.get_item("Format").unwrap().unwrap().extract::<String>().unwrap(), "MyFormat");
        });
    }

    #[test]
    fn fulfill_promises_resolves_a_single_valued_promise_in_place() {
        Python::attach(|py| {
            let promise = Promise::from_parts("f", false, false).into_pyobject(py).unwrap().into_any();
            let fund = py.get_type::<Fund>().call1((promise,)).unwrap();
            let doc = build_document(py, "R", "F", vec![(1, vec![fund])]);
            let mapping = PyDict::new(py);
            mapping.set_item("f", "Resolved Name").unwrap();
            doc.bind(py).borrow().fulfill_promises(py, &mapping).unwrap();
            let pr = doc.bind(py).borrow().results.bind(py).get_item(0).unwrap();
            let pr = pr.cast::<PageResults>().unwrap();
            let funds = pr.borrow().funds.bind(py).clone();
            assert_eq!(funds.len(), 1);
        });
    }

    #[test]
    fn fulfill_promises_leaves_entry_unexpanded_when_its_multiple_promise_resolves_to_an_empty_list() {
        // `core::promisable::fulfill_promises` treats an empty resolved list for a `multiple`
        // field as "nothing to expand" (skips that field), not as "drop the entity" — that drop
        // path is only taken when a *non-multiple* promise's key is missing entirely. So the
        // single Fund here survives, still carrying its unresolved `name` promise.
        Python::attach(|py| {
            let promise = Promise::from_parts("g", false, true).into_pyobject(py).unwrap().into_any();
            let fund = py.get_type::<Fund>().call1((promise,)).unwrap();
            let doc = build_document(py, "R", "F", vec![(1, vec![fund])]);
            let mapping = PyDict::new(py);
            mapping.set_item("g", PyListType::empty(py)).unwrap();
            doc.bind(py).borrow().fulfill_promises(py, &mapping).unwrap();
            let pr = doc.bind(py).borrow().results.bind(py).get_item(0).unwrap();
            let pr = pr.cast::<PageResults>().unwrap();
            assert_eq!(pr.borrow().funds.bind(py).len(), 1);
        });
    }

    #[test]
    fn fulfill_promises_expands_a_multiple_promise_into_several_entries() {
        Python::attach(|py| {
            let promise = Promise::from_parts("h", false, true).into_pyobject(py).unwrap().into_any();
            let fund = py.get_type::<Fund>().call1((promise,)).unwrap();
            let doc = build_document(py, "R", "F", vec![(1, vec![fund])]);
            let mapping = PyDict::new(py);
            mapping.set_item("h", PyListType::new(py, ["Name A", "Name B"]).unwrap()).unwrap();
            doc.bind(py).borrow().fulfill_promises(py, &mapping).unwrap();
            let pr = doc.bind(py).borrow().results.bind(py).get_item(0).unwrap();
            let pr = pr.cast::<PageResults>().unwrap();
            assert_eq!(pr.borrow().funds.bind(py).len(), 2);
        });
    }

    #[test]
    fn fulfill_promises_drops_an_entry_whose_promise_key_is_entirely_missing() {
        Python::attach(|py| {
            let promise = Promise::from_parts("missing", false, false).into_pyobject(py).unwrap().into_any();
            let fund = py.get_type::<Fund>().call1((promise,)).unwrap();
            let doc = build_document(py, "R", "F", vec![(1, vec![fund])]);
            let mapping = PyDict::new(py); // "missing" is not a key here → KeyError → dropped.
            doc.bind(py).borrow().fulfill_promises(py, &mapping).unwrap();
            let pr = doc.bind(py).borrow().results.bind(py).get_item(0).unwrap();
            let pr = pr.cast::<PageResults>().unwrap();
            assert_eq!(pr.borrow().funds.bind(py).len(), 0);
        });
    }

    // -------------------------------------------------------------------------------------
    // transform_to_files_schema: investments / bond additional infos
    // -------------------------------------------------------------------------------------

    #[test]
    fn single_equity_investment_gets_id_one_and_creates_its_fund() {
        Python::attach(|py| {
            let equity = make_equity(py, "Some Corp", "SOME CORP", "Growth Fund", 1000.0, Currency::EUR);
            let doc = build_document(py, "Report1", "FORMAT-A", vec![(3, vec![equity])]);
            let tables = transform(py, vec![doc]);

            assert_eq!(tables.investments.len(), 1);
            let row = &tables.investments[0];
            assert_eq!(row.id, 1);
            assert_eq!(row.report_page, 3);
            assert_eq!(row.report, "Report1");
            assert_eq!(row.format, "FORMAT-A");
            assert_eq!(row.financial_instrument, FinancialInstrument::EQUITY);
            assert_eq!(row.fund_id, 1);
            assert_eq!(tables.funds.len(), 1);
            assert_eq!(tables.funds[0].name, "GROWTH FUND");
            assert!(tables.funds[0].report_page.is_none(), "fund only seen indirectly via an investment");
        });
    }

    #[test]
    fn bond_investment_splits_maturity_and_interest_rate_into_additional_infos() {
        Python::attach(|py| {
            let bond = make_bond(py, "Some Corp", "SOME CORP", "Bond Fund", 500.0, Currency::USD, Some(SimpleDate { year: 2030, month: 6, day: 15 }), Some(0.05));
            let doc = build_document(py, "Report1", "FORMAT-A", vec![(1, vec![bond])]);
            let tables = transform(py, vec![doc]);

            assert_eq!(tables.investments.len(), 1);
            let row = &tables.investments[0];
            assert_eq!(row.financial_instrument, FinancialInstrument::BOND);
            let info = tables.additional_infos.get(&row.id).unwrap();
            assert_eq!(info.maturity, Some(SimpleDate { year: 2030, month: 6, day: 15 }));
            assert_eq!(info.interest_rate, Some(0.05));
        });
    }

    #[test]
    fn two_investments_get_sequential_ids() {
        Python::attach(|py| {
            let e1 = make_equity(py, "A", "A", "Fund X", 100.0, Currency::EUR);
            let e2 = make_equity(py, "B", "B", "Fund X", 200.0, Currency::EUR);
            let doc = build_document(py, "R", "F", vec![(1, vec![e1, e2])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.investments[0].id, 1);
            assert_eq!(tables.investments[1].id, 2);
            // Same fund referenced twice → still exactly one Fund row.
            assert_eq!(tables.funds.len(), 1);
        });
    }

    #[test]
    fn none_entries_in_investments_are_skipped() {
        Python::attach(|py| {
            let doc = Py::new(py, DocumentResults::new(py, "R".into(), "F".into()).unwrap()).unwrap();
            let pr = Py::new(py, PageResults::new(py).unwrap()).unwrap();
            pr.bind(py).borrow_mut().page_number = 1;
            pr.bind(py).borrow().investments.bind(py).append(py.None()).unwrap();
            doc.bind(py).borrow().results.bind(py).append(pr).unwrap();

            let tables = transform(py, vec![doc]);
            assert!(tables.investments.is_empty());
            assert!(tables.funds.is_empty());
        });
    }

    // -------------------------------------------------------------------------------------
    // Fund debug-info stamping (the one row type with genuinely optional Report page/etc.)
    // -------------------------------------------------------------------------------------

    #[test]
    fn fund_seen_directly_and_indirectly_on_the_same_page_gets_debug_info_once() {
        Python::attach(|py| {
            let fund = make_fund(py, "Growth Fund");
            let equity = make_equity(py, "A", "A", "Growth Fund", 100.0, Currency::EUR);
            let doc = build_document(py, "R", "F", vec![(5, vec![fund, equity])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.funds.len(), 1);
            assert_eq!(tables.funds[0].report_page, Some(5));
        });
    }

    #[test]
    fn fund_created_indirectly_then_seen_directly_on_a_later_page_gets_stamped_retroactively() {
        Python::attach(|py| {
            let equity = make_equity(py, "A", "A", "Growth Fund", 100.0, Currency::EUR);
            let fund = make_fund(py, "Growth Fund");
            let doc = build_document(py, "R", "F", vec![(1, vec![equity]), (2, vec![fund])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.funds.len(), 1);
            assert_eq!(tables.funds[0].report_page, Some(2));
        });
    }

    #[test]
    fn fund_never_seen_directly_has_no_debug_info_at_all() {
        Python::attach(|py| {
            let equity = make_equity(py, "A", "A", "Growth Fund", 100.0, Currency::EUR);
            let doc = build_document(py, "R", "F", vec![(1, vec![equity])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.funds[0].report_page, None);
            assert_eq!(tables.funds[0].report, None);
            assert_eq!(tables.funds[0].format, None);
        });
    }

    // -------------------------------------------------------------------------------------
    // assets_managers / management-company / investments-manager wiring
    // -------------------------------------------------------------------------------------

    #[test]
    fn management_company_assigns_management_company_id_to_its_managed_funds() {
        Python::attach(|py| {
            let mc = make_management_company(py, "BlackRock", &["Growth Fund", "Value Fund"]);
            let doc = build_document(py, "R", "F", vec![(1, vec![mc])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.assets_managers.len(), 1);
            let manager_id = tables.assets_managers[0].id;
            assert_eq!(tables.funds.len(), 2);
            for fund in &tables.funds {
                assert_eq!(fund.management_company_id, Some(manager_id));
            }
        });
    }

    #[test]
    fn investments_manager_appends_a_row_per_managed_fund() {
        Python::attach(|py| {
            let im = make_investments_manager(py, "Fidelity", &["Fund A", "Fund B"]);
            let doc = build_document(py, "R", "F", vec![(1, vec![im])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.investments_managers.len(), 2);
            assert!(tables.funds.iter().all(|f| f.management_company_id.is_none()));
        });
    }

    #[test]
    fn same_manager_name_across_pages_reuses_the_same_assets_manager_row() {
        Python::attach(|py| {
            let mc1 = make_management_company(py, "BlackRock", &["Fund A"]);
            let mc2 = make_management_company(py, "BlackRock", &["Fund B"]);
            let doc = build_document(py, "R", "F", vec![(1, vec![mc1]), (2, vec![mc2])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.assets_managers.len(), 1);
            assert_eq!(tables.funds.len(), 2);
        });
    }

    #[test]
    fn none_entries_in_assets_managers_are_skipped() {
        Python::attach(|py| {
            let doc = Py::new(py, DocumentResults::new(py, "R".into(), "F".into()).unwrap()).unwrap();
            let pr = Py::new(py, PageResults::new(py).unwrap()).unwrap();
            pr.bind(py).borrow_mut().page_number = 1;
            pr.bind(py).borrow().assets_managers.bind(py).append(py.None()).unwrap();
            doc.bind(py).borrow().results.bind(py).append(pr).unwrap();

            let tables = transform(py, vec![doc]);
            assert!(tables.assets_managers.is_empty());
        });
    }

    // -------------------------------------------------------------------------------------
    // funds_change_name (Renaming vs. Merging) + its combo uniqueness
    // -------------------------------------------------------------------------------------

    #[test]
    fn fund_rename_produces_a_renaming_row() {
        Python::attach(|py| {
            let d = SimpleDate { year: 2024, month: 1, day: 1 };
            let rename = make_fund_rename(py, "Old Fund", "New Fund", d);
            let doc = build_document(py, "R", "F", vec![(1, vec![rename])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.funds_change_name.len(), 1);
            assert_eq!(tables.funds_change_name[0].event_type, ChangeNameEventType::Renaming);
            assert_eq!(tables.funds_change_name[0].old_name, "Old Fund");
        });
    }

    #[test]
    fn fund_merge_produces_a_merging_row() {
        Python::attach(|py| {
            let d = SimpleDate { year: 2024, month: 1, day: 1 };
            let merge = make_fund_merge(py, "Absorbed Fund", "Surviving Fund", d);
            let doc = build_document(py, "R", "F", vec![(1, vec![merge])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.funds_change_name[0].event_type, ChangeNameEventType::Merging);
        });
    }

    #[test]
    fn identical_change_name_events_on_different_pages_are_rejected_as_duplicates() {
        Python::attach(|py| {
            let d = SimpleDate { year: 2024, month: 1, day: 1 };
            let r1 = make_fund_rename(py, "Old Fund", "New Fund", d);
            let r2 = make_fund_rename(py, "Old Fund", "New Fund", d);
            let doc = build_document(py, "R", "F", vec![(1, vec![r1]), (2, vec![r2])]);
            let list = PyListType::new(py, [doc]).unwrap();
            let err = py_transform_to_files_schema(py, &list, false).unwrap_err();
            assert!(err.to_string().contains("funds_change_name"));
        });
    }

    #[test]
    fn change_name_events_differing_only_by_date_are_not_duplicates() {
        Python::attach(|py| {
            let r1 = make_fund_rename(py, "Old Fund", "New Fund", SimpleDate { year: 2024, month: 1, day: 1 });
            let r2 = make_fund_rename(py, "Old Fund", "New Fund", SimpleDate { year: 2024, month: 2, day: 1 });
            let doc = build_document(py, "R", "F", vec![(1, vec![r1, r2])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.funds_change_name.len(), 2);
        });
    }

    // -------------------------------------------------------------------------------------
    // funds_assets: nullable date + combo uniqueness
    // -------------------------------------------------------------------------------------

    #[test]
    fn fund_assets_row_created_with_null_date() {
        Python::attach(|py| {
            let fa = make_fund_assets(py, "Fund A", 100.0, 40.0, 60.0, Currency::EUR, None);
            let doc = build_document(py, "R", "F", vec![(1, vec![fa])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.funds_assets.len(), 1);
            assert_eq!(tables.funds_assets[0].date, None);
        });
    }

    #[test]
    fn fund_assets_same_fund_and_date_twice_is_a_duplicate() {
        Python::attach(|py| {
            let d = SimpleDate { year: 2024, month: 3, day: 31 };
            let fa1 = make_fund_assets(py, "Fund A", 100.0, 40.0, 60.0, Currency::EUR, Some(d));
            let fa2 = make_fund_assets(py, "Fund A", 100.0, 40.0, 60.0, Currency::EUR, Some(d));
            let doc = build_document(py, "R", "F", vec![(1, vec![fa1, fa2])]);
            let list = PyListType::new(py, [doc]).unwrap();
            assert!(py_transform_to_files_schema(py, &list, false).is_err());
        });
    }

    #[test]
    fn fund_assets_same_fund_different_date_is_not_a_duplicate() {
        Python::attach(|py| {
            let fa1 = make_fund_assets(py, "Fund A", 100.0, 40.0, 60.0, Currency::EUR, Some(SimpleDate { year: 2024, month: 3, day: 31 }));
            let fa2 = make_fund_assets(py, "Fund A", 100.0, 40.0, 60.0, Currency::EUR, Some(SimpleDate { year: 2024, month: 6, day: 30 }));
            let doc = build_document(py, "R", "F", vec![(1, vec![fa1, fa2])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.funds_assets.len(), 2);
        });
    }

    // -------------------------------------------------------------------------------------
    // funds_sfdr_classification: Fund ID uniqueness
    // -------------------------------------------------------------------------------------

    #[test_case::test_case(SfdrArticle::ART_6; "article 6")]
    #[test_case::test_case(SfdrArticle::ART_8; "article 8")]
    #[test_case::test_case(SfdrArticle::ART_9; "article 9")]
    fn fund_sfdr_classification_row_records_every_article(article: SfdrArticle) {
        Python::attach(|py| {
            let fsc = make_fund_sfdr(py, "Fund A", article);
            let doc = build_document(py, "R", "F", vec![(1, vec![fsc])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.funds_sfdr_classification[0].sfdr_classification, article);
        });
    }

    #[test]
    fn two_sfdr_classifications_for_the_same_fund_are_rejected() {
        Python::attach(|py| {
            let fsc1 = make_fund_sfdr(py, "Fund A", SfdrArticle::ART_6);
            let fsc2 = make_fund_sfdr(py, "Fund A", SfdrArticle::ART_8);
            let doc = build_document(py, "R", "F", vec![(1, vec![fsc1]), (2, vec![fsc2])]);
            let list = PyListType::new(py, [doc]).unwrap();
            let err = py_transform_to_files_schema(py, &list, false).unwrap_err();
            assert!(err.to_string().contains("funds_sfdr_classification"));
        });
    }

    // -------------------------------------------------------------------------------------
    // funds_esg_indicators: Fund ID is *not* unique (a fund may have several indicators)
    // -------------------------------------------------------------------------------------

    #[test]
    fn multiple_esg_indicators_for_the_same_fund_are_all_kept() {
        Python::attach(|py| {
            let a = make_fund_esg(py, "Fund A", "GHG intensity", "12.3");
            let b = make_fund_esg(py, "Fund A", "Board gender diversity", "45%");
            let doc = build_document(py, "R", "F", vec![(1, vec![a, b])]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.funds_esg_indicators.len(), 2);
            assert_eq!(tables.funds.len(), 1, "both indicators reference the same fund");
        });
    }

    // -------------------------------------------------------------------------------------
    // investments_managers combo uniqueness
    // -------------------------------------------------------------------------------------

    #[test]
    fn same_manager_and_fund_pair_twice_is_a_duplicate() {
        Python::attach(|py| {
            let im1 = make_investments_manager(py, "Fidelity", &["Fund A"]);
            let im2 = make_investments_manager(py, "Fidelity", &["Fund A"]);
            let doc = build_document(py, "R", "F", vec![(1, vec![im1]), (2, vec![im2])]);
            let list = PyListType::new(py, [doc]).unwrap();
            let err = py_transform_to_files_schema(py, &list, false).unwrap_err();
            assert!(err.to_string().contains("investments_managers"));
        });
    }

    // -------------------------------------------------------------------------------------
    // Cross-document accumulation + a reasonable stress test
    // -------------------------------------------------------------------------------------

    #[test]
    fn ids_stay_sequential_across_multiple_documents() {
        Python::attach(|py| {
            let e1 = make_equity(py, "A", "A", "Fund X", 100.0, Currency::EUR);
            let doc1 = build_document(py, "R1", "F", vec![(1, vec![e1])]);
            let e2 = make_equity(py, "B", "B", "Fund Y", 200.0, Currency::EUR);
            let doc2 = build_document(py, "R2", "F", vec![(1, vec![e2])]);
            let tables = transform(py, vec![doc1, doc2]);
            assert_eq!(tables.investments[0].id, 1);
            assert_eq!(tables.investments[1].id, 2);
            assert_eq!(tables.funds.len(), 2);
        });
    }

    #[test]
    fn stress_1000_investments_across_100_funds_get_unique_sequential_ids() {
        Python::attach(|py| {
            let mut items = Vec::with_capacity(1000);
            for i in 0..1000 {
                let fund_name = format!("Fund {}", i % 100);
                items.push(make_equity(py, "Company", "Company", &fund_name, 100.0 + i as f64, Currency::EUR));
            }
            let doc = build_document(py, "R", "F", vec![(1, items)]);
            let tables = transform(py, vec![doc]);
            assert_eq!(tables.investments.len(), 1000);
            let ids: std::collections::HashSet<u32> = tables.investments.iter().map(|r| r.id).collect();
            assert_eq!(ids.len(), 1000);
            assert_eq!(tables.funds.len(), 100);
        });
    }

    // -------------------------------------------------------------------------------------
    // write_files
    // -------------------------------------------------------------------------------------

    fn out_profile<'py>(py: Python<'py>, name: &str) -> Bound<'py, PyAny> {
        py.import("freeports._internals.cli.conf_parse")
            .unwrap()
            .getattr("OutStructureNormalMode")
            .unwrap()
            .getattr(name)
            .unwrap()
    }

    fn out_flags<'py>(py: Python<'py>, compressed: bool) -> Bound<'py, PyAny> {
        let cls = py.import("freeports._internals.cli.conf_parse").unwrap().getattr("OutFlagsNormalMode").unwrap();
        if compressed {
            cls.getattr("COMPRESSED").unwrap()
        } else {
            cls.call1((0,)).unwrap()
        }
    }

    fn sample_tables(py: Python<'_>) -> TransformedTables {
        let equity = make_equity(py, "Corp A", "CORP A", "Growth Fund", 100.0, Currency::EUR);
        let bond = make_bond(py, "Corp B", "CORP B", "Bond Fund", 200.0, Currency::USD, Some(SimpleDate { year: 2030, month: 1, day: 1 }), Some(0.03));
        let doc = build_document(py, "Report1", "FORMAT-A", vec![(1, vec![equity, bond])]);
        transform(py, vec![doc])
    }

    #[test]
    fn write_regular_creates_every_expected_file() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let tables = sample_tables(py);
            let dir = tempfile::tempdir().unwrap();
            let out_path = dir.path().join("out");
            py_write_files(&tables, out_path.clone(), &out_profile(py, "REGULAR"), &out_flags(py, false)).unwrap();

            for name in [
                "investments.csv",
                "funds_assets.csv",
                "funds.csv",
                "funds_sfdr_classification.csv",
                "funds_esg_indicators.csv",
                "assets_managers.csv",
                "investments_managers_to_funds.csv",
                "funds_change_name.csv",
                "investments_add_infos.yaml",
            ] {
                assert!(out_path.join(name).exists(), "missing {name}");
            }
            let investments_csv = std::fs::read_to_string(out_path.join("investments.csv")).unwrap();
            assert!(investments_csv.contains("EQUITY"));
            assert!(investments_csv.contains("BOND"));
            let yaml = std::fs::read_to_string(out_path.join("investments_add_infos.yaml")).unwrap();
            assert!(yaml.contains("interest_rate"));
        });
    }

    #[test]
    fn write_yaml_dicts_matches_pyyaml_exactly_quoted_date_sorted_keys_python_float_repr() {
        // Pins the exact PyYAML-`yaml.dump` output shape a real fixture comparison requires:
        // (1) keys sorted alphabetically (`interest_rate` before `maturity`, not declaration
        // order), (2) the date single-quoted so it reloads as a `str`, not `!!timestamp`, (3) a
        // whole-number float keeping its trailing `.0` (Rust's bare `Display` would drop it).
        // Regression coverage for the `investments_add_infos.yaml mismatch` the full
        // `analysis_finance_reports_formats` suite caught twice while porting this function.
        Python::attach(|py| {
            let mut infos = HashMap::new();
            infos.insert(2u32, BondAdditionalInfoRow { maturity: Some(SimpleDate { year: 2028, month: 3, day: 30 }), interest_rate: Some(0.5) });
            infos.insert(1u32, BondAdditionalInfoRow { maturity: None, interest_rate: None });
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("add_infos.yaml");
            write_yaml_dicts(&infos, &path, None).unwrap();
            let content = std::fs::read_to_string(&path).unwrap();
            assert_eq!(content, "1:\n  interest_rate: null\n  maturity: null\n2:\n  interest_rate: 0.5\n  maturity: '2028-03-30'\n");

            // And it must round-trip through Python's own `yaml.safe_load` as the same dict a
            // real fixture (produced by `yaml.dump`) would compare equal to.
            let yaml_mod = py.import("yaml").unwrap();
            let loaded = yaml_mod.call_method1("safe_load", (content.as_str(),)).unwrap();
            let entry_2 = loaded.get_item(2).unwrap();
            let maturity: String = entry_2.get_item("maturity").unwrap().extract().unwrap();
            assert_eq!(maturity, "2028-03-30", "must reload as a str, not a datetime.date");
        });
    }

    #[test]
    fn write_yaml_dicts_writes_flow_style_empty_dict_when_there_are_no_bonds() {
        // Regression test: a format with no bond investments at all (e.g. FIDEURAM-IT24) has an
        // empty `additional_infos` map. PyYAML's `yaml.dump({})` writes the literal `"{}\n"`; a
        // bare loop over zero entries would instead write 0 bytes — not valid YAML, and not what
        // the real fixture (produced by `yaml.dump`) contains. Caught by the full
        // `analysis_finance_reports_formats` suite, not by any of this module's other tests
        // (which all exercise at least one bond).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("add_infos.yaml");
        write_yaml_dicts(&HashMap::new(), &path, None).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}\n");
    }

    #[test]
    fn write_single_file_merges_bond_infos_and_leaves_equity_columns_null() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let tables = sample_tables(py);
            let dir = tempfile::tempdir().unwrap();
            let out_path = dir.path().join("out.csv");
            py_write_files(&tables, out_path.clone(), &out_profile(py, "SINGLE_FILE"), &out_flags(py, false)).unwrap();

            let content = std::fs::read_to_string(&out_path).unwrap();
            assert!(content.contains("Maturity"));
            assert!(content.contains("Interest rate"));
            assert!(content.contains("2030-01-01"));
        });
    }

    #[test]
    fn write_structured_writes_investments_subdirectory() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let tables = sample_tables(py);
            let dir = tempfile::tempdir().unwrap();
            let out_path = dir.path().join("out");
            py_write_files(&tables, out_path.clone(), &out_profile(py, "STRUCTURED"), &out_flags(py, false)).unwrap();

            assert!(out_path.join("investments").join("table.csv").exists());
            assert!(out_path.join("investments").join("dicts.yaml").exists());
        });
    }

    #[test]
    fn write_files_rejects_an_unrecognized_profile() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let tables = sample_tables(py);
            let dir = tempfile::tempdir().unwrap();
            let out_path = dir.path().join("out");
            // A bare object with a `.name` that isn't one of the 3 known profiles.
            let bogus = py.eval(std::ffi::CString::new("type('P', (), {'name': 'WEIRD'})()").unwrap().as_c_str(), None, None).unwrap();
            let err = py_write_files(&tables, out_path, &bogus, &out_flags(py, false)).unwrap_err();
            assert!(err.to_string().contains("WEIRD"));
        });
    }

    #[test]
    fn write_regular_compressed_produces_tar_gz_and_removes_uncompressed_dir_when_it_did_not_preexist() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let tables = sample_tables(py);
            let dir = tempfile::tempdir().unwrap();
            let out_path = dir.path().join("out");
            py_write_files(&tables, out_path.clone(), &out_profile(py, "REGULAR"), &out_flags(py, true)).unwrap();

            assert!(dir.path().join("out.tar.gz").exists());
            assert!(!out_path.exists(), "uncompressed dir should be removed since it didn't preexist");
        });
    }

    #[test]
    fn write_regular_compressed_keeps_uncompressed_dir_when_it_preexisted() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let tables = sample_tables(py);
            let dir = tempfile::tempdir().unwrap();
            let out_path = dir.path().join("out");
            std::fs::create_dir_all(&out_path).unwrap();
            py_write_files(&tables, out_path.clone(), &out_profile(py, "REGULAR"), &out_flags(py, true)).unwrap();

            assert!(dir.path().join("out.tar.gz").exists());
            assert!(out_path.exists(), "uncompressed dir preexisted, so it must be kept");
        });
    }

    #[test]
    fn write_single_file_compressed_produces_gz_and_removes_uncompressed_file() {
        Python::attach(|py| {
            crate::test_support::ensure_freeports_imported(py);
            let tables = sample_tables(py);
            let dir = tempfile::tempdir().unwrap();
            let out_path = dir.path().join("out.csv");
            py_write_files(&tables, out_path.clone(), &out_profile(py, "SINGLE_FILE"), &out_flags(py, true)).unwrap();

            assert!(dir.path().join("out.csv.gz").exists());
            assert!(!out_path.exists());
        });
    }
}
