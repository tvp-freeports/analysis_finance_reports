"""Utilities for writing `pdf_extract` functions.

This module provides decorators and utilities for filtering and processing PDF content
based on XML elements, fonts, and positional data.

``PdfExtractSfdrArticleStandard``, ``PdfExtractCurrencyConstant``, ``PdfExtractPageClassifyStandard``,
``PdfExtractInvestmentsStandard``, and ``PdfExtractAssetsStandard`` are now implemented in Rust —
see ``packages/freeports_engine/src/pdf_extract/standard_funcs.rs`` and
``analysis_finance_reports/agent-memory/rust-rewrite-plan.md``. They call back into this module's
own ``TablePosAlgorithm``/``get_table_coordinates``/``CollapseAlgorithm``/``TableConfig`` (from
``position.py``) and into ``freeports._native.core.PdfLineSelection`` (``PdfLineSelection`` used to
live in the separate ``freeports_lib`` crate, merged into ``freeports_engine`` in Fase E) generically,
exactly the way the Python originals did — scoped this way deliberately
(user confirmed, 2026-08-19) so that once those pieces are themselves ported to Rust, the
round-trips collapse away without needing another pass over this file. ``PdfExtractFundStandard``/
``PdfExtractCurrencyStandard``/``PdfExtractManagmentCompanyStandard`` were already turned into
thin Python factories over the Rust-backed ``ExtractTextPdfBlockOrFailPage`` during the
`pdf_extract/common.py` port and are unchanged here.

The original (pre-Rust-port) ``_LegacyPdfExtractSfdrArticleStandard``/
``_LegacyPdfExtractCurrencyConstant``/``_LegacyPdfExtractPageClassifyStandard``/
``_LegacyPdfExtractInvestmentsStandard``/``_LegacyPdfExtractAssetsStandard`` dead-code class bodies
this module used to keep for reference, and the 3 unused ``UpdateMetadataFunc``/``FilterCondition``/
``PdfFilterFunc`` type aliases that had no consumer anywhere (in this file or outside it, confirmed
via grep) even before those classes were removed, were dropped during the freeports_core ->
freeports_engine consolidation (see
``analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md``). ``PdfBlock``/
``TextBlock``/``ExpectedPdfBlockNotFound``/``PageParseFail``/``SelectExpectedText``/
``ExtractTextPdfBlockOrFailPage``/``Promise`` (formerly re-exported via now-removed
``_internals.core.classes``/``_internals.core.promises``/``_internals.formats.utils.pdf_extract.
common`` alias-only shim modules) are imported directly from ``freeports._native.core`` here per
that same consolidation's Decision 3.
"""

from freeports import _native
from freeports.utils.pdf_extract import PdfLineSelection
from freeports.interfaces.pdf_blks import ResultStandardExtraction

ExtractTextPdfBlockOrFailPage = _native.core.ExtractTextPdfBlockOrFailPage

PdfExtractSfdrArticleStandard = _native.core.PdfExtractSfdrArticleStandard
PdfExtractCurrencyConstant = _native.core.PdfExtractCurrencyConstant
PdfExtractPageClassifyStandard = _native.core.PdfExtractPageClassifyStandard
PdfExtractInvestmentsStandard = _native.core.PdfExtractInvestmentsStandard
PdfExtractAssetsStandard = _native.core.PdfExtractAssetsStandard


class PdfExtractFundStandard:
    """Builds an `ExtractTextPdfBlockOrFailPage` for the fund name.

    Used to subclass `ExtractTextPdfBlockOrFailPage` — now Rust-backed (originally from
    `pdf_extract/common.py`, an alias-only shim module removed during the freeports_core ->
    freeports_engine consolidation; see this module's own docstring), which can't be subclassed
    from Python. Re-examined the same way `standard_txt_blks.py`'s `TextBlock` subclasses were:
    this adds no fields and overrides no behavior, it only hardcodes `name`/`type_block`, and
    nothing does `isinstance(x, PdfExtractFundStandard)` anywhere. Rewritten as a thin factory
    whose `__new__` returns a real `ExtractTextPdfBlockOrFailPage` instead of subclassing it —
    same idiom, same reasoning, same precedent.
    """

    def __new__(cls, selection: PdfLineSelection) -> ExtractTextPdfBlockOrFailPage:
        return ExtractTextPdfBlockOrFailPage(
            selection=selection,
            name="fund",
            type_block=ResultStandardExtraction.FUND_NAME.name,
        )


class PdfExtractCurrencyStandard:
    """Builds an `ExtractTextPdfBlockOrFailPage` for the currency. See `PdfExtractFundStandard`."""

    def __new__(cls, selection: PdfLineSelection) -> ExtractTextPdfBlockOrFailPage:
        return ExtractTextPdfBlockOrFailPage(
            selection=selection,
            name="currency",
            type_block=ResultStandardExtraction.CURRENCY_STATEMENT.name,
        )


class PdfExtractManagmentCompanyStandard:
    """Builds an `ExtractTextPdfBlockOrFailPage` for the management company. See `PdfExtractFundStandard`."""

    def __new__(cls, selection: PdfLineSelection) -> ExtractTextPdfBlockOrFailPage:
        return ExtractTextPdfBlockOrFailPage(
            selection=selection,
            name="managment company",
            type_block=ResultStandardExtraction.MANAGEMENT_COMPANY.name,
        )
