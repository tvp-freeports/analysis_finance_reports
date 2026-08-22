"""Standard text block types for filtered document processing results.

``StandardManagmentCompanyTextBlock``/``StandardInvestmentsMangerTextBlock``/
``StandardFundTextBlock`` used to subclass ``TextBlock`` — the reason `TextBlock` itself couldn't
be ported to Rust earlier in this migration. Re-examined on explicit user instruction: none of
the three add fields or override behavior, they exist purely as convenience constructors
hardcoding a `type_block` string and a metadata shape, and nothing anywhere does
`isinstance(x, StandardFundTextBlock)` (checked — only `type_block`, a plain string since the
Fase 2 `Enum -> str` migration, is ever used to tell these apart at runtime). So each is now a
thin class whose ``__new__`` returns a genuine ``TextBlock`` instance directly instead of
subclassing it — Python allows ``__new__`` to return an object of a different type than ``cls``,
in which case ``__init__`` is not called, which is exactly the "constructor that is actually a
factory function" behavior these three want. This keeps both call conventions working
unchanged (``StandardFundTextBlock(blk)`` and ``StandardFundTextBlock.from_content(fund)``), and
the result is a real ``TextBlock`` — satisfies the one place that actually does
`isinstance(obj, TextBlock)` at runtime (`core/serialization.py`'s fixture round-trip).
"""

from enum import Enum, auto
from typing import Set

from freeports.core import TextBlock, PdfBlock
from freeports._internals.core import match


class OneTextBlockType(Enum):
    """Enum representing types of individual text blocks in document processing.

    Attributes
    ----------
    RELEVANT_BLOCK : OneTextBlockType
        Text block containing relevant information to extract.
    """

    RELEVANT_BLOCK = auto()


class ResultStandardFiltering(Enum):
    """Enum representing two type of text blocks in document processing.

    Attributes
    ----------
    BOND_TARGET : enum
        Text block containing target `Bond` row.
    EQUITY_TARGET : enum
        Text block containing target `Equity` row.
    """

    BOND_TARGET = auto()
    EQUITY_TARGET = auto()
    FUND = auto()
    MANAGEMENT_COMPANY = auto()
    INVESTMENTS_MANAGER = auto()
    SFDR_ARTICLE = auto()
    PAGE_CLASS = auto()


class StandardManagmentCompanyTextBlock:
    """Builds a `TextBlock` representing a management company with its managed funds.

    Parameters
    ----------
    pdf_blk : PdfBlock
        The PDF block this text block originates from.
    funds : set of MatchFund
        Set of funds managed by this company.
    """

    def __new__(cls, pdf_blk: PdfBlock, funds: Set[match.MatchFund]) -> TextBlock:
        return TextBlock(
            ResultStandardFiltering.MANAGEMENT_COMPANY.name,
            {"managed_funds": set((f.name for f in funds))},
            pdf_blk,
        )

    @staticmethod
    def from_content(name: str, funds: Set[match.MatchFund]) -> TextBlock:
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
        return TextBlock.from_content(
            ResultStandardFiltering.MANAGEMENT_COMPANY.name,
            {"managed_funds": set((f.name for f in funds))},
            name,
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

    def __new__(cls, pdf_blk: PdfBlock, funds: Set[match.MatchFund]) -> TextBlock:
        return TextBlock(
            ResultStandardFiltering.INVESTMENTS_MANAGER.name,
            {"managed_funds": set((f.name for f in funds))},
            pdf_blk,
        )

    @staticmethod
    def from_content(name: str, funds: Set[match.MatchFund]) -> TextBlock:
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
        return TextBlock.from_content(
            ResultStandardFiltering.INVESTMENTS_MANAGER.name,
            {"managed_funds": set((f.name for f in funds))},
            name,
        )

    from_name = from_content


class StandardFundTextBlock:
    """Builds a `TextBlock` representing a fund extracted from a document.

    Parameters
    ----------
    blk : PdfBlock
        The PDF block this text block originates from.
    """

    def __new__(cls, blk: PdfBlock) -> TextBlock:
        return TextBlock(ResultStandardFiltering.FUND.name, {}, blk)

    @staticmethod
    def from_matched_fund(fund: match.MatchFund) -> TextBlock:
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
        return TextBlock.from_content(ResultStandardFiltering.FUND.name, {}, fund.name)

    @staticmethod
    def from_content(fund: str) -> TextBlock:
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
        return TextBlock.from_content(ResultStandardFiltering.FUND.name, {}, fund)

    from_name = from_content
