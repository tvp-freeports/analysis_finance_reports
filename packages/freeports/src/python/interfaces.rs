//! Shim di `freeports.interfaces`: i nomi dei tipi di blocco che collegano i pipe d'autore a
//! quelli standard, e i tre costruttori di `TextBlock` standard.
//!
//! # Perché sono enum di stringhe
//!
//! Nel crate nuovo il tipo di un blocco è una **stringa** in un newtype (`core::classes::
//! BlockType`, decisione D2 di `PLAN.md`): i repo formati estendono liberamente l'insieme dei
//! tipi, quindi un enum chiuso sarebbe sbagliato. I quattro "enum" che il codice d'autore importa
//! sono perciò cataloghi di nomi: ogni membro porta la stessa stringa che `BlockType` usa già, ed
//! è quella che `.name` restituisce — che è l'unico modo in cui il repo formati li usa
//! (`ResultStandardExtraction.FUND_NAME.name`).

use pyo3::prelude::*;
use pyo3::types::PyType;

use crate::core::classes::{BlockType, PdfBlock};
use crate::core::match_fund::MatchFund;
use crate::formats_utils::text_filter::standard_txt_blk_builders as builders;

use super::core::{PyPdfBlock, PyTextBlock};
use super::utils::text_filter::PyMatchFund;

/// Genera un catalogo di nomi di tipo blocco con il protocollo `enum.Enum` che il codice
/// d'autore usa: `Tipo.MEMBRO`, `.name`, `.value`, uguaglianza, hash, `Tipo['MEMBRO']`.
///
/// I membri sono attaccati a runtime da [`init`], non come `#[classattr]`: un `#[classattr]` non
/// può essere generato in numero variabile a partire da una lista.
macro_rules! block_type_catalog {
    ($shim:ident, $py_name:literal, [$($member:ident),+ $(,)?]) => {
        #[doc = concat!("Shim Python dell'enum `", $py_name, "` del riferimento.")]
        #[pyclass(name = $py_name, module = "freeports.interfaces", frozen, eq, hash)]
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $shim(&'static str);

        impl $shim {
            /// I nomi dei membri, nell'ordine di dichiarazione del riferimento.
            pub const MEMBERS: &'static [&'static str] = &[$(stringify!($member)),+];
        }

        #[pymethods]
        impl $shim {
            #[getter]
            fn name(&self) -> &'static str {
                self.0
            }

            /// `value` è il nome stesso: è la stringa che finisce in `type_block`, ed è ciò che
            /// il codice d'autore confronta.
            #[getter]
            fn value(&self) -> &'static str {
                self.0
            }

            fn __repr__(&self) -> String {
                format!("<{}.{}>", $py_name, self.0)
            }

            fn __str__(&self) -> String {
                format!("{}.{}", $py_name, self.0)
            }

            #[classmethod]
            fn __class_getitem__(_cls: &Bound<'_, PyType>, name: &str) -> PyResult<$shim> {
                Self::MEMBERS
                    .iter()
                    .find(|m| **m == name)
                    .map(|m| $shim(m))
                    .ok_or_else(|| {
                        // Same rationale as `python/consts.rs`'s enum shims: past this point the
                        // failure only lives as a Python `KeyError`.
                        tracing::error!(catalog = $py_name, name, "unknown block type name");
                        pyo3::exceptions::PyKeyError::new_err(format!("{}: {name}", $py_name))
                    })
            }

            #[classattr]
            fn __members__() -> std::collections::BTreeMap<&'static str, $shim> {
                Self::MEMBERS.iter().map(|m| (*m, $shim(m))).collect()
            }
        }
    };
}

block_type_catalog!(PyOnePdfBlockType, "OnePdfBlockType", [RELEVANT_BLOCK]);
block_type_catalog!(PyOneTextBlockType, "OneTextBlockType", [RELEVANT_BLOCK]);
block_type_catalog!(
    PyResultStandardExtraction,
    "ResultStandardExtraction",
    [FUND_NAME, CURRENCY_STATEMENT, TABLE_BODY, MANAGEMENT_COMPANY, INVESTMENTS_MANAGER, SFDR_ARTICLE, PAGE_CLASS]
);
block_type_catalog!(
    PyResultStandardFiltering,
    "ResultStandardFiltering",
    [BOND_TARGET, EQUITY_TARGET, FUND, MANAGEMENT_COMPANY, INVESTMENTS_MANAGER, SFDR_ARTICLE, PAGE_CLASS]
);

/// I fondi passati a un costruttore di blocco standard: un iterabile di `MatchFund`.
fn match_funds(funds: &Bound<'_, PyAny>) -> PyResult<std::collections::BTreeSet<MatchFund>> {
    funds
        .try_iter()?
        .map(|item| Ok(item?.extract::<PyRef<'_, PyMatchFund>>()?.inner().clone()))
        .collect()
}

/// I tre costruttori di `TextBlock` standard.
///
/// # Perché sono *istanze* di una classe e non classi
///
/// Nel riferimento erano classi il cui `__new__` restituiva un `TextBlock` invece di un'istanza
/// della classe stessa — una forma che esiste solo per essere chiamata. PyO3 non sa esprimerlo:
/// un `#[new]` deve costruire il proprio tipo. Ognuno di questi è quindi un oggetto **callable**
/// registrato nel modulo sotto il nome pubblico: `StandardFundTextBlock(pdf_blk)` passa da
/// `__call__` e `StandardFundTextBlock.from_content(...)` da un metodo, che è esattamente ciò che
/// il repo formati scrive. La stessa limitazione era già annotata nel riferimento, che per
/// aggirarla teneva tre classi Python vere.
macro_rules! standard_txt_blk {
    ($shim:ident, $py_name:literal, $from_block:path, $from_content:path) => {
        #[doc = concat!("Shim Python di `", $py_name, "`.")]
        #[pyclass(name = $py_name, module = "freeports.interfaces", frozen)]
        pub struct $shim;

        // `from_content`/`from_name` prendono `&self` benche' clippy si aspetti il contrario da un
        // `from_*`: qui non sono costruttori del proprio tipo ma metodi di un oggetto *callable*
        // registrato nel modulo (vedi il doc-comment del macro), e un metodo di un'istanza deve
        // prendere `self`. Il nome e' quello del riferimento e non e' negoziabile.
        #[allow(clippy::wrong_self_convention)]
        #[pymethods]
        impl $shim {
            fn __call__(
                &self,
                py: Python<'_>,
                pdf_blk: PyRef<'_, PyPdfBlock>,
                funds: &Bound<'_, PyAny>,
            ) -> PyResult<PyTextBlock> {
                let funds = match_funds(funds)?;
                let block = $from_block(pdf_blk.native(py)?, &funds);
                PyTextBlock::from_native(py, &block)
            }

            fn from_content(&self, py: Python<'_>, name: &str, funds: &Bound<'_, PyAny>) -> PyResult<PyTextBlock> {
                let funds = match_funds(funds)?;
                let block = $from_content(name, &funds);
                PyTextBlock::from_native(py, &block)
            }

            /// Alias storico di `from_content`, mantenuto perché il riferimento lo esponeva.
            fn from_name(&self, py: Python<'_>, name: &str, funds: &Bound<'_, PyAny>) -> PyResult<PyTextBlock> {
                self.from_content(py, name, funds)
            }
        }
    };
}

standard_txt_blk!(
    PyStandardManagmentCompanyTextBlock,
    "StandardManagmentCompanyTextBlock",
    builders::standard_management_company_txt_blk,
    builders::standard_management_company_txt_blk_from_content
);
standard_txt_blk!(
    PyStandardInvestmentsMangerTextBlock,
    "StandardInvestmentsMangerTextBlock",
    builders::standard_investmet_manager_txt_blk,
    builders::standard_investmet_manager_txt_blk_from_content
);

/// Il blocco di un fondo: a differenza degli altri due non porta un insieme di fondi gestiti,
/// quindi non passa dal macro sopra.
#[pyclass(name = "StandardFundTextBlock", module = "freeports.interfaces", frozen)]
pub struct PyStandardFundTextBlock;

// Vedi la nota su `clippy::wrong_self_convention` nel macro `standard_txt_blk!`.
#[allow(clippy::wrong_self_convention)]
#[pymethods]
impl PyStandardFundTextBlock {
    fn __call__(&self, py: Python<'_>, pdf_blk: PyRef<'_, PyPdfBlock>) -> PyResult<PyTextBlock> {
        let block = builders::standard_fund_txt_blk(pdf_blk.native(py)?);
        PyTextBlock::from_native(py, &block)
    }

    fn from_content(&self, py: Python<'_>, fund: &str) -> PyResult<PyTextBlock> {
        PyTextBlock::from_native(py, &builders::standard_fund_txt_blk_from_content(fund))
    }

    fn from_name(&self, py: Python<'_>, fund: &str) -> PyResult<PyTextBlock> {
        self.from_content(py, fund)
    }

    /// Da un `MatchFund`, di cui prende il nome: e' la forma che i moduli d'autore usano quando
    /// il fondo arriva dal confronto con le societa' bersaglio invece che da un blocco PDF.
    fn from_matched_fund(&self, py: Python<'_>, fund: &Bound<'_, PyAny>) -> PyResult<PyTextBlock> {
        let name: String = match fund.extract::<PyRef<'_, crate::python::utils::text_filter::PyMatchFund>>() {
            Ok(matched) => matched.inner().name().to_string(),
            Err(_) => fund.getattr("name")?.extract()?,
        };
        self.from_content(py, &name)
    }
}

/// Attacca ai due moduli i membri dei quattro cataloghi.
///
/// Il controllo che ogni nome sia anche un `BlockType` standard non è decorativo: è ciò che
/// impedisce ai due elenchi di divergere in silenzio, dato che vivono in due posti.
pub fn init(pdf_blks: &Bound<'_, PyModule>, text_blks: &Bound<'_, PyModule>) -> PyResult<()> {
    debug_assert!(
        [
            PyOnePdfBlockType::MEMBERS,
            PyOneTextBlockType::MEMBERS,
            PyResultStandardExtraction::MEMBERS,
            PyResultStandardFiltering::MEMBERS,
        ]
        .iter()
        .flat_map(|members| members.iter())
        .all(|name| BlockType::STANDARD.iter().any(|t| t.as_str() == *name)),
        "every catalog member must be a standard BlockType"
    );

    attach(pdf_blks, "OnePdfBlockType", PyOnePdfBlockType::MEMBERS, PyOnePdfBlockType)?;
    attach(pdf_blks, "ResultStandardExtraction", PyResultStandardExtraction::MEMBERS, PyResultStandardExtraction)?;
    attach(text_blks, "OneTextBlockType", PyOneTextBlockType::MEMBERS, PyOneTextBlockType)?;
    attach(text_blks, "ResultStandardFiltering", PyResultStandardFiltering::MEMBERS, PyResultStandardFiltering)?;

    // I tre costruttori sono oggetti callable, non classi: vedi il doc di `standard_txt_blk!`.
    let py = text_blks.py();
    text_blks.setattr("StandardManagmentCompanyTextBlock", Bound::new(py, PyStandardManagmentCompanyTextBlock)?)?;
    text_blks.setattr("StandardInvestmentsMangerTextBlock", Bound::new(py, PyStandardInvestmentsMangerTextBlock)?)?;
    text_blks.setattr("StandardFundTextBlock", Bound::new(py, PyStandardFundTextBlock)?)?;
    Ok(())
}

/// Attacca i membri di un catalogo al proprio oggetto-tipo.
fn attach<T, F>(module: &Bound<'_, PyModule>, py_name: &str, members: &[&'static str], make: F) -> PyResult<()>
where
    T: for<'py> IntoPyObject<'py>,
    F: Fn(&'static str) -> T,
{
    let class = module.getattr(py_name)?;
    for member in members {
        class.setattr(*member, make(member))?;
    }
    Ok(())
}

/// Il `PdfBlock` è il solo tipo che questo modulo importa da `core` per costruire i blocchi
/// standard; l'`use` è qui per tenere il compilatore onesto sul fatto che il builder lo pretende.
const _: fn(PdfBlock, &std::collections::BTreeSet<MatchFund>) -> crate::core::classes::TextBlock =
    builders::standard_management_company_txt_blk;
