"""AMUNDI format submodule"""

from enum import Enum
from typing import List
from lxml import etree
from .. import PdfBlock, TextBlock
from ..utils_pdf_filter import (
    one_pdf_blk,
    get_lines_with_font,
    standard_header_font_filter,
)
from ..utils_text_extract import standard_text_extraction, one_txt_blk
from ..utils_tabularize import standard_tabularizer, perc_to_float


@standard_header_font_filter(
    "Securities Portfolio as at", "ArialNarrow-BoldItalic", "ArialNarrow"
)
def pdf_filter(xml_root: etree.Element) -> dict:
    line = get_lines_with_font(xml_root, "ArialMT")
    text = None
    for ln in line:
        bbox = ln.xpath(".//@bbox")[0]
        if float(bbox.split()[-1]) < 27:
            text = ln.xpath(".//@text")[0]
    return {"subfund": text}


@standard_text_extraction({"n assets": +1, "fair value": -1, "% net asset": -2})
def text_extract(pdf_blocks: List[PdfBlock], i: int) -> dict:
    pass


@standard_tabularizer(
    {
        "match": str,
        "subfund": str,
        "n assets": int,
        "fair value": int,
        "% net asset": perc_to_float,
    }
)
def tabularize(TextBlock: TextBlock) -> dict:
    pass


@one_pdf_blk
class PdfBlockType(Enum):
    pass


@one_txt_blk
class TextBlockType(Enum):
    pass
