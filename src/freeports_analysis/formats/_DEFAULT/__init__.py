from enum import Enum
from lxml import etree
from typing import List
from .. import PdfBlock, Text_Block


def pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
    pass


def text_extract(pdf_blocks: List[PdfBlock], targets: List[str]) -> List[Text_Block]:
    pass


def tabularize(text_block: Text_Block) -> dict:
    pass


class PDF_BlockType(Enum):
    pass


class Text_BlockType(Enum):
    pass
