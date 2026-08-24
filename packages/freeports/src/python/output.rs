//! Shim di `output::classes`: le entità che un pipe `deserialize` produce.
//!
//! Il contratto Python è quello che il repo formati usa già oggi — nomi di classe, kwargs del
//! costruttore, getter — verificato riga per riga contro i `#[pyclass]` che il vecchio
//! `freeports_core` esponeva, così che l'aggiornamento del repo formati resti limitato agli
//! import (`M10-implementation-plan.md`, vincolo 3).
//!
//! # `Investment`, `AssetsManager`, `FundChangeName` sono tuple, non classi base
//!
//! Nel riferimento erano le basi Pydantic comuni delle classi concrete; nel porting Rust le
//! classi concrete sono indipendenti e non hanno un antenato comune. Restano esposti come
//! **tuple di tipi**, che è esattamente ciò che serve: ogni uso reale nel repo formati è un
//! `isinstance(x, Investment)`, e `isinstance` accetta una tupla. Non è una nuova astrazione, è
//! un rimpiazzo alla lettera — la stessa scelta che il vecchio `classes_schema.py` aveva già
//! fatto.
//!
//! # Uguaglianza e hash passano da serde
//!
//! Le entità native derivano `PartialEq`/`Eq` ma non tutte `Hash` (i campi `f64` viaggiano in
//! `OrderedFloat` dentro `Option`, e non tutte le struct l'hanno derivato). Invece di aggiungere
//! derive al codice esistente, gli shim confrontano e hashano la **forma serde** del valore: è
//! deterministica, definita per tutti, e coerente fra `__eq__` e `__hash__`.

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

/// Un errore di costruzione di un'entità come `ValueError` Python.
fn value_error<E: std::fmt::Display>(error: E) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(error.to_string())
}

/// La forma serde di un valore, usata da `__eq__`/`__hash__`/`__repr__` degli shim.
fn canonical<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|e| format!("<unserializable: {e}>"))
}

/// Hash della forma serde — coerente con l'uguaglianza, che confronta la stessa stringa.
fn canonical_hash<T: Serialize>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    canonical(value).hash(&mut hasher);
    hasher.finish()
}

/// Genera le quattro `dunder` comuni a ogni shim di entità: uguaglianza, hash e repr, tutte
/// basate sulla forma serde del valore nativo avvolto.
/// Emette l'**intero** blocco `#[pymethods]` di uno shim di entità: le tre dunder comuni
/// (uguaglianza, hash, repr, tutte sulla forma serde) più i metodi specifici del tipo.
///
/// Il blocco è generato tutto insieme, e non composto da più `#[pymethods]`, per due ragioni:
/// PyO3 accetta un solo blocco per classe senza la feature `multiple-pymethods` (che
/// aggiungerebbe una dipendenza per una comodità di scrittura), e non ammette invocazioni di
/// macro *dentro* un blocco `#[pymethods]` — le vede come item non riconosciuti. Un macro che
/// produce il blocco intero non ha nessuno dei due problemi: `macro_rules!` espande prima, e
/// l'attributo vede solo item veri.
///
/// L'arm `investment` aggiunge i nove getter che `Equity` e `Bond` condividono, leggendoli dalla
/// stessa `InvestmentData` annidata.
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

            /// L'unico campo scrivibile di un investimento: vedi il doc-comment di
            /// `entity_shim!` per il caso d'uso che lo richiede. Riscrivere il valore di mercato
            /// **risolve** la promessa se ce n'era una — assegnare un numero significa che quel
            /// numero è il valore definitivo, non un rinvio.
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

            /// I nomi dei campi dell'entità, ordinati.
            ///
            /// Esistono per la serializzazione delle fixture di `freeports-dev`, che deve poter
            /// enumerare i campi di un'entità senza conoscerne il tipo (nel riferimento lo faceva
            /// con `BaseModel.model_fields` di Pydantic, che qui non c'è). Sono ricavati dalla
            /// forma serde — la stessa da cui dipendono già uguaglianza, hash e repr — quindi
            /// coincidono per costruzione con le chiavi che il costruttore accetta, senza un
            /// elenco scritto a mano che possa divergere.
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

/// Dichiara il newtype `#[pyclass]` di un'entità e i suoi accessori interni.
///
/// L'arm `mutable` toglie `frozen` alla classe. Serve a `Equity` e `Bond` e a nessun'altra: un
/// modulo d'autore corregge il valore di mercato *dopo* aver costruito l'investimento
/// (`mediolanum_es24_b.py`: `blk = std(txt_blk); if blk is not None: blk.market_value =
/// blk.market_value * 1000`), e non c'è modo di esprimerlo su una classe immutabile. Le altre
/// otto entità restano `frozen`, che è la forma giusta per un risultato già validato.
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

/// Un campo che può essere già risolto o ancora promesso, come oggetto Python: il valore stesso,
/// oppure lo shim `Promise`. È la forma che il riferimento esponeva e che il codice d'autore
/// legge (`isinstance(x.fund, Promise)` non compare da nessuna parte: si legge e basta).
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
        /// `name` accetta una stringa o una `Promise`: il nome di un fondo è spesso scoperto in una
        /// pagina diversa da quella che lo cita.
        #[new]
        #[pyo3(signature = (name))]
        fn new(name: &Bound<'_, PyAny>) -> PyResult<PyFund> {
            Ok(PyFund(Fund::from_value(&block_value_from_py(name)?).map_err(value_error)?))
        }

        /// Il nome maiuscolizzato, come nel riferimento — oppure la promessa, se ancora pendente.
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

/// I campi comuni di `Equity` e `Bond`, letti dai kwargs con gli stessi nomi del riferimento.
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
            Ok(PyEquity(Equity::build(fields).map_err(value_error)?))
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
            Ok(PyBond(Bond::build(fields, maturity, interest_rate).map_err(value_error)?))
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

/// Una data non promettibile passata come argomento: `datetime.date` o `None`.
///
/// Diverso da un campo `Promised<Date>`: `Bond::maturity` è un `Option<Date>` secco, quindi una
/// `Promise` qui è un errore d'uso, non un valore accettabile.
fn date_argument(field: &str, value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Date>> {
    let Some(value) = value.filter(|v| !v.is_none()) else { return Ok(None) };
    match block_value_from_py(value)? {
        BlockValue::Date(date) => Ok(Some(date)),
        BlockValue::Str(raw) => raw.parse::<Date>().map(Some).map_err(value_error),
        other => Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "'{field}' must be a date, found a {}",
            other.kind()
        ))),
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
                FundAssets::build(fund, tot_assets, liabilities, net_assets, &currency, date.as_ref())
                    .map_err(value_error)?,
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

/// `FundRename` e `FundMerge` differiscono solo per il tipo: stessi tre campi, stessa
/// costruzione. Il macro evita di scrivere due volte lo stesso blocco.
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
                Ok($shim(<$native>::build(old_name, current_name, &date).map_err(value_error)?))
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
            Ok(PyFundEsgIndicator(FundEsgIndicator::build(&fund, name, value).map_err(value_error)?))
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
                FundSfdrClassification::build(fund, &article).map_err(value_error)?,
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

/// I due gestori patrimoniali: stessi due campi, stessa costruzione.
///
/// `managed_funds` accetta un iterabile qualunque, non solo un `set`: un deserializer d'autore
/// non è tenuto a costruire proprio un set, e il riferimento (Pydantic) coercizzava una lista
/// senza protestare.
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
                Ok($shim(<$native>::build(&name, &managed_funds).map_err(value_error)?))
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

/// Attacca al modulo le tre tuple-alias (`Investment`, `AssetsManager`, `FundChangeName`).
///
/// Non sono `#[pyclass]`: sono tuple di tipi, costruibili solo dopo che i tipi esistono, cioè a
/// modulo già costruito. Vedi il doc-comment del modulo per il perché non sono classi base.
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
