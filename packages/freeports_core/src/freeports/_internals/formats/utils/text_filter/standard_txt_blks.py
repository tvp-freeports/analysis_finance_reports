"""Standard text block types for filtered document processing results."""

from enum import Enum, auto
from typing import Set

from freeports.core import TextBlock, PdfBlock
from freeports._internals.formats.utils.text_filter import match


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


class StandardManagmentCompanyTextBlock(TextBlock):
    """Text block representing a management company with its managed funds.

    Parameters
    ----------
    pdf_blk : PdfBlock
        The PDF block this text block originates from.
    funds : set of MatchFund
        Set of funds managed by this company.
    """

    def __init__(self, pdf_blk: PdfBlock, funds: Set[match.MatchFund]) -> None:
        super().__init__(
            ResultStandardFiltering.MANAGEMENT_COMPANY,
            {"managed_funds": set((f.name for f in funds))},
            pdf_blk,
        )

    @classmethod
    def from_content(
        cls, name: str, funds: Set[match.MatchFund]
    ) -> "StandardManagmentCompanyTextBlock":
        """Create a management company block from content rather than a PDF block.

        Parameters
        ----------
        name : str
            The company name.
        funds : set of MatchFund
            Set of funds managed by this company.

        Returns
        -------
        StandardManagmentCompanyTextBlock
            A new instance built from the given content.
        """
        return super().from_content(
            ResultStandardFiltering.MANAGEMENT_COMPANY,
            {"managed_funds": set((f.name for f in funds))},
            name,
        )

    from_name = from_content


class StandardInvestmentsMangerTextBlock(TextBlock):
    """Text block representing an investments manager with its managed funds.

    Parameters
    ----------
    pdf_blk : PdfBlock
        The PDF block this text block originates from.
    funds : set of MatchFund
        Set of funds managed by this investments manager.
    """

    def __init__(self, pdf_blk: PdfBlock, funds: Set[match.MatchFund]) -> None:
        super().__init__(
            ResultStandardFiltering.INVESTMENTS_MANAGER,
            {"managed_funds": set((f.name for f in funds))},
            pdf_blk,
        )

    @classmethod
    def from_content(
        cls, name: str, funds: Set[match.MatchFund]
    ) -> "StandardInvestmentsMangerTextBlock":
        """Create an investments manager block from content rather than a PDF block.

        Parameters
        ----------
        name : str
            The investments manager name.
        funds : set of MatchFund
            Set of funds managed by this investments manager.

        Returns
        -------
        StandardInvestmentsMangerTextBlock
            A new instance built from the given content.
        """
        return super().from_content(
            ResultStandardFiltering.INVESTMENTS_MANAGER,
            {"managed_funds": set((f.name for f in funds))},
            name,
        )

    from_name = from_content


class StandardFundTextBlock(TextBlock):
    """Text block representing a fund extracted from a document.

    Parameters
    ----------
    blk : PdfBlock
        The PDF block this text block originates from.
    """

    def __init__(self, blk: PdfBlock) -> None:
        super().__init__(ResultStandardFiltering.FUND, {}, blk)

    @classmethod
    def from_matched_fund(cls, fund: match.MatchFund) -> "StandardFundTextBlock":
        """Create a fund block from a matched fund object.

        Parameters
        ----------
        fund : MatchFund
            The matched fund to build the block from.

        Returns
        -------
        StandardFundTextBlock
            A new instance using the fund's name as content.
        """
        return super().from_content(ResultStandardFiltering.FUND, {}, fund.name)

    @classmethod
    def from_content(cls, fund: str) -> "StandardFundTextBlock":
        """Create a fund block from a fund name string.

        Parameters
        ----------
        fund : str
            The fund name.

        Returns
        -------
        StandardFundTextBlock
            A new instance built from the given fund name.
        """
        return super().from_content(ResultStandardFiltering.FUND, {}, fund)

    from_name = from_content
