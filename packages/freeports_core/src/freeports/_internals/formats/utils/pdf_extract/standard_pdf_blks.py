from enum import Enum, auto


class OnePdfBlockType(Enum):
    """Enum representing types of PDF blocks in document processing.

    Attributes
    ----------
    RELEVANT_BLOCK : enum
        PDF block containing relevant information to extract.
    """

    RELEVANT_BLOCK = auto()


class ResultStandardExtraction(Enum):
    FUND_NAME = auto()
    CURRENCY_STATEMENT = auto()
    TABLE_BODY = auto()
    MANAGEMENT_COMPANY = auto()
    INVESTMENTS_MANAGER = auto()
    SFDR_ARTICLE = auto()
    PAGE_CLASS = auto()
