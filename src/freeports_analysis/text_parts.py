from enum import Enum, auto
from typing import Optional
from pdf_part import PDF_part


class Text_part:
    type_part: Enum
    metadata: Optional[dict]
    pdf_part: PDF_part
    content: str

    def __init__(self, pdf_part: PDF_part, type_part: Enum, metadata: Optional[dict]):
        self.type_part = type_part
        self.metadata = metadata
        self.pdf_part = pdf_part
        self.content = PDF_part.content


class EURIZON(Enum):
    ROW_TABLE_COMPLETE = auto()
    ROW_TABLE_INCOMPLETE = auto()


class _DEFAULT(Enum):
    DEFAULT_PART = auto()
