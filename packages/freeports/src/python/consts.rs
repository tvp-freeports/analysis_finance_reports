//! Shim di `commons::consts`: `Currency`, `SfdrArticle`, `FinancialInstrument`.
//!
//! I tre tipi nativi sono `enum` Rust senza campi. Non sono annotati `#[pyclass]` — il layer di
//! shim non tocca il codice esistente (vedi il doc-comment di [`super::convert`]) — quindi qui
//! vivono tre newtype che li avvolgono ed espongono lo stesso protocollo che il codice d'autore
//! dei repo formati usa già: `Currency.EUR` come attributo di classe, `.name`/`.value`,
//! uguaglianza e hash.
//!
//! Le costanti di classe sono generate dal macro [`enum_shim`] a partire dall'elenco di varianti
//! che il tipo nativo espone già (`Currency::variants()` e i due `VARIANTS` equivalenti scritti
//! qui per gli altri due, che non ne hanno uno): scriverle a mano avrebbe voluto dire
//! quarantasei `#[classattr]` per la sola `Currency`.

use pyo3::prelude::*;
use pyo3::types::PyType;

use crate::commons::consts::{Currency, FinancialInstrument, SfdrArticle};

/// Genera il newtype `#[pyclass]` di un enum senza campi: costruzione per nome, `name`/`value`,
/// `__eq__`/`__hash__`/`__repr__`/`__str__`, l'accesso `Tipo[NOME]` e la mappa
/// `__members__` — cioè la parte del protocollo `enum.Enum` che il codice esistente usa.
macro_rules! enum_shim {
    ($shim:ident, $native:ty, $py_name:literal, $variants:expr, $name_of:expr) => {
        #[doc = concat!("Shim Python di [`", stringify!($native), "`].")]
        #[pyclass(name = $py_name, module = "freeports.consts", frozen, eq, hash)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $shim($native);

        impl From<$native> for $shim {
            fn from(value: $native) -> Self {
                $shim(value)
            }
        }

        impl $shim {
            /// Il valore nativo avvolto — l'unico modo in cui il resto del layer lo legge.
            pub fn inner(&self) -> $native {
                self.0
            }

            /// Il nome della variante, che è anche la chiave con cui Python la indirizza.
            fn variant_name(value: $native) -> &'static str {
                #[allow(clippy::redundant_closure_call)]
                ($name_of)(value)
            }

            /// Il nome pubblico della variante avvolta.
            pub fn variant_name_of(&self) -> &'static str {
                Self::variant_name(self.0)
            }

            fn by_name(name: &str) -> Option<$shim> {
                $variants.iter().copied().find(|v| Self::variant_name(*v) == name).map($shim)
            }
        }

        #[pymethods]
        impl $shim {
            #[getter]
            fn name(&self) -> &'static str {
                Self::variant_name(self.0)
            }

            /// `value` è il nome stesso: gli enum del riferimento Python sono `str`-valued, e i
            /// repo formati leggono l'uno o l'altro indifferentemente.
            #[getter]
            fn value(&self) -> &'static str {
                Self::variant_name(self.0)
            }

            fn __repr__(&self) -> String {
                format!("{}.{}", $py_name, Self::variant_name(self.0))
            }

            fn __str__(&self) -> String {
                self.__repr__()
            }

            /// `Tipo[NOME]`, il lookup per nome di `enum.Enum`.
            #[classmethod]
            fn __class_getitem__(_cls: &Bound<'_, PyType>, name: &str) -> PyResult<$shim> {
                Self::by_name(name).ok_or_else(|| {
                    pyo3::exceptions::PyKeyError::new_err(format!("{}: {name}", $py_name))
                })
            }

            #[classattr]
            fn __members__() -> std::collections::BTreeMap<&'static str, $shim> {
                $variants.iter().copied().map(|v| (Self::variant_name(v), $shim(v))).collect()
            }
        }
    };
}

/// Le tre varianti di `SfdrArticle`, nell'ordine di dichiarazione del tipo nativo.
const SFDR_VARIANTS: [SfdrArticle; 3] = [SfdrArticle::Art6, SfdrArticle::Art8, SfdrArticle::Art9];
/// Le due varianti di `FinancialInstrument`, nell'ordine di dichiarazione del tipo nativo.
const INSTRUMENT_VARIANTS: [FinancialInstrument; 2] =
    [FinancialInstrument::EQUITY, FinancialInstrument::BOND];

enum_shim!(PyCurrency, Currency, "Currency", Currency::variants(), |v: Currency| v.code());
// I nomi visibili da Python sono `ART_6`/`ART_8`/`ART_9`, non gli identificatori Rust
// `Art6`/`Art8`/`Art9`: sono quelli che il codice d'autore scrive (`SfdrArticle.ART_6` in
// `eurizon_it24.py`) e quelli con cui le fixture gia' registrate nominano la variante. La
// convenzione di scrittura del crate resta `CamelCase`; il nome pubblico e' un'altra cosa.
enum_shim!(PySfdrArticle, SfdrArticle, "SfdrArticle", SFDR_VARIANTS, |v: SfdrArticle| match v {
    SfdrArticle::Art6 => "ART_6",
    SfdrArticle::Art8 => "ART_8",
    SfdrArticle::Art9 => "ART_9",
});
enum_shim!(
    PyFinancialInstrument,
    FinancialInstrument,
    "FinancialInstrument",
    INSTRUMENT_VARIANTS,
    |v: FinancialInstrument| match v {
        FinancialInstrument::EQUITY => "EQUITY",
        FinancialInstrument::BOND => "BOND",
    }
);

/// Aggiunge a un modulo shim le costanti di classe di un enum (`Currency.EUR` e sorelle).
///
/// Non sono `#[classattr]` perché un `#[classattr]` non può essere generato in numero variabile:
/// si registrano sul tipo dopo che il modulo è stato costruito, che è l'unico punto in cui
/// l'oggetto-tipo Python esiste davvero.
fn attach_variants<T>(module: &Bound<'_, PyModule>, py_name: &str, values: Vec<(&str, T)>) -> PyResult<()>
where
    T: for<'py> IntoPyObject<'py>,
{
    let class = module.getattr(py_name)?;
    for (name, value) in values {
        class.setattr(name, value)?;
    }
    Ok(())
}

/// Registra le costanti di classe dei tre enum. Chiamata da [`super::register`] subito dopo la
/// costruzione del modulo.
pub fn init(module: &Bound<'_, PyModule>) -> PyResult<()> {
    attach_variants(
        module,
        "Currency",
        Currency::variants().iter().map(|v| (v.code(), PyCurrency::from(*v))).collect(),
    )?;
    attach_variants(
        module,
        "SfdrArticle",
        SFDR_VARIANTS.iter().map(|v| (PySfdrArticle::variant_name(*v), PySfdrArticle::from(*v))).collect(),
    )?;
    attach_variants(
        module,
        "FinancialInstrument",
        INSTRUMENT_VARIANTS
            .iter()
            .map(|v| (PyFinancialInstrument::variant_name(*v), PyFinancialInstrument::from(*v)))
            .collect(),
    )?;
    Ok(())
}
