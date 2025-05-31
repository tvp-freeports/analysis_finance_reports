from enum import Enum
from lxml import etree
from typing import List
from .. import PDF_Block, Text_Block


def pdf_filter(xml_root: etree.Element) -> List[PDF_Block]:
    pass


def text_extract(pdf_blocks: List[PDF_Block], targets: List[str]) -> List[Text_Block]:
    pass


def tabularize(text_block: Text_Block) -> dict:
    pass


class PDF_BlockType(Enum):
    pass


class Text_BlockType(Enum):
    pass
