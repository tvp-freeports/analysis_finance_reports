from enum import Enum, auto

from freeports.core import TextBlock


class OneTextBlockType(Enum):
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
    def __init__(self, pdf_blk: PdfBlock, funds: Set[match.MatchFund]):
        super().__init__(
            ResultStandardFiltering.MANAGEMENT_COMPANY,
            {"managed_funds": set((f.name for f in funds))},
            pdf_blk,
        )

    @classmethod
    def from_content(cls, name, funds: Set[match.MatchFund]):
        return super().from_content(
            ResultStandardFiltering.MANAGEMENT_COMPANY,
            {"managed_funds": set((f.name for f in funds))},
            name,
        )

    from_name = from_content


class StandardInvestmentsMangerTextBlock(TextBlock):
    def __init__(self, pdf_blk: PdfBlock, funds: Set[match.MatchFund]):
        super().__init__(
            ResultStandardFiltering.INVESTMENTS_MANAGER,
            {"managed_funds": set((f.name for f in funds))},
            pdf_blk,
        )

    @classmethod
    def from_content(cls, name, funds: Set[match.MatchFund]):
        return super().from_content(
            ResultStandardFiltering.INVESTMENTS_MANAGER,
            {"managed_funds": set((f.name for f in funds))},
            name,
        )

    from_name = from_content


class StandardFundTextBlock(TextBlock):
    def __init__(self, blk):
        super().__init__(ResultStandardFiltering.FUND, {}, blk)

    @classmethod
    def from_matched_fund(cls, fund: match.MatchFund):
        return super().from_content(ResultStandardFiltering.FUND, {}, fund.name)

    @classmethod
    def from_content(cls, fund: str):
        return super().from_content(ResultStandardFiltering.FUND, {}, fund)

    from_name = from_content
