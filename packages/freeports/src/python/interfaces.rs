//! The shims of the block-type catalogues that connect author pipes to the standard ones, and the
//! three standard text-block builders.
//!
//! # Why they are catalogues of names
//!
//! A block's type is a **string** in a newtype, because formats repositories extend the set of
//! types freely and a closed enumeration would be wrong. The four "enumerations" author code
//! imports are therefore catalogues of names: each member carries the same string the native type
//! uses, and that string is what the name attribute returns — the only way a formats repository
//! uses them.

use pyo3::prelude::*;
use pyo3::types::PyType;

use crate::core::classes::{BlockType, PdfBlock};
use crate::core::match_fund::MatchFund;
use crate::formats_utils::text_filter::standard_txt_blk_builders as builders;

use super::core::{PyPdfBlock, PyTextBlock};
use super::utils::text_filter::PyMatchFund;

/// Generates a block-type catalogue with the enumeration protocol author code uses: member access,
/// name and value, equality, hashing, and indexing by name.
///
/// The members are attached at runtime rather than declared, because a declared attribute cannot be
/// generated in variable number from a list.
macro_rules! block_type_catalog {
    ($shim:ident, $py_name:literal, [$($member:ident),+ $(,)?]) => {
        #[doc = concat!("Shim Python dell'enum `", $py_name, "` del riferimento.")]
        #[pyclass(name = $py_name, module = "freeports.interfaces", frozen, eq, hash)]
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $shim(&'static str);

        impl $shim {
            /// The member names, in declaration order.
            pub const MEMBERS: &'static [&'static str] = &[$(stringify!($member)),+];
        }

        #[pymethods]
        impl $shim {
            #[getter]
            fn name(&self) -> &'static str {
                self.0
            }

            /// The value is the name itself: it is the string that ends up as a block's type, and
            /// what author code compares against.
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

/// The funds passed to a standard block builder: an iterable of fund identities.
fn match_funds(funds: &Bound<'_, PyAny>) -> PyResult<std::collections::BTreeSet<MatchFund>> {
    funds
        .try_iter()?
        .map(|item| Ok(item?.extract::<PyRef<'_, PyMatchFund>>()?.inner().clone()))
        .collect()
}

/// The three standard text-block builders.
///
/// # Why they are *instances* of a class rather than classes
///
/// They used to be classes whose constructor returned a block instead of an instance of themselves
/// — a shape that exists only to be called. PyO3 cannot express that: a constructor must build its
/// own type. Each of these is therefore a **callable object** registered in the module under the
/// public name, so that calling it goes through the call protocol and its named constructor through
/// a method — exactly what a formats repository writes.
macro_rules! standard_txt_blk {
    ($shim:ident, $py_name:literal, $from_block:path, $from_content:path) => {
        #[doc = concat!("Shim Python di `", $py_name, "`.")]
        #[pyclass(name = $py_name, module = "freeports.interfaces", frozen)]
        pub struct $shim;

        // These take a reference to self despite their `from_` names: they are not constructors of
        // their own type but methods of a callable object registered in the module, and a method of
        // an instance must take self. The names are the public ones and are not negotiable.
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

            /// A historical alias of the content constructor, kept because it was exposed.
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

/// The fund block: unlike the other two it carries no set of managed funds, so it does not go
/// through the macro above.
#[pyclass(name = "StandardFundTextBlock", module = "freeports.interfaces", frozen)]
pub struct PyStandardFundTextBlock;

// See the note on the self convention in the macro above.
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

    /// From a fund identity, taking its name: the form author modules use when the fund comes from
    /// matching against the target companies rather than from a PDF block.
    fn from_matched_fund(&self, py: Python<'_>, fund: &Bound<'_, PyAny>) -> PyResult<PyTextBlock> {
        let name: String = match fund.extract::<PyRef<'_, crate::python::utils::text_filter::PyMatchFund>>() {
            Ok(matched) => matched.inner().name().to_string(),
            Err(_) => fund.getattr("name")?.extract()?,
        };
        self.from_content(py, &name)
    }
}

/// Attaches the members of the four catalogues to the two modules.
///
/// Checking that every name is also a standard block type is not decorative: it is what stops the
/// two lists from drifting apart, living as they do in two places.
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

    // The three builders are callable objects, not classes; see the macro above.
    let py = text_blks.py();
    text_blks.setattr("StandardManagmentCompanyTextBlock", Bound::new(py, PyStandardManagmentCompanyTextBlock)?)?;
    text_blks.setattr("StandardInvestmentsMangerTextBlock", Bound::new(py, PyStandardInvestmentsMangerTextBlock)?)?;
    text_blks.setattr("StandardFundTextBlock", Bound::new(py, PyStandardFundTextBlock)?)?;
    Ok(())
}

/// Attaches a catalogue's members to its own type object.
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

/// The PDF block is the only type this module imports from the engine to build the standard blocks;
/// the import is here to keep the compiler honest about the builder requiring it.
const _: fn(PdfBlock, &std::collections::BTreeSet<MatchFund>) -> crate::core::classes::TextBlock =
    builders::standard_management_company_txt_blk;
