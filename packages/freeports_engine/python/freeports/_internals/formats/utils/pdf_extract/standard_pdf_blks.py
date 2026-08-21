"""Standard PDF block types used across extraction pipelines."""

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
    """Enum representing standard extraction result types for PDF documents.

    Attributes
    ----------
    FUND_NAME : ResultStandardExtraction
        Extracted fund name.
    CURRENCY_STATEMENT : ResultStandardExtraction
        Extracted currency statement.
    TABLE_BODY : ResultStandardExtraction
        Extracted table body content.
    MANAGEMENT_COMPANY : ResultStandardExtraction
        Extracted management company name.
    INVESTMENTS_MANAGER : ResultStandardExtraction
        Extracted investments manager name.
    SFDR_ARTICLE : ResultStandardExtraction
        Extracted SFDR article classification.
    PAGE_CLASS : ResultStandardExtraction
        Extracted page class indicator.
    """

    FUND_NAME = auto()
    CURRENCY_STATEMENT = auto()
    TABLE_BODY = auto()
    MANAGEMENT_COMPANY = auto()
    INVESTMENTS_MANAGER = auto()
    SFDR_ARTICLE = auto()
    PAGE_CLASS = auto()
