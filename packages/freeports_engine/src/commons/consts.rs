//! Implementation-independent constants and classes.
//!
//! Rust port of `packages/freeports_core/src/freeports/_internals/commons/consts.py`'s three
//! enums (`FinancialInstrument`, `SfdrArticle`, `Currency`). See
//! `analysis_finance_reports/agent-memory/rust-rewrite-plan.md` for the migration this belongs
//! to, and the "Currency spike" discussion there for why the two lookup paths below
//! (value-based `__new__` vs. name-based `__class_getitem__`) are deliberately NOT the same:
//! that mirrors the current Python behavior exactly (`Currency("EUR")` works, `Currency("EURO")`
//! raises `ValueError`, but `Currency["EURO"]` succeeds as an alias of `Currency.EUR`).
//!
//! Known, accepted divergences from Python's `enum.Enum` (verified against the current
//! behavior before porting, not assumed):
//! - No singleton identity: `Currency("EUR") is Currency.EUR` is `False` here (`True` in
//!   Python). Nothing in this codebase compares these enums with `is`, only `==`/hashing, both
//!   of which behave identically to Python.
//! - `for e in Currency` (iterating the class itself) is not supported — PyO3 pyclasses can't
//!   easily gain a custom-metaclass `__iter__`. The only three call sites that did this
//!   (`output/files_schema.py`) were updated to iterate `Currency.__members__.values()` instead,
//!   which is supported and produces the same set of values.

use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyType};

/// Serialization mirrors Python `enum.Enum`'s default Pydantic behavior: dump to `.value` (via
/// `operator.attrgetter`, not a Rust closure, so it doesn't need `#[pyfunction]` machinery of
/// its own).
fn pydantic_value_ser_schema<'py>(
    py: Python<'py>,
    core_schema: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let value_getter = py
        .import("operator")?
        .call_method1("attrgetter", ("value",))?;
    core_schema.call_method1("plain_serializer_function_ser_schema", (value_getter,))
}

/// Builds a `pydantic_core.core_schema.is_instance_schema(cls, serialization=...)`: accepts only
/// an existing instance of `cls`, no coercion from raw values. Use for types with no meaningful
/// value-based constructor (`SfdrArticle`, `FinancialInstrument` — nothing in this codebase
/// constructs them from a raw int/string, only ever passes existing instances around).
fn pydantic_is_instance_schema(cls: &Bound<'_, PyType>) -> PyResult<Py<PyAny>> {
    let py = cls.py();
    let core_schema = py.import("pydantic_core")?.getattr("core_schema")?;
    let ser_schema = pydantic_value_ser_schema(py, &core_schema)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("serialization", ser_schema)?;
    let schema = core_schema.call_method("is_instance_schema", (cls,), Some(&kwargs))?;
    Ok(schema.into())
}

/// Builds a `pydantic_core.core_schema.no_info_plain_validator_function(cls, serialization=...)`:
/// validates by calling `cls(value)` — accepts both an existing instance (`cls.__new__`'s
/// fast-path, see `Currency::new`) and a raw value to coerce (e.g. a YAML/JSON string). This
/// replicates Python `enum.Enum`'s default Pydantic behavior for fields typed as a bare
/// `Currency` (not wrapped in `Annotated[..., BeforeValidator(...)]`) — see
/// `formats/repo/algorithms/semistructured/pdf_extract.py::InputStandardCostCurr.currency`, a
/// YAML-config model that receives a plain string like `"EUR"` and needs it coerced.
fn pydantic_coercing_schema(cls: &Bound<'_, PyType>) -> PyResult<Py<PyAny>> {
    let py = cls.py();
    let core_schema = py.import("pydantic_core")?.getattr("core_schema")?;
    let ser_schema = pydantic_value_ser_schema(py, &core_schema)?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("serialization", ser_schema)?;
    let schema =
        core_schema.call_method("no_info_plain_validator_function", (cls,), Some(&kwargs))?;
    Ok(schema.into())
}

#[pyclass(eq, frozen, hash, module = "freeports_engine")]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FinancialInstrument {
    EQUITY,
    BOND,
}

#[pymethods]
impl FinancialInstrument {
    fn __repr__(&self) -> String {
        format!("<FinancialInstrument.{0}: {1}>", self.name(), self.int_value())
    }

    fn __str__(&self) -> String {
        format!("FinancialInstrument.{}", self.name())
    }

    #[getter]
    fn value(&self) -> i32 {
        self.int_value()
    }

    #[getter]
    fn name(&self) -> &'static str {
        match self {
            FinancialInstrument::EQUITY => "EQUITY",
            FinancialInstrument::BOND => "BOND",
        }
    }

    /// Bug found while porting `output/classes_schema.py::FundSfdrClassification` (which
    /// round-trips an `SfdrArticle` through `core/serialization.py`'s tag scheme,
    /// `_tag_to_enum`'s `cls[value]`): the Fase-1 port of these three enums gave `Currency` a
    /// `__class_getitem__` but not `FinancialInstrument`/`SfdrArticle`, so `cls[value]` raised
    /// `TypeError: not subscriptable` for either — never caught because nothing exercised
    /// bracket-lookup reconstruction for them until now. Fixed here rather than deferred: small,
    /// squarely inside the enum port this migration already owns, not a downstream mystery.
    #[classmethod]
    fn __class_getitem__(_cls: &Bound<'_, PyType>, key: &str) -> PyResult<Self> {
        match key {
            "EQUITY" => Ok(FinancialInstrument::EQUITY),
            "BOND" => Ok(FinancialInstrument::BOND),
            _ => Err(PyKeyError::new_err(key.to_string())),
        }
    }

    #[classmethod]
    fn __get_pydantic_core_schema__(
        cls: &Bound<'_, PyType>,
        _source: &Bound<'_, PyAny>,
        _handler: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        pydantic_is_instance_schema(cls)
    }
}

impl FinancialInstrument {
    fn int_value(&self) -> i32 {
        match self {
            FinancialInstrument::EQUITY => 1,
            FinancialInstrument::BOND => 2,
        }
    }
}

#[pyclass(eq, frozen, hash, module = "freeports_engine")]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[allow(non_camel_case_types)]
pub enum SfdrArticle {
    ART_6,
    ART_8,
    ART_9,
}

#[pymethods]
impl SfdrArticle {
    fn __repr__(&self) -> String {
        format!("<SfdrArticle.{0}: {1}>", self.name(), self.int_value())
    }

    fn __str__(&self) -> String {
        format!("SfdrArticle.{}", self.name())
    }

    #[getter]
    fn value(&self) -> i32 {
        self.int_value()
    }

    #[getter]
    fn name(&self) -> &'static str {
        match self {
            SfdrArticle::ART_6 => "ART_6",
            SfdrArticle::ART_8 => "ART_8",
            SfdrArticle::ART_9 => "ART_9",
        }
    }

    /// See `FinancialInstrument::__class_getitem__`'s doc comment — same missing-since-Fase-1
    /// bug, same fix.
    #[classmethod]
    fn __class_getitem__(_cls: &Bound<'_, PyType>, key: &str) -> PyResult<Self> {
        match key {
            "ART_6" => Ok(SfdrArticle::ART_6),
            "ART_8" => Ok(SfdrArticle::ART_8),
            "ART_9" => Ok(SfdrArticle::ART_9),
            _ => Err(PyKeyError::new_err(key.to_string())),
        }
    }

    #[classmethod]
    fn __get_pydantic_core_schema__(
        cls: &Bound<'_, PyType>,
        _source: &Bound<'_, PyAny>,
        _handler: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        pydantic_is_instance_schema(cls)
    }
}

impl SfdrArticle {
    fn int_value(&self) -> i32 {
        match self {
            SfdrArticle::ART_6 => 1,
            SfdrArticle::ART_8 => 2,
            SfdrArticle::ART_9 => 3,
        }
    }
}

/// ISO 3-letter currency codes, in the same order as the original Python `Currency` enum
/// (order matters: it's the iteration/`__members__` order).
#[pyclass(eq, frozen, hash, module = "freeports_engine")]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Currency {
    USD,
    EUR,
    GBP,
    JPY,
    CNY,
    AUD,
    CAD,
    CHF,
    CNH,
    SEK,
    NOK,
    DKK,
    SGD,
    HKD,
    KRW,
    INR,
    BRL,
    MXN,
    RUB,
    ZAR,
    TRY,
    PLN,
    THB,
    IDR,
    MYR,
    PHP,
    ILS,
    AED,
    SAR,
    QAR,
    KWD,
    CLP,
    COP,
    PEN,
    ARS,
    VND,
    UAH,
    CZK,
    HUF,
    RON,
    HRK,
    BGN,
    ISK,
    NZD,
    EGP,
    TWD,
}

impl Currency {
    /// All 46 canonical members, in declaration order.
    pub fn variants() -> &'static [Currency] {
        use Currency::*;
        &[
            USD, EUR, GBP, JPY, CNY, AUD, CAD, CHF, CNH, SEK, NOK, DKK, SGD, HKD, KRW, INR, BRL,
            MXN, RUB, ZAR, TRY, PLN, THB, IDR, MYR, PHP, ILS, AED, SAR, QAR, KWD, CLP, COP, PEN,
            ARS, VND, UAH, CZK, HUF, RON, HRK, BGN, ISK, NZD, EGP, TWD,
        ]
    }

    pub fn code(&self) -> &'static str {
        match self {
            Currency::USD => "USD",
            Currency::EUR => "EUR",
            Currency::GBP => "GBP",
            Currency::JPY => "JPY",
            Currency::CNY => "CNY",
            Currency::AUD => "AUD",
            Currency::CAD => "CAD",
            Currency::CHF => "CHF",
            Currency::CNH => "CNH",
            Currency::SEK => "SEK",
            Currency::NOK => "NOK",
            Currency::DKK => "DKK",
            Currency::SGD => "SGD",
            Currency::HKD => "HKD",
            Currency::KRW => "KRW",
            Currency::INR => "INR",
            Currency::BRL => "BRL",
            Currency::MXN => "MXN",
            Currency::RUB => "RUB",
            Currency::ZAR => "ZAR",
            Currency::TRY => "TRY",
            Currency::PLN => "PLN",
            Currency::THB => "THB",
            Currency::IDR => "IDR",
            Currency::MYR => "MYR",
            Currency::PHP => "PHP",
            Currency::ILS => "ILS",
            Currency::AED => "AED",
            Currency::SAR => "SAR",
            Currency::QAR => "QAR",
            Currency::KWD => "KWD",
            Currency::CLP => "CLP",
            Currency::COP => "COP",
            Currency::PEN => "PEN",
            Currency::ARS => "ARS",
            Currency::VND => "VND",
            Currency::UAH => "UAH",
            Currency::CZK => "CZK",
            Currency::HUF => "HUF",
            Currency::RON => "RON",
            Currency::HRK => "HRK",
            Currency::BGN => "BGN",
            Currency::ISK => "ISK",
            Currency::NZD => "NZD",
            Currency::EGP => "EGP",
            Currency::TWD => "TWD",
        }
    }

    fn symbol_str(&self) -> &'static str {
        match self {
            Currency::USD => "$",
            Currency::EUR => "€",
            Currency::GBP => "£",
            Currency::JPY => "¥",
            Currency::CNY => "¥",
            Currency::AUD => "$",
            Currency::CAD => "$",
            Currency::CHF => "CHF",
            Currency::CNH => "¥",
            Currency::SEK => "kr",
            Currency::NOK => "kr",
            Currency::DKK => "kr",
            Currency::SGD => "$",
            Currency::HKD => "$",
            Currency::KRW => "₩",
            Currency::INR => "₹",
            Currency::BRL => "R$",
            Currency::MXN => "$",
            Currency::RUB => "₽",
            Currency::ZAR => "R",
            Currency::TRY => "₺",
            Currency::PLN => "zł",
            Currency::THB => "฿",
            Currency::IDR => "Rp",
            Currency::MYR => "RM",
            Currency::PHP => "₱",
            Currency::ILS => "₪",
            Currency::AED => "د.إ",
            Currency::SAR => "﷼",
            Currency::QAR => "ر.ق",
            Currency::KWD => "د.ك",
            Currency::CLP => "$",
            Currency::COP => "$",
            Currency::PEN => "S/.",
            Currency::ARS => "$",
            Currency::VND => "₫",
            Currency::UAH => "₴",
            Currency::CZK => "Kč",
            Currency::HUF => "Ft",
            Currency::RON => "lei",
            Currency::HRK => "kn",
            Currency::BGN => "лв",
            Currency::ISK => "kr",
            Currency::NZD => "$",
            Currency::EGP => "ج.م",
            Currency::TWD => "$",
        }
    }

    /// Value-based lookup: exact ISO code match only, no aliases. Mirrors Python's
    /// `Currency(value)` (which uses `_value2member_map_` — aliases don't get their own entry).
    fn from_code(code: &str) -> Option<Currency> {
        Currency::variants()
            .iter()
            .copied()
            .find(|c| c.code() == code)
    }

    /// Name-based lookup: accepts both canonical member names and the `EURO` alias for `EUR`.
    /// Mirrors Python's `Currency[name]` (`_member_map_`, which does include aliases). `pub(crate)`
    /// so `core/cast.rs`'s `to_currency` can reuse it (it needs the same alias-aware lookup).
    pub(crate) fn from_name(name: &str) -> Option<Currency> {
        if name == "EURO" {
            return Some(Currency::EUR);
        }
        Currency::variants()
            .iter()
            .copied()
            .find(|c| c.code() == name)
    }
}

#[pymethods]
impl Currency {
    /// Mirrors Python `EnumMeta.__call__`'s fast path: calling the class with an existing
    /// member of that class returns it unchanged (`Currency(Currency.EUR) is Currency.EUR`),
    /// rather than trying to look it up as a string. `core/promises.py::try_convert_to_currency`
    /// relies on this — it's called on already-deserialized `Currency` instances too, not just
    /// raw strings, when round-tripping through JSON (`core/serialization.py`).
    #[new]
    pub fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(existing) = value.extract::<Currency>() {
            return Ok(existing);
        }
        let code: String = value.extract::<String>().map_err(|_| {
            PyValueError::new_err(format!("{value:?} is not a valid Currency"))
        })?;
        let code: &str = &code;
        Currency::from_code(code)
            .ok_or_else(|| PyValueError::new_err(format!("{code:?} is not a valid Currency")))
    }

    #[getter]
    fn value(&self) -> &'static str {
        self.code()
    }

    #[getter]
    fn name(&self) -> &'static str {
        self.code()
    }

    /// Currency symbol for this currency (e.g. `"€"` for `EUR`).
    #[getter]
    fn symbol(&self) -> &'static str {
        self.symbol_str()
    }

    fn __repr__(&self) -> String {
        format!("<Currency.{0}: '{0}'>", self.code())
    }

    fn __str__(&self) -> String {
        format!("Currency.{}", self.code())
    }

    #[classmethod]
    fn __class_getitem__(_cls: &Bound<'_, PyType>, key: &str) -> PyResult<Self> {
        Currency::from_name(key).ok_or_else(|| PyKeyError::new_err(key.to_string()))
    }

    /// Name -> member map, including the `EURO` alias (matching Python `Enum.__members__`,
    /// which lists aliases alongside canonical names, both pointing at the same member).
    #[classattr]
    fn __members__(py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        for v in Currency::variants() {
            dict.set_item(v.code(), *v)?;
        }
        dict.set_item("EURO", Currency::EUR)?;
        Ok(dict.into())
    }

    #[classmethod]
    fn __get_pydantic_core_schema__(
        cls: &Bound<'_, PyType>,
        _source: &Bound<'_, PyAny>,
        _handler: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        pydantic_coercing_schema(cls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_lookup_finds_exact_code() {
        assert_eq!(Currency::from_code("EUR"), Some(Currency::EUR));
    }

    #[test]
    fn value_lookup_rejects_alias() {
        assert_eq!(Currency::from_code("EURO"), None);
    }

    #[test]
    fn name_lookup_accepts_alias() {
        assert_eq!(Currency::from_name("EURO"), Some(Currency::EUR));
    }

    #[test]
    fn name_lookup_finds_canonical_name() {
        assert_eq!(Currency::from_name("USD"), Some(Currency::USD));
    }

    #[test]
    fn unknown_code_is_none() {
        assert_eq!(Currency::from_code("XXX"), None);
        assert_eq!(Currency::from_name("XXX"), None);
    }

    #[test]
    fn has_46_canonical_members() {
        assert_eq!(Currency::variants().len(), 46);
    }

    #[test]
    fn every_variant_has_a_distinct_code() {
        let mut codes: Vec<&str> = Currency::variants().iter().map(|c| c.code()).collect();
        let n = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), n, "duplicate currency code found");
    }

    #[test]
    fn symbol_matches_known_values() {
        assert_eq!(Currency::EUR.symbol_str(), "€");
        assert_eq!(Currency::USD.symbol_str(), "$");
        assert_eq!(Currency::CHF.symbol_str(), "CHF");
        assert_eq!(Currency::JPY.symbol_str(), "¥");
    }

    #[test]
    fn every_variant_has_a_symbol() {
        for c in Currency::variants() {
            assert!(!c.symbol_str().is_empty());
        }
    }

    #[test]
    fn sfdr_article_values_match_python_auto_numbering() {
        assert_eq!(SfdrArticle::ART_6.int_value(), 1);
        assert_eq!(SfdrArticle::ART_8.int_value(), 2);
        assert_eq!(SfdrArticle::ART_9.int_value(), 3);
    }

    #[test]
    fn financial_instrument_values_match_python_auto_numbering() {
        assert_eq!(FinancialInstrument::EQUITY.int_value(), 1);
        assert_eq!(FinancialInstrument::BOND.int_value(), 2);
    }

    /// Regression test for the bug documented on `SfdrArticle::__class_getitem__`/
    /// `FinancialInstrument::__class_getitem__`: `cls[value]` (used by
    /// `core/serialization.py::_tag_to_enum`) raised `TypeError: not subscriptable` for these
    /// two types before this was added — verified via the real Python bracket-lookup syntax,
    /// not the Rust classmethod directly (constructing a `&Bound<PyType>` by hand is awkward and
    /// the point is to prove the Python-visible behavior).
    #[test]
    fn sfdr_article_and_financial_instrument_support_bracket_lookup() {
        Python::attach(|py| {
            let sfdr_type = py.get_type::<SfdrArticle>();
            let sfdr: SfdrArticle = sfdr_type.get_item("ART_8").unwrap().extract().unwrap();
            assert_eq!(sfdr, SfdrArticle::ART_8);

            let fi_type = py.get_type::<FinancialInstrument>();
            let fi: FinancialInstrument = fi_type.get_item("BOND").unwrap().extract().unwrap();
            assert_eq!(fi, FinancialInstrument::BOND);
        });
    }
}
