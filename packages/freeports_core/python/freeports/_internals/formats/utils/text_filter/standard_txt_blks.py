"""Standard text block types for filtered document processing results.

``OneTextBlockType``, ``ResultStandardFiltering``, and the construction logic behind
``StandardManagmentCompanyTextBlock``/``StandardInvestmentsMangerTextBlock``/
``StandardFundTextBlock`` are now implemented in Rust — see
``packages/freeports_core/src/formats_utils/text_filter/standard_txt_blks.rs`` and
``agent-memory/fase5-porting-implementation-plan.md`` ("Module 1"). The two enums delegate
directly to their native counterparts (same pattern as ``commons/consts.py``'s
``FinancialInstrument``/``SfdrArticle``/``Currency``).

The three classes stay real Python classes, unlike the enums, for a concrete PyO3 limitation, not
a style choice: their whole reason to exist is a ``__new__`` that returns a *different* type
(``TextBlock``) than ``cls``, which PyO3 ``#[new]`` cannot express, and a real
``#[pyclass(extends = TextBlock)]`` subclass would resurrect the now-dead ``subtype_tag``
machinery ``core/classes.rs`` deliberately made inert. So each stays a thin dispatch shim whose
``__new__``/``from_content``/`from_name`/`from_matched_fund`` bodies do nothing but call the
native constructor functions and return their result directly — all the real logic (the
``TextBlock`` shape, the ``managed_funds`` metadata) now lives in Rust.

``funds`` arguments here are sets of ``match.MatchFund`` (the Python bridge class, see
``_internals/core/match.py``) — each element's ``._core`` is the real
``freeports._native.core.MatchFund`` the native functions below actually expect.
"""

from typing import Set

from freeports import _native
from freeports._internals.core import match

OneTextBlockType = _native.OneTextBlockType
ResultStandardFiltering = _native.ResultStandardFiltering


class StandardManagmentCompanyTextBlock:
    """Builds a `TextBlock` representing a management company with its managed funds.

    Parameters
    ----------
    pdf_blk : PdfBlock
        The PDF block this text block originates from.
    funds : set of MatchFund
        Set of funds managed by this company.
    """

    def __new__(cls, pdf_blk, funds: Set[match.MatchFund]):
        return _native.core.standard_management_company_text_block(
            pdf_blk, [f._core for f in funds]
        )

    @staticmethod
    def from_content(name: str, funds: Set[match.MatchFund]):
        """Create a management company block from content rather than a PDF block.

        Parameters
        ----------
        name : str
            The company name.
        funds : set of MatchFund
            Set of funds managed by this company.

        Returns
        -------
        TextBlock
            A new instance built from the given content.
        """
        return _native.core.standard_management_company_text_block_from_content(
            name, [f._core for f in funds]
        )

    from_name = from_content


class StandardInvestmentsMangerTextBlock:
    """Builds a `TextBlock` representing an investments manager with its managed funds.

    Parameters
    ----------
    pdf_blk : PdfBlock
        The PDF block this text block originates from.
    funds : set of MatchFund
        Set of funds managed by this investments manager.
    """

    def __new__(cls, pdf_blk, funds: Set[match.MatchFund]):
        return _native.core.standard_investments_manager_text_block(
            pdf_blk, [f._core for f in funds]
        )

    @staticmethod
    def from_content(name: str, funds: Set[match.MatchFund]):
        """Create an investments manager block from content rather than a PDF block.

        Parameters
        ----------
        name : str
            The investments manager name.
        funds : set of MatchFund
            Set of funds managed by this investments manager.

        Returns
        -------
        TextBlock
            A new instance built from the given content.
        """
        return _native.core.standard_investments_manager_text_block_from_content(
            name, [f._core for f in funds]
        )

    from_name = from_content


class StandardFundTextBlock:
    """Builds a `TextBlock` representing a fund extracted from a document.

    Parameters
    ----------
    blk : PdfBlock
        The PDF block this text block originates from.
    """

    def __new__(cls, blk):
        return _native.core.standard_fund_text_block(blk)

    @staticmethod
    def from_matched_fund(fund: match.MatchFund):
        """Create a fund block from a matched fund object.

        Parameters
        ----------
        fund : MatchFund
            The matched fund to build the block from.

        Returns
        -------
        TextBlock
            A new instance using the fund's name as content.
        """
        return _native.core.standard_fund_text_block_from_content(fund.name)

    @staticmethod
    def from_content(fund: str):
        """Create a fund block from a fund name string.

        Parameters
        ----------
        fund : str
            The fund name.

        Returns
        -------
        TextBlock
            A new instance built from the given fund name.
        """
        return _native.core.standard_fund_text_block_from_content(fund)

    from_name = from_content
