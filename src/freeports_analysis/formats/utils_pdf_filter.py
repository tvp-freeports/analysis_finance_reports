from lxml import etree
from typing import List
from enum import Enum, auto
from freeports_analysis.formats import PdfBlock
from .utils_commons import overwrite_if_implemented


class one_PdfBlockType(Enum):
    RELEVANT_BLOCK = auto()


def one_pdf_blk(_):
    return one_PdfBlockType


def standard_header_font_filter(header_txt, header_font, body_font):
    def decorator(f):
        @overwrite_if_implemented(f)
        def page_metadata(_: etree.Element) -> dict:
            return {}

        @one_pdf_blk
        class PdfBlockType(Enum):
            pass

        @filter_page_if(lambda x: is_present_txt_font(x, header_txt, header_font))
        def pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
            metadata = page_metadata(xml_root)
            rows = get_lines_with_font(xml_root, body_font)
            parts = [
                PdfBlock(PdfBlockType.RELEVANT_BLOCK, metadata, blk) for blk in rows
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


def is_positioned(blk: etree) -> bool: ...


def is_present_positioned(blk: etree) -> bool: ...


def filter_page_if(condition):
    def wrapper(pdf_filter):
        def conditionated_pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
            parts = []
            if condition(xml_root):
                parts = pdf_filter(xml_root)
            return parts

        return conditionated_pdf_filter

    return wrapper


def get_lines_with_font(blk: etree, font: str) -> List[etree]:
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
