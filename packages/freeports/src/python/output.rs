//! The shims of the output entities: what a deserialize pipe produces.
//!
//! The Python contract is the one formats repositories already use — class names, constructor
//! keywords, getters — so that updating a repository stays limited to its imports.
//!
//! # Three names are tuples of types, not base classes
//!
//! `Investment`, `AssetsManager` and `FundChangeName` used to be common base classes; the concrete
//! entities are now independent and share no ancestor. They stay exposed as **tuples of types**,
//! which is exactly what is needed: every real use in a formats repository is an `isinstance` test,
//! and `isinstance` accepts a tuple.
//!
//! # Equality and hashing go through serde
//!
//! The native entities derive equality but not all of them hashing, floating-point fields being
//! what they are. Rather than adding derives to existing code, the shims compare and hash the
//! **serde form** of the value: deterministic, defined for all of them, and consistent between
//! equality and hashing by construction.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pyo3::prelude::*;
use pyo3::types::{PySet, PyTuple};
use serde::Serialize;

use crate::commons::date::Date;
use crate::core::classes::BlockValue;
use crate::core::promisable::Promised;
use crate::output::classes::assets_manager::{InvestmentsManager, ManagementCompany};
use crate::output::classes::fund::Fund;
use crate::output::classes::fund_assets::FundAssets;
use crate::output::classes::fund_change_name::{FundMerge, FundRename};
use crate::output::classes::fund_esg_indicator::FundEsgIndicator;
use crate::output::classes::fund_sfdr_classification::FundSfdrClassification;
use crate::output::classes::investment::{Bond, Equity, InvestmentFields};

use super::convert::block_value_from_py;
use super::core::PyPromise;
use crate::core::tracing_setup::log_error;

/// An entity construction error as a Python `ValueError`.
fn value_error<E: std::fmt::Display>(error: E) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(error.to_string())
}

/// The serde form of a value, used by the shims' equality, hashing and representation.
fn canonical<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|e| format!("<unserializable: {e}>"))
}

/// A hash of the serde form — consistent with equality, which compares the same string.
fn canonical_hash<T: Serialize>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    canonical(value).hash(&mut hasher);
    hasher.finish()
}

/// Emits the **whole** methods block of an entity shim: the three common special methods, all over
/// the serde form, plus the type's own methods.
///
/// It is generated all at once rather than composed from several blocks for two reasons: PyO3
/// accepts only one block per class without an extra feature, and it does not allow macro
/// invocations *inside* such a block, seeing them as unrecognised items. A macro producing the
/// whole block has neither problem.
///
/// One arm adds the nine getters an equity and a bond share, reading them from the same nested
/// data.
macro_rules! entity_pymethods {
    ($shim:ident, $py_name:literal, investment, { $($rest:tt)* }) => {
        entity_pymethods!($shim, $py_name, {
            #[getter]
            fn company(&self) -> &str {
                &self.0.data.company
            }

            #[getter]
            fn company_match(&self) -> &str {
                &self.0.data.company_match
            }

            #[getter]
            fn fund<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
                promised_to_py(py, &self.0.data.fund, |v| Ok(v.into_pyobject(py)?.into_any()))
            }

            #[getter]
            fn nominal_quantity(&self) -> Option<f64> {
                self.0.data.nominal_quantity.map(|v| v.into_inner())
            }

            #[getter]
            fn market_value<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
                promised_to_py(py, &self.0.data.market_value, |v| {
                    Ok(v.into_inner().into_pyobject(py)?.into_any())
                })
            }

            /// The only writable field of an investment; see the shim macro for the use case that
            /// requires it. Rewriting the market value **resolves** the promise if there was one:
            /// assigning a number means that number is the final value, not a deferral.
            #[setter]
            fn set_market_value(&mut self, value: f64) {
                self.0.data.market_value =
                    crate::core::promisable::Promised::Resolved(ordered_float::OrderedFloat(value));
            }

            #[getter]
            fn currency<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
                promised_to_py(py, &self.0.data.currency, |v| {
                    Ok(Bound::new(py, super::consts::PyCurrency::from(*v))?.into_any())
                })
            }

            #[getter]
            fn perc_net_assets<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
                self.0
                    .data
                    .perc_net_assets
                    .as_ref()
                    .map(|p| promised_to_py(py, p, |v| Ok(v.into_inner().into_pyobject(py)?.into_any())))
                    .transpose()
            }

            #[getter]
            fn acquisition_cost<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
                self.0
                    .data
                    .acquisition_cost
                    .as_ref()
                    .map(|p| promised_to_py(py, p, |v| Ok(v.into_inner().into_pyobject(py)?.into_any())))
                    .transpose()
            }

            #[getter]
            fn acquisition_currency<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
                self.0
                    .data
                    .acquisition_currency
                    .as_ref()
                    .map(|p| {
                        promised_to_py(py, p, |v| {
                            Ok(Bound::new(py, super::consts::PyCurrency::from(*v))?.into_any())
                        })
                    })
                    .transpose()
            }

            $($rest)*
        });
    };
    ($shim:ident, $py_name:literal, { $($rest:tt)* }) => {
        #[pymethods]
        impl $shim {
            fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
                match other.extract::<PyRef<'_, $shim>>() {
                    Ok(other) => canonical(&self.0) == canonical(&other.0),
                    Err(_) => false,
                }
            }

            fn __hash__(&self) -> u64 {
                canonical_hash(&self.0)
            }

            fn __repr__(&self) -> String {
                format!("{}({})", $py_name, canonical(&self.0))
            }

            /// The entity's field names, sorted.
            ///
            /// They exist for the development tooling's fixture serialization, which must enumerate
            /// an entity's fields without knowing its type. They are derived from the serde form —
            /// the same one equality, hashing and representation already depend on — so they
            /// coincide by construction with the keys the constructor accepts, with no hand-written
            /// list that could drift.
            fn __serialize_fields__(&self) -> PyResult<Vec<String>> {
                match serde_json::from_str::<serde_json::Value>(&canonical(&self.0)) {
                    Ok(serde_json::Value::Object(map)) => Ok(map.keys().cloned().collect()),
                    _ => Err(pyo3::exceptions::PyRuntimeError::new_err(concat!(
                        $py_name,
                        " does not serialize to a JSON object"
                    ))),
                }
            }

            $($rest)*
        }
    };
}

/// Declares an entity's Python newtype and its internal accessors.
///
/// One arm makes the class mutable. It is needed by the two investment entities and by no others:
/// an author module corrects the market value *after* building the investment, and there is no way
/// to express that on an immutable class. The other eight stay immutable, which is the right shape
/// for an already-validated result.
macro_rules! entity_shim {
    ($shim:ident, $native:ty, $py_name:literal) => {
        #[doc = concat!("Shim Python di [`", stringify!($native), "`].")]
        #[pyclass(name = $py_name, module = "freeports.output", frozen)]
        #[derive(Debug, Clone)]
        pub struct $shim($native);

        entity_shim!(@common $shim, $native);
    };

    (mutable $shim:ident, $native:ty, $py_name:literal) => {
        #[doc = concat!("Shim Python di [`", stringify!($native), "`].")]
        #[pyclass(name = $py_name, module = "freeports.output")]
        #[derive(Debug, Clone)]
        pub struct $shim($native);

        entity_shim!(@common $shim, $native);
    };

    (@common $shim:ident, $native:ty) => {

        impl From<$native> for $shim {
            fn from(value: $native) -> Self {
                $shim(value)
            }
        }

        impl $shim {
            pub fn inner(&self) -> &$native {
                &self.0
            }
        }
    };
}

entity_shim!(PyFund, Fund, "Fund");
entity_shim!(mutable PyEquity, Equity, "Equity");
entity_shim!(mutable PyBond, Bond, "Bond");
entity_shim!(PyFundAssets, FundAssets, "FundAssets");
entity_shim!(PyFundRename, FundRename, "FundRename");
entity_shim!(PyFundMerge, FundMerge, "FundMerge");
entity_shim!(PyFundEsgIndicator, FundEsgIndicator, "FundEsgIndicator");
entity_shim!(PyFundSfdrClassification, FundSfdrClassification, "FundSfdrClassification");
entity_shim!(PyManagementCompany, ManagementCompany, "ManagementCompany");
entity_shim!(PyInvestmentsManager, InvestmentsManager, "InvestmentsManager");

/// A field that may be already resolved or still promised, as a Python object: the value itself, or
/// the promise shim.
fn promised_to_py<'py, T, F>(
    py: Python<'py>,
    promised: &Promised<T>,
    resolved: F,
) -> PyResult<Bound<'py, PyAny>>
where
    F: FnOnce(&T) -> PyResult<Bound<'py, PyAny>>,
{
    match promised {
        Promised::Resolved(value) => resolved(value),
        Promised::Pending(promise) => {
            Ok(Bound::new(py, PyPromise::from(promise.clone()))?.into_any())
        }
    }
}

entity_pymethods!(PyFund, "Fund", {
        /// The name accepts a string or a promise: a fund's name is often discovered on a page
        /// other than the one citing it.
        #[new]
        #[pyo3(signature = (name))]
        fn new(name: &Bound<'_, PyAny>) -> PyResult<PyFund> {
            Ok(PyFund(Fund::from_value(&block_value_from_py(name)?).map_err(|e| {
                tracing::error!(error = log_error(&e), "Fund construction failed: {e}");
                value_error(e)
            })?))
        }

        /// The upper-cased name — or the promise, if still pending.
        #[getter]
        fn name<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
            match (self.0.name(), self.0.pending_name()) {
                (Some(name), _) => Ok(name.into_pyobject(py)?.into_any()),
                (None, Some(promise)) => {
                    Ok(Bound::new(py, PyPromise::from(promise.clone()))?.into_any())
                }
                (None, None) => Ok(py.None().into_bound(py)),
            }
        }
});

/// The fields an equity and a bond share, read from keyword arguments under their public names.
#[allow(clippy::too_many_arguments)]
fn investment_fields(
    company: String,
    company_match: String,
    fund: &Bound<'_, PyAny>,
    market_value: &Bound<'_, PyAny>,
    currency: &Bound<'_, PyAny>,
    nominal_quantity: Option<f64>,
    perc_net_assets: Option<&Bound<'_, PyAny>>,
    acquisition_cost: Option<&Bound<'_, PyAny>>,
    acquisition_currency: Option<&Bound<'_, PyAny>>,
) -> PyResult<InvestmentFields> {
    let optional = |value: Option<&Bound<'_, PyAny>>| -> PyResult<Option<BlockValue>> {
        value.filter(|v| !v.is_none()).map(block_value_from_py).transpose()
    };
    let mut fields = InvestmentFields::new(
        company,
        company_match,
        block_value_from_py(fund)?,
        block_value_from_py(market_value)?,
        block_value_from_py(currency)?,
    );
    fields.nominal_quantity = nominal_quantity;
    fields.perc_net_assets = optional(perc_net_assets)?;
    fields.acquisition_cost = optional(acquisition_cost)?;
    fields.acquisition_currency = optional(acquisition_currency)?;
    Ok(fields)
}

// `Equity` e `Bond` condividono i nove getter di `InvestmentData`: li porta l'arm `investment`
// di `entity_pymethods!`.
entity_pymethods!(PyEquity, "Equity", investment, {
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
            nominal_quantity: Option<f64>,
            perc_net_assets: Option<&Bound<'_, PyAny>>,
            acquisition_cost: Option<&Bound<'_, PyAny>>,
            acquisition_currency: Option<&Bound<'_, PyAny>>,
        ) -> PyResult<PyEquity> {
            let fields = investment_fields(
                company,
                company_match,
                fund,
                market_value,
                currency,
                nominal_quantity,
                perc_net_assets,
                acquisition_cost,
                acquisition_currency,
            )?;
            Ok(PyEquity(Equity::build(fields).map_err(|e| {
                tracing::error!(error = log_error(&e), "Equity construction failed: {e}");
                value_error(e)
            })?))
        }
});

entity_pymethods!(PyBond, "Bond", investment, {
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
            nominal_quantity: Option<f64>,
            perc_net_assets: Option<&Bound<'_, PyAny>>,
            acquisition_cost: Option<&Bound<'_, PyAny>>,
            acquisition_currency: Option<&Bound<'_, PyAny>>,
            maturity: Option<&Bound<'_, PyAny>>,
            interest_rate: Option<f64>,
        ) -> PyResult<PyBond> {
            let fields = investment_fields(
                company,
                company_match,
                fund,
                market_value,
                currency,
                nominal_quantity,
                perc_net_assets,
                acquisition_cost,
                acquisition_currency,
            )?;
            let maturity = date_argument("maturity", maturity)?;
            Ok(PyBond(Bond::build(fields, maturity, interest_rate).map_err(|e| {
                tracing::error!(error = log_error(&e), "Bond construction failed: {e}");
                value_error(e)
            })?))
        }

        #[getter]
        fn maturity<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
            self.0
                .maturity
                .as_ref()
                .map(|d| {
                    py.import("datetime")?
                        .getattr("date")?
                        .call_method1("fromisoformat", (d.to_string(),))
                })
                .transpose()
        }

        #[getter]
        fn interest_rate(&self) -> Option<f64> {
            self.0.interest_rate.map(|v| v.into_inner())
        }
});

/// A non-promisable date passed as an argument.
///
/// Different from a promisable field: a bond's maturity is a plain optional date, so a promise here
/// is a usage error rather than an acceptable value.
fn date_argument(field: &str, value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Date>> {
    let Some(value) = value.filter(|v| !v.is_none()) else { return Ok(None) };
    match block_value_from_py(value)? {
        BlockValue::Date(date) => Ok(Some(date)),
        BlockValue::Str(raw) => raw.parse::<Date>().map(Some).map_err(|e| {
            tracing::error!(error = log_error(&e), field, raw = %raw, "cannot parse as a date: {e}");
            value_error(e)
        }),
        other => {
            let kind = other.kind();
            tracing::error!(field, kind, "expected a date, found a different kind of value");
            Err(pyo3::exceptions::PyTypeError::new_err(format!("'{field}' must be a date, found a {kind}")))
        }
    }
}

entity_pymethods!(PyFundAssets, "FundAssets", {
        #[new]
        #[pyo3(signature = (*, fund, tot_assets, liabilities, net_assets, currency, date = None))]
        fn new(
            fund: String,
            tot_assets: f64,
            liabilities: f64,
            net_assets: f64,
            currency: &Bound<'_, PyAny>,
            date: Option<&Bound<'_, PyAny>>,
        ) -> PyResult<PyFundAssets> {
            let currency = block_value_from_py(currency)?;
            let date = date.filter(|v| !v.is_none()).map(block_value_from_py).transpose()?;
            Ok(PyFundAssets(
                FundAssets::build(fund, tot_assets, liabilities, net_assets, &currency, date.as_ref()).map_err(
                    |e| {
                        tracing::error!(error = log_error(&e), "FundAssets construction failed: {e}");
                        value_error(e)
                    },
                )?,
            ))
        }

        #[getter]
        fn fund(&self) -> &str {
            &self.0.fund
        }

        #[getter]
        fn tot_assets(&self) -> f64 {
            self.0.tot_assets.into_inner()
        }

        #[getter]
        fn liabilities(&self) -> f64 {
            self.0.liabilities.into_inner()
        }

        #[getter]
        fn net_assets(&self) -> f64 {
            self.0.net_assets.into_inner()
        }

        #[getter]
        fn currency<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
            promised_to_py(py, &self.0.currency, |v| {
                Ok(Bound::new(py, super::consts::PyCurrency::from(*v))?.into_any())
            })
        }

        #[getter]
        fn date<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
            self.0
                .date
                .as_ref()
                .map(|p| {
                    promised_to_py(py, p, |d| {
                        py.import("datetime")?
                            .getattr("date")?
                            .call_method1("fromisoformat", (d.to_string(),))
                    })
                })
                .transpose()
        }
});

/// The two change-of-name entities differ only in type: same three fields, same construction.
macro_rules! change_name_shim {
    ($shim:ident, $native:ty, $py_name:literal) => {
        entity_pymethods!($shim, $py_name, {
            #[new]
            #[pyo3(signature = (*, old_name, current_name, date))]
            fn new(
                old_name: String,
                current_name: String,
                date: &Bound<'_, PyAny>,
            ) -> PyResult<$shim> {
                let date = block_value_from_py(date)?;
                Ok($shim(<$native>::build(old_name, current_name, &date).map_err(|e| {
                    tracing::error!(error = log_error(&e), entity = $py_name, "construction failed: {e}");
                    value_error(e)
                })?))
            }

            #[getter]
            fn old_name(&self) -> &str {
                &self.0.data.old_name
            }

            #[getter]
            fn current_name(&self) -> &str {
                &self.0.data.current_name
            }

            #[getter]
            fn date<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
                promised_to_py(py, &self.0.data.date, |d| {
                    py.import("datetime")?
                        .getattr("date")?
                        .call_method1("fromisoformat", (d.to_string(),))
                })
            }
        });
    };
}

change_name_shim!(PyFundRename, FundRename, "FundRename");
change_name_shim!(PyFundMerge, FundMerge, "FundMerge");

entity_pymethods!(PyFundEsgIndicator, "FundEsgIndicator", {
        #[new]
        #[pyo3(signature = (*, fund, name, value))]
        fn new(fund: &Bound<'_, PyAny>, name: String, value: String) -> PyResult<PyFundEsgIndicator> {
            let fund = block_value_from_py(fund)?;
            Ok(PyFundEsgIndicator(FundEsgIndicator::build(&fund, name, value).map_err(|e| {
                tracing::error!(error = log_error(&e), "FundEsgIndicator construction failed: {e}");
                value_error(e)
            })?))
        }

        #[getter]
        fn fund<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
            promised_to_py(py, &self.0.fund, |v| Ok(v.into_pyobject(py)?.into_any()))
        }

        #[getter]
        fn name(&self) -> &str {
            &self.0.name
        }

        #[getter]
        fn value(&self) -> &str {
            &self.0.value
        }
});

entity_pymethods!(PyFundSfdrClassification, "FundSfdrClassification", {
        #[new]
        #[pyo3(signature = (*, fund, article))]
        fn new(fund: String, article: &Bound<'_, PyAny>) -> PyResult<PyFundSfdrClassification> {
            let article = block_value_from_py(article)?;
            Ok(PyFundSfdrClassification(
                FundSfdrClassification::build(fund, &article).map_err(|e| {
                    tracing::error!(error = log_error(&e), "FundSfdrClassification construction failed: {e}");
                    value_error(e)
                })?,
            ))
        }

        #[getter]
        fn fund(&self) -> &str {
            &self.0.fund
        }

        #[getter]
        fn article<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
            promised_to_py(py, &self.0.article, |v| {
                Ok(Bound::new(py, super::consts::PySfdrArticle::from(*v))?.into_any())
            })
        }
});

/// The two asset managers: same two fields, same construction.
///
/// The managed funds accept any iterable, not only a set: an author's deserializer is not obliged
/// to build one.
macro_rules! assets_manager_shim {
    ($shim:ident, $native:ty, $py_name:literal) => {
        entity_pymethods!($shim, $py_name, {
            #[new]
            #[pyo3(signature = (*, name, managed_funds))]
            fn new(name: String, managed_funds: &Bound<'_, PyAny>) -> PyResult<$shim> {
                let managed_funds = BlockValue::Set(
                    managed_funds
                        .try_iter()?
                        .map(|item| Ok(BlockValue::Str(item?.extract::<String>()?)))
                        .collect::<PyResult<_>>()?,
                );
                let name = BlockValue::Str(name);
                Ok($shim(<$native>::build(&name, &managed_funds).map_err(|e| {
                    tracing::error!(error = log_error(&e), entity = $py_name, "construction failed: {e}");
                    value_error(e)
                })?))
            }

            #[getter]
            fn name(&self) -> &str {
                &self.0.data.name
            }

            #[getter]
            fn managed_funds<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PySet>> {
                PySet::new(py, &self.0.data.managed_funds)
            }
        });
    };
}

assets_manager_shim!(PyManagementCompany, ManagementCompany, "ManagementCompany");
assets_manager_shim!(PyInvestmentsManager, InvestmentsManager, "InvestmentsManager");

/// Attaches the three tuple aliases to the module.
///
/// They are not classes: they are tuples of types, constructible only once the types exist, that
/// is, with the module already built. See the module documentation for why they are not base
/// classes.
pub fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = module.py();
    for (alias, members) in [
        ("Investment", ["Equity", "Bond"].as_slice()),
        ("AssetsManager", ["ManagementCompany", "InvestmentsManager"].as_slice()),
        ("FundChangeName", ["FundRename", "FundMerge"].as_slice()),
    ] {
        let types = members.iter().map(|name| module.getattr(*name)).collect::<PyResult<Vec<_>>>()?;
        module.setattr(alias, PyTuple::new(py, types)?)?;
    }
    Ok(())
}
