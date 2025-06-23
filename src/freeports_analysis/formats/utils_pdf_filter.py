from lxml import etree
from typing import List
from enum import Enum, auto
from . import PDF_Block


def one_pdf_blk(_):
    class PDF_BlockType(Enum):
        RELEVANT_BLOCK = auto()

    return PDF_BlockType


def standard_header_font_filter(header_txt, header_font, body_font):
    def decorator(_):
        def pdf_filter(xml_root: etree.Element) -> List[PDF_Block]:
            @one_pdf_blk
            class PDF_BlockType(Enum):
                pass

            parts = []
            if is_present_txt_font(xml_root, header_txt, header_font):
                rows = lines_with_font(xml_root, body_font)
                parts = [
                    PDF_Block(PDF_BlockType.RELEVANT_BLOCK, {}, blk) for blk in rows
                ]
            return parts

        return pdf_filter

    return decorator


def is_present_txt_font(blk: etree, txt: str, font: str) -> bool:
    """Return if a certain pdf block with a specific text and font is present in the tree

    Parameters
    ----------
    blk : etree
        tree to search in
    txt : str
        text to search
    font : str
        font to search

    Returns
    -------
    bool
        boolean describing if the block is present or not
    """
    relevant_part = blk.xpath(
        ".//line[contains(@text,'" + txt + "') and font[@name='" + font + "']]"
    )
    return len(relevant_part) > 0


def lines_with_font(blk: etree, font: str) -> List[etree]:
    """Return all the line with a certain font in a tree

    Parameters
    ----------
    blk : etree
        tree from which extract lines
    font : str
        font to extract

    Returns
    -------
    List[etree]
        list of relevant blocks
    """
    return blk.xpath(".//line[font[@name='" + font + "']]")
