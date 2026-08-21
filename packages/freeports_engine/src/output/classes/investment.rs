//! Rust port of `Investment`/`Equity`/`Bond` in `output/classes_schema.py`.
//!
//! `Investment` is declared `ABC` in Python and never directly instantiated anywhere (verified
//! across `analysis_finance_reports_formats` and `freeports_core`); `Equity`/`Bond` are the only
//! concrete types and are never subclassed either — same situation as
//! `FundChangeName`/`FundRename`/`FundMerge` (`core/fund_change_name.rs`) and
//! `AssetsManager`/`ManagementCompany`/`InvestmentsManager` (`core/assets_manager.rs`). Unlike
//! those, `Bond` adds two extra fields (`maturity`, `interest_rate`) on top of every `Investment`
//! field, so `Equity`/`Bond` aren't identical-shape variants of one macro — they're two pyclasses
//! that each own an `InvestmentData` (the 9 shared fields + shared logic), with `Bond` adding its
//! own two on the side.
//!
//! **Deliberately not ported**: `Investment.__str__`/`Bond.__str__` (a multi-line, i18n-translated
//! human-readable dump). Verified unused anywhere outside `classes_schema.py` itself (no
//! `str(investment)`/f-string call site in either this repo or the formats repo) — porting an
//! i18n-heavy formatting method that nothing calls isn't worth the Rust-reaching-into-Python-`_()`
//! layering violation it would require. `__repr__` gets a plain (untranslated) equivalent instead,
//! for basic debuggability.
//!
//! `#[new]` here does NOT tolerate unrecognized keyword arguments. It briefly did (a
//! `#[pyo3(signature = (..., **_extra))]` catch-all mirroring Pydantic's default
//! `extra="ignore"`) after the full formats suite caught
//! `DeserializerInvestmentStandard` (`formats/utils/deserialize/standard_funcs.py`) passing a
//! stray `manco=...` kwarg that was never a real field here — dead leftover from a different
//! deserializer, silently swallowed by Pydantic for years. Root-caused and fixed on the Python
//! side instead (the dead `"manco": ...` key was deleted from that deserializer's `args` dict),
//! per explicit user direction: fix bugs like this at the root even when it means touching
//! Python, then remove whatever Rust-side compatibility scaffolding existed only to accommodate
//! them. `market_value` still has a `#[setter]` (see below) — that one's for a real, still-live
//! use case (`mediolanum_es24_b.py` rescaling a value after construction), not a dead-code
//! accommodation, so it stays.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::commons::consts::Currency;
use crate::core::promisable::{self, FloatConstraint, Promised};
use crate::core::promise::Promise;
use crate::core::py_date::SimpleDate;

#[derive(Debug, Clone, PartialEq)]
struct InvestmentData {
    company: String,
    company_match: String,
    fund: Promised<String>,
    nominal_quantity: Option<f64>,
    market_value: Promised<f64>,
    currency: Promised<Currency>,
    perc_net_assets: Option<Promised<f64>>,
    acquisition_cost: Option<Promised<f64>>,
    acquisition_currency: Option<Promised<Currency>>,
}

/// Extracts an `Option<f64>` (never `Promise`-typed) from an optional Python argument — `None`
/// (or the argument being omitted) maps to `None`, matching Pydantic's `Optional[...] = None`.
fn extract_opt_f64(ob: Option<&Bound<'_, PyAny>>, constraint: FloatConstraint) -> PyResult<Option<f64>> {
    ob.filter(|v| !v.is_none()).map(|v| promisable::extract_f64(v, constraint)).transpose()
}

/// Same as [`extract_opt_f64`] but for a `Promised<f64>` field (`Optional[PromisedX]`).
fn extract_opt_promised_f64(
    ob: Option<&Bound<'_, PyAny>>,
    constraint: FloatConstraint,
) -> PyResult<Option<Promised<f64>>> {
    ob.filter(|v| !v.is_none())
        .map(|v| promisable::extract_promised_f64(v, constraint))
        .transpose()
}

/// Same idea for a `Promised<Currency>` field.
fn extract_opt_promised_currency(ob: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Promised<Currency>>> {
    ob.filter(|v| !v.is_none()).map(promisable::extract_promised_currency).transpose()
}

impl InvestmentData {
    #[allow(clippy::too_many_arguments)]
    fn new(
        company: String,
        company_match: String,
        fund: &Bound<'_, PyAny>,
        nominal_quantity: Option<&Bound<'_, PyAny>>,
        market_value: &Bound<'_, PyAny>,
        currency: &Bound<'_, PyAny>,
        perc_net_assets: Option<&Bound<'_, PyAny>>,
        acquisition_cost: Option<&Bound<'_, PyAny>>,
        acquisition_currency: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            company,
            company_match,
            fund: promisable::extract_promised::<String>(fund)?,
            nominal_quantity: extract_opt_f64(nominal_quantity, FloatConstraint::Positive)?,
            market_value: promisable::extract_promised_f64(market_value, FloatConstraint::Positive)?,
            currency: promisable::extract_promised_currency(currency)?,
            perc_net_assets: extract_opt_promised_f64(perc_net_assets, FloatConstraint::UnitIntervalHalfOpen)?,
            acquisition_cost: extract_opt_promised_f64(acquisition_cost, FloatConstraint::Positive)?,
            acquisition_currency: extract_opt_promised_currency(acquisition_currency)?,
        })
    }

    fn pending_promises(&self) -> Vec<(&'static str, Promise)> {
        let mut out = Vec::new();
        if let Promised::Pending(p) = &self.fund {
            out.push(("fund", p.clone()));
        }
        if let Promised::Pending(p) = &self.market_value {
            out.push(("market_value", p.clone()));
        }
        if let Promised::Pending(p) = &self.currency {
            out.push(("currency", p.clone()));
        }
        if let Some(Promised::Pending(p)) = &self.perc_net_assets {
            out.push(("perc_net_assets", p.clone()));
        }
        if let Some(Promised::Pending(p)) = &self.acquisition_cost {
            out.push(("acquisition_cost", p.clone()));
        }
        if let Some(Promised::Pending(p)) = &self.acquisition_currency {
            out.push(("acquisition_currency", p.clone()));
        }
        out
    }

    /// Returns `true` if `field` was one of this struct's own fields (and has been resolved);
    /// `false` if the caller (`Bond`) needs to try its own extra fields instead.
    fn resolve_field(&mut self, py: Python<'_>, field: &'static str, value: Py<PyAny>) -> PyResult<bool> {
        match field {
            "fund" => {
                self.fund = Promised::Resolved(value.extract::<String>(py)?);
            }
            "market_value" => {
                self.market_value =
                    Promised::Resolved(promisable::extract_f64(value.bind(py), FloatConstraint::Positive)?);
            }
            "currency" => {
                self.currency = Promised::Resolved(Currency::new(value.bind(py))?);
            }
            "perc_net_assets" => {
                self.perc_net_assets = Some(Promised::Resolved(promisable::extract_f64(
                    value.bind(py),
                    FloatConstraint::UnitIntervalHalfOpen,
                )?));
            }
            "acquisition_cost" => {
                self.acquisition_cost =
                    Some(Promised::Resolved(promisable::extract_f64(value.bind(py), FloatConstraint::Positive)?));
            }
            "acquisition_currency" => {
                self.acquisition_currency = Some(Promised::Resolved(Currency::new(value.bind(py))?));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Populates `dict` with every `Investment` field under its CSV alias (`fund` excluded,
    /// matching `Field(exclude=True)`) — shared by `Equity`/`Bond`, which then add their own
    /// fields (`Bond` only) on top of what this returns.
    fn model_dump_into<'py>(&self, py: Python<'py>, dict: &Bound<'py, PyDict>) -> PyResult<()> {
        dict.set_item("Investee", &self.company)?;
        dict.set_item("Triggering text", &self.company_match)?;
        dict.set_item("Nominal/Quantity", self.nominal_quantity)?;
        match &self.market_value {
            Promised::Resolved(v) => dict.set_item("Market value", v)?,
            Promised::Pending(_) => return Err(unresolved_promise_error("market_value")),
        }
        match &self.currency {
            Promised::Resolved(c) => dict.set_item("Currency", c.code())?,
            Promised::Pending(_) => return Err(unresolved_promise_error("currency")),
        }
        dict.set_item("% net assets", resolved_or_none("perc_net_assets", &self.perc_net_assets)?)?;
        dict.set_item("Acquisition cost", resolved_or_none("acquisition_cost", &self.acquisition_cost)?)?;
        match &self.acquisition_currency {
            None => dict.set_item("Acquisition currency", py.None())?,
            Some(Promised::Resolved(c)) => dict.set_item("Acquisition currency", c.code())?,
            Some(Promised::Pending(_)) => return Err(unresolved_promise_error("acquisition_currency")),
        }
        Ok(())
    }
}

fn unresolved_promise_error(field: &str) -> PyErr {
    PyValueError::new_err(format!(
        "cannot model_dump an Investment with an unresolved '{field}' promise"
    ))
}

fn resolved_or_none(field: &str, value: &Option<Promised<f64>>) -> PyResult<Option<f64>> {
    match value {
        None => Ok(None),
        Some(Promised::Resolved(v)) => Ok(Some(*v)),
        Some(Promised::Pending(_)) => Err(unresolved_promise_error(field)),
    }
}

// PyO3's `#[pymethods]` macro parses the impl block's items at the token level before any
// nested `macro_rules!` invocation can expand, so the shared getters below can't be factored
// into a `macro_rules!` invoked *inside* each `#[pymethods] impl` block (tried; PyO3 rejects it
// with "macros cannot be used as items in #[pymethods] impl blocks"). They're duplicated
// verbatim between `Equity` and `Bond` instead — each is one line, not worth a bigger structural
// workaround (e.g. a shared non-`#[pymethods]` trait) for two call sites.

#[pyclass(module = "freeports_engine")]
#[derive(Debug, Clone, PartialEq)]
pub struct Equity {
    data: InvestmentData,
}

#[pymethods]
impl Equity {
    #[new]
    #[pyo3(signature = (
        *, company, company_match, fund, market_value, currency,
        nominal_quantity = None, perc_net_assets = None, acquisition_cost = None,
        acquisition_currency = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        company: String,
        company_match: String,
        fund: &Bound<'_, PyAny>,
        market_value: &Bound<'_, PyAny>,
        currency: &Bound<'_, PyAny>,
        nominal_quantity: Option<&Bound<'_, PyAny>>,
        perc_net_assets: Option<&Bound<'_, PyAny>>,
        acquisition_cost: Option<&Bound<'_, PyAny>>,
        acquisition_currency: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Ok(Self {
            data: InvestmentData::new(
                company, company_match, fund, nominal_quantity, market_value, currency,
                perc_net_assets, acquisition_cost, acquisition_currency,
            )?,
        })
    }

    #[getter]
    fn company(&self) -> &str {
        &self.data.company
    }

    #[getter]
    fn company_match(&self) -> &str {
        &self.data.company_match
    }

    #[getter]
    fn fund(&self) -> Promised<String> {
        self.data.fund.clone()
    }

    #[getter]
    fn nominal_quantity(&self) -> Option<f64> {
        self.data.nominal_quantity
    }

    #[getter]
    fn market_value(&self) -> Promised<f64> {
        self.data.market_value.clone()
    }

    /// Pydantic's default `BaseModel` fields are mutable (`PromisableDict.model_config` sets
    /// `validate_assignment=True`, revalidating on every assignment) — found necessary via the
    /// full formats suite: `mediolanum_es24_b.py` does `blk.market_value = blk.market_value *
    /// 1000` on a freshly deserialized `Bond`/`Equity`. Only `market_value` has a setter for
    /// now — it's the only field any real format code was found mutating after construction
    /// (checked via grep across both this repo and `analysis_finance_reports_formats`); the rest
    /// stay read-only until a concrete need for them surfaces the same way.
    #[setter]
    fn set_market_value(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.data.market_value = promisable::extract_promised_f64(value, FloatConstraint::Positive)?;
        Ok(())
    }

    #[getter]
    fn currency(&self) -> Promised<Currency> {
        self.data.currency.clone()
    }

    #[getter]
    fn perc_net_assets(&self) -> Option<Promised<f64>> {
        self.data.perc_net_assets.clone()
    }

    #[getter]
    fn acquisition_cost(&self) -> Option<Promised<f64>> {
        self.data.acquisition_cost.clone()
    }

    #[getter]
    fn acquisition_currency(&self) -> Option<Promised<Currency>> {
        self.data.acquisition_currency.clone()
    }

    #[classattr]
    fn __rust_model_fields__() -> (
        &'static str, &'static str, &'static str, &'static str, &'static str,
        &'static str, &'static str, &'static str, &'static str,
    ) {
        (
            "company", "company_match", "fund", "nominal_quantity", "market_value",
            "currency", "perc_net_assets", "acquisition_cost", "acquisition_currency",
        )
    }

    #[pyo3(signature = (*, mode = "python", by_alias = false))]
    fn model_dump<'py>(&self, py: Python<'py>, mode: &str, by_alias: bool) -> PyResult<Bound<'py, PyDict>> {
        if mode != "json" || !by_alias {
            return Err(PyValueError::new_err(
                "only model_dump(mode=\"json\", by_alias=True) is supported by this Rust port",
            ));
        }
        let dict = PyDict::new(py);
        self.data.model_dump_into(py, &dict)?;
        Ok(dict)
    }

    fn fulfill_promises(
        &mut self,
        py: Python<'_>,
        mapping: &Bound<'_, PyDict>,
    ) -> PyResult<Option<Vec<Py<Self>>>> {
        let expansions = promisable::fulfill_promises(self, py, mapping)?;
        match expansions {
            None => Ok(None),
            Some(clones) => clones.into_iter().map(|f| Py::new(py, f)).collect::<PyResult<Vec<_>>>().map(Some),
        }
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        let dict = PyDict::new(py);
        self.data.model_dump_into(py, &dict)?;
        promisable::hash_via_model_dump_items(py, &dict)
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.extract::<PyRef<'_, Self>>().map(|o| *self == *o).unwrap_or(false)
    }

    fn __repr__(&self) -> String {
        format!("Equity(company={:?}, market_value={:?})", self.data.company, self.data.market_value)
    }
}

impl crate::core::promisable::PromisableFields for Equity {
    fn pending_promises(&self) -> Vec<(&'static str, Promise)> {
        self.data.pending_promises()
    }

    fn resolve_field(&mut self, py: Python<'_>, field: &'static str, value: Py<PyAny>) -> PyResult<()> {
        if self.data.resolve_field(py, field, value)? {
            Ok(())
        } else {
            unreachable!("Equity has no promisable field {field:?}")
        }
    }
}

#[pyclass(module = "freeports_engine")]
#[derive(Debug, Clone, PartialEq)]
pub struct Bond {
    data: InvestmentData,
    maturity: Option<SimpleDate>,
    interest_rate: Option<Promised<f64>>,
}

#[pymethods]
impl Bond {
    #[new]
    #[pyo3(signature = (
        *, company, company_match, fund, market_value, currency,
        nominal_quantity = None, perc_net_assets = None, acquisition_cost = None,
        acquisition_currency = None, maturity = None, interest_rate = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        company: String,
        company_match: String,
        fund: &Bound<'_, PyAny>,
        market_value: &Bound<'_, PyAny>,
        currency: &Bound<'_, PyAny>,
        nominal_quantity: Option<&Bound<'_, PyAny>>,
        perc_net_assets: Option<&Bound<'_, PyAny>>,
        acquisition_cost: Option<&Bound<'_, PyAny>>,
        acquisition_currency: Option<&Bound<'_, PyAny>>,
        maturity: Option<&Bound<'_, PyAny>>,
        interest_rate: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let maturity = maturity
            .filter(|v| !v.is_none())
            .map(|v| v.extract::<SimpleDate>())
            .transpose()?;
        let interest_rate = extract_opt_promised_f64(interest_rate, FloatConstraint::UnitIntervalHalfOpen)?;
        Ok(Self {
            data: InvestmentData::new(
                company, company_match, fund, nominal_quantity, market_value, currency,
                perc_net_assets, acquisition_cost, acquisition_currency,
            )?,
            maturity,
            interest_rate,
        })
    }

    #[getter]
    fn company(&self) -> &str {
        &self.data.company
    }

    #[getter]
    fn company_match(&self) -> &str {
        &self.data.company_match
    }

    #[getter]
    fn fund(&self) -> Promised<String> {
        self.data.fund.clone()
    }

    #[getter]
    fn nominal_quantity(&self) -> Option<f64> {
        self.data.nominal_quantity
    }

    #[getter]
    fn market_value(&self) -> Promised<f64> {
        self.data.market_value.clone()
    }

    /// Pydantic's default `BaseModel` fields are mutable (`PromisableDict.model_config` sets
    /// `validate_assignment=True`, revalidating on every assignment) — found necessary via the
    /// full formats suite: `mediolanum_es24_b.py` does `blk.market_value = blk.market_value *
    /// 1000` on a freshly deserialized `Bond`/`Equity`. Only `market_value` has a setter for
    /// now — it's the only field any real format code was found mutating after construction
    /// (checked via grep across both this repo and `analysis_finance_reports_formats`); the rest
    /// stay read-only until a concrete need for them surfaces the same way.
    #[setter]
    fn set_market_value(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        self.data.market_value = promisable::extract_promised_f64(value, FloatConstraint::Positive)?;
        Ok(())
    }

    #[getter]
    fn currency(&self) -> Promised<Currency> {
        self.data.currency.clone()
    }

    #[getter]
    fn perc_net_assets(&self) -> Option<Promised<f64>> {
        self.data.perc_net_assets.clone()
    }

    #[getter]
    fn acquisition_cost(&self) -> Option<Promised<f64>> {
        self.data.acquisition_cost.clone()
    }

    #[getter]
    fn acquisition_currency(&self) -> Option<Promised<Currency>> {
        self.data.acquisition_currency.clone()
    }

    #[getter]
    fn maturity<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        match self.maturity {
            None => Ok(py.None().into_bound(py)),
            Some(d) => Ok(d.into_pyobject(py)?.into_any()),
        }
    }

    #[getter]
    fn interest_rate(&self) -> Option<Promised<f64>> {
        self.interest_rate.clone()
    }

    #[classattr]
    #[allow(clippy::type_complexity)]
    fn __rust_model_fields__() -> (
        &'static str, &'static str, &'static str, &'static str, &'static str,
        &'static str, &'static str, &'static str, &'static str, &'static str, &'static str,
    ) {
        (
            "company", "company_match", "fund", "nominal_quantity", "market_value",
            "currency", "perc_net_assets", "acquisition_cost", "acquisition_currency",
            "maturity", "interest_rate",
        )
    }

    #[pyo3(signature = (*, mode = "python", by_alias = false))]
    fn model_dump<'py>(&self, py: Python<'py>, mode: &str, by_alias: bool) -> PyResult<Bound<'py, PyDict>> {
        if mode != "json" || !by_alias {
            return Err(PyValueError::new_err(
                "only model_dump(mode=\"json\", by_alias=True) is supported by this Rust port",
            ));
        }
        let dict = PyDict::new(py);
        self.data.model_dump_into(py, &dict)?;
        // No `serialization_alias` on either field in the Python original — dumped under the
        // raw field name, unlike every `Investment` field.
        match self.maturity {
            None => dict.set_item("maturity", py.None())?,
            Some(d) => dict.set_item("maturity", format!("{:04}-{:02}-{:02}", d.year, d.month, d.day))?,
        }
        match &self.interest_rate {
            None => dict.set_item("interest_rate", py.None())?,
            Some(Promised::Resolved(v)) => dict.set_item("interest_rate", v)?,
            Some(Promised::Pending(_)) => return Err(unresolved_promise_error("interest_rate")),
        }
        Ok(dict)
    }

    fn fulfill_promises(
        &mut self,
        py: Python<'_>,
        mapping: &Bound<'_, PyDict>,
    ) -> PyResult<Option<Vec<Py<Self>>>> {
        let expansions = promisable::fulfill_promises(self, py, mapping)?;
        match expansions {
            None => Ok(None),
            Some(clones) => clones.into_iter().map(|f| Py::new(py, f)).collect::<PyResult<Vec<_>>>().map(Some),
        }
    }

    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        let dict = PyDict::new(py);
        self.data.model_dump_into(py, &dict)?;
        match self.maturity {
            None => dict.set_item("maturity", py.None())?,
            Some(d) => dict.set_item("maturity", format!("{:04}-{:02}-{:02}", d.year, d.month, d.day))?,
        }
        match &self.interest_rate {
            None => dict.set_item("interest_rate", py.None())?,
            Some(Promised::Resolved(v)) => dict.set_item("interest_rate", v)?,
            Some(Promised::Pending(_)) => return Err(unresolved_promise_error("interest_rate")),
        }
        promisable::hash_via_model_dump_items(py, &dict)
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other.extract::<PyRef<'_, Self>>().map(|o| *self == *o).unwrap_or(false)
    }

    fn __repr__(&self) -> String {
        format!("Bond(company={:?}, market_value={:?})", self.data.company, self.data.market_value)
    }
}

impl crate::core::promisable::PromisableFields for Bond {
    fn pending_promises(&self) -> Vec<(&'static str, Promise)> {
        let mut out = self.data.pending_promises();
        if let Some(Promised::Pending(p)) = &self.interest_rate {
            out.push(("interest_rate", p.clone()));
        }
        out
    }

    fn resolve_field(&mut self, py: Python<'_>, field: &'static str, value: Py<PyAny>) -> PyResult<()> {
        if self.data.resolve_field(py, field, value.clone_ref(py))? {
            return Ok(());
        }
        match field {
            "interest_rate" => {
                self.interest_rate =
                    Some(Promised::Resolved(promisable::extract_f64(value.bind(py), FloatConstraint::UnitIntervalHalfOpen)?));
                Ok(())
            }
            _ => unreachable!("Bond has no promisable field {field:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn py_f64(py: Python<'_>, v: f64) -> Bound<'_, PyAny> {
        v.into_pyobject(py).unwrap().into_any()
    }

    fn make_equity(py: Python<'_>, company: &str, market_value: f64) -> Equity {
        let fund = "F".into_pyobject(py).unwrap().into_any();
        let mv = py_f64(py, market_value);
        let currency = "EUR".into_pyobject(py).unwrap().into_any();
        Equity::new(
            company.into(), "match".into(), &fund, &mv, &currency, None, None, None, None,
        )
        .unwrap()
    }

    #[test]
    fn constructs_and_reads_fields() {
        Python::attach(|py| {
            let e = make_equity(py, "ACME", 100.0);
            assert_eq!(e.company(), "ACME");
            assert_eq!(e.data.market_value, Promised::Resolved(100.0));
            assert_eq!(e.data.currency, Promised::Resolved(Currency::EUR));
        });
    }

    #[test]
    fn rejects_non_positive_market_value() {
        Python::attach(|py| {
            let fund = "F".into_pyobject(py).unwrap().into_any();
            let mv = py_f64(py, 0.0);
            let currency = "EUR".into_pyobject(py).unwrap().into_any();
            let result = Equity::new("A".into(), "m".into(), &fund, &mv, &currency, None, None, None, None);
            assert!(result.is_err());
        });
    }

    #[test]
    fn market_value_setter_revalidates_like_assignment_would() {
        Python::attach(|py| {
            let mut e = make_equity(py, "ACME", 100.0);
            let new_value = py_f64(py, 300.0);
            e.set_market_value(&new_value).unwrap();
            assert_eq!(e.data.market_value, Promised::Resolved(300.0));

            let negative = py_f64(py, -1.0);
            assert!(e.set_market_value(&negative).is_err());
        });
    }

    #[test]
    fn model_dump_includes_none_optionals_and_excludes_fund() {
        Python::attach(|py| {
            let e = make_equity(py, "ACME", 100.0);
            let dumped = e.model_dump(py, "json", true).unwrap();
            assert_eq!(dumped.get_item("Investee").unwrap().unwrap().extract::<String>().unwrap(), "ACME");
            assert!(dumped.get_item("Nominal/Quantity").unwrap().unwrap().is_none());
            assert!(dumped.get_item("fund").unwrap().is_none());
            assert_eq!(dumped.len(), 8);
        });
    }

    #[test]
    fn equity_and_bond_with_same_fields_are_not_equal_types() {
        Python::attach(|py| {
            let e = make_equity(py, "ACME", 100.0);
            let fund = "F".into_pyobject(py).unwrap().into_any();
            let mv = py_f64(py, 100.0);
            let currency = "EUR".into_pyobject(py).unwrap().into_any();
            let b = Bond::new(
                "ACME".into(), "match".into(), &fund, &mv, &currency,
                None, None, None, None, None, None,
            )
            .unwrap();
            let bound_b = Py::new(py, b).unwrap();
            assert!(!e.__eq__(bound_b.bind(py)));
        });
    }

    #[test]
    fn bond_dump_uses_raw_field_names_for_maturity_and_interest_rate() {
        Python::attach(|py| {
            let fund = "F".into_pyobject(py).unwrap().into_any();
            let mv = py_f64(py, 100.0);
            let currency = "EUR".into_pyobject(py).unwrap().into_any();
            let b = Bond::new(
                "ACME".into(), "match".into(), &fund, &mv, &currency,
                None, None, None, None, None, None,
            )
            .unwrap();
            let dumped = b.model_dump(py, "json", true).unwrap();
            assert!(dumped.get_item("maturity").unwrap().unwrap().is_none());
            assert!(dumped.get_item("interest_rate").unwrap().unwrap().is_none());
            assert_eq!(dumped.len(), 10);
        });
    }

    #[test]
    fn fulfill_promises_resolves_market_value_and_currency() {
        Python::attach(|py| {
            let fund = "F".into_pyobject(py).unwrap().into_any();
            let mv = Promise::from_parts("mv", false, false).into_pyobject(py).unwrap().into_any();
            let currency = Promise::from_parts("cur", false, false).into_pyobject(py).unwrap().into_any();
            let mut e = Equity::new(
                "ACME".into(), "match".into(), &fund, &mv, &currency, None, None, None, None,
            )
            .unwrap();
            let mapping = PyDict::new(py);
            mapping.set_item("mv", 200.0).unwrap();
            mapping.set_item("cur", "USD").unwrap();
            let result = e.fulfill_promises(py, &mapping).unwrap();
            assert!(result.is_none());
            assert_eq!(e.data.market_value, Promised::Resolved(200.0));
            assert_eq!(e.data.currency, Promised::Resolved(Currency::USD));
        });
    }

    #[test]
    fn hash_matches_python_frozenset_semantics_order_independence() {
        Python::attach(|py| {
            let a = make_equity(py, "ACME", 100.0);
            let b = make_equity(py, "ACME", 100.0);
            assert_eq!(a.__hash__(py).unwrap(), b.__hash__(py).unwrap());
        });
    }
}
