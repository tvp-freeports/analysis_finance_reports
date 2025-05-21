from enum import Enum, auto
from typing import Optional


class PDF_part:
    type_part: Enum
    metadata: Optional[dict]
    content: Optional[str]

    def __init__(self, type_part: Enum, metadata: Optional[dict]):
        self.type_part = type_part
        self.metadata = metadata
        self.content = ""


class EURIZON(Enum):
    TITLE = auto()
    TABLE = auto()


class _DEFAULT(Enum):
    DEFAULT_PART = auto()
