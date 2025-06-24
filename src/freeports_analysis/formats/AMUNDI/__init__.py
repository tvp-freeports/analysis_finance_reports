from enum import Enum
from lxml import etree
from typing import List
from .. import PdfBlock, Text_Block
from ..utils_pdf_filter import one_pdf_blk, lines_with_font, is_present_txt_font


def pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
    pdf_blks = []
    if is_present_txt_font(
        xml_root, "Securities Portfolio as at", "ArialNarrow-BoldItalic"
    ):
        text = None
        line = lines_with_font(
            xml_root, "ArialMT"
        )  # page.xpath('.//line[font[@name="ArialMT"]]')
        for l in line:
            bbox = l.xpath(".//@bbox")[0]
            if float(bbox.split()[-1]) < 27:
                text = l.xpath(".//@text")[0]

        blks = lines_with_font(
            xml_root, "ArialNarrow"
        )  # page.xpath('.//line[font[@name="ArialNarrow"]]')
        pdf_blks = [
            PdfBlock(PDF_BlockType.RELEVANT_BLOCK, {"subfund": text}, blk)
            for blk in blks
        ]
    return pdf_blks


def text_extract(pdf_blocks: List[PdfBlock], targets: List[str]) -> List[Text_Block]:
    pass


def tabularize(text_block: Text_Block) -> dict:
    pass


@one_pdf_blk
class PDF_BlockType(Enum):
    pass


class Text_BlockType(Enum):
    pass
