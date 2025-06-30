"""Utilities for writing `pdf_filter` functions"""

from typing import List, Optional, Tuple, TypeAlias, Callable
from enum import Enum, auto
from lxml import etree
from freeports_analysis.formats import PdfBlock
from .utils_commons import overwrite_if_implemented

PdfBlockType: TypeAlias = Enum


class OnePdfBlockType(Enum):
    """Enum representing one type of pdf blocks in document processing.

    Attributes
    ----------
    RELEVANT_BLOCK : enum
        Pdf block with relevant information to extract.
    """

    RELEVANT_BLOCK = auto()


def one_pdf_blk(_: PdfBlockType) -> OnePdfBlockType:
    """Decorator for overwriting classes definition with a unique
    pdf block type. It is helpful when there is only one type of pdf block

    Returns
    -------
    OnePdfBlockType
        The text block type enum.
    """
    return OnePdfBlockType


def standard_header_font_filter(header_txt: str, header_font: str, body_font: str):
    """Decorator factory for creating PDF filters
    that process pages with specific header and body fonts.

    Creates a filter that:
    1. Only processes pages containing the specified header text in the specified header font
    2. Extracts all lines with the specified body font as relevant blocks
    3. Allows customization of page metadata and block types through decorated functions

    Parameters
    ----------
    header_txt : str
        Text that must be present in header to process the page
    header_font : str
        Font name that header text must use
    body_font : str
        Font name for lines to extract as relevant blocks

    Returns
    -------
    function
        Decorator that can be applied to PDF filter functions

    Notes
    -----
    The decorated function can override:
    - page_metadata(): returns additional metadata dictionary for each page
    """

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


def is_present_txt_font(blk: etree.Element, txt: str, font: str) -> bool:
    """Return if a certain pdf block with a specific text and font is present in the tree

    Parameters
    ----------
    blk : etree.Element
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


def is_positioned(
    blk: etree.Element,
    x_range: Optional[Tuple[float, float]] = None,
    y_range: Optional[Tuple[float, float]] = None,
) -> bool:
    """Check if a block is positioned within specified x and/or y ranges.
    Parameters
    ----------
    blk : etree.Element
        XML element representing a PDF block
    x_range : Optional[Tuple[float, float]], optional
        Minimum and maximum x-coordinate values (left, right)
    y_range : Optional[Tuple[float, float]], optional
        Minimum and maximum y-coordinate values (bottom, top)
    Returns
    -------
    bool
        True if block's coordinates are within specified ranges, False otherwise
    """
    bbox = blk.xpath(".//@bbox")
    if not bbox:
        return False

    coords = [float(c) for c in bbox[0].split()]  # x0, y0, x1, y1

    if x_range is not None:
        x_center = (coords[0] + coords[2]) / 2
        if not x_range[0] <= x_center <= x_range[1]:
            return False

    if y_range is not None:
        y_center = (coords[1] + coords[3]) / 2
        if not y_range[0] <= y_center <= y_range[1]:
            return False

    return True


def is_present_positioned(
    blk: etree.Element,
    text: str,
    x_range: Optional[Tuple[float, float]] = None,
    y_range: Optional[Tuple[float, float]] = None,
) -> bool:
    """Check if text is present in a block within specified coordinate ranges.
    Parameters
    ----------
    blk : etree.Element
        XML element representing a PDF block
    text : str
        Text to search for
    x_range : Optional[Tuple[float, float]], optional
        Horizontal range (left, right)
    y_range : Optional[Tuple[float, float]], optional
        Vertical range (bottom, top)

    Returns
    -------
    bool
        True if text is found in block within ranges, False otherwise
    """
    lines = blk.xpath(f".//line[contains(@text,'{text}')]")
    for line in lines:
        if is_positioned(line, x_range, y_range):
            return True
    return False


def is_contained(
    blk: etree.Element,
    x_range: Optional[Tuple[float, float]] = None,
    y_range: Optional[Tuple[float, float]] = None,
) -> bool:
    """Check if a block's bounding box is fully contained within specified ranges.

    Parameters
    ----------
    blk : etree.Element
        XML element representing a PDF block
    x_range : Optional[Tuple[float, float]], optional
        Horizontal range (min_x, max_x)
    y_range : Optional[Tuple[float, float]], optional
        Vertical range (min_y, max_y)

    Returns
    -------
    bool
        True if entire bbox is within ranges, False otherwise
    """
    bbox = blk.xpath(".//@bbox")
    if not bbox:
        return False

    x0, y0, x1, y1 = [float(c) for c in bbox[0].split()]  # x0, y0, x1, y1

    if x_range is not None and (x0 < x_range[0] or x1 > x_range[1]):
        return False

    if y_range is not None and (y0 < y_range[0] or y1 > y_range[1]):
        return False

    return True


def is_present_contained(
    blk: etree.Element,
    text: str,
    x_range: Optional[Tuple[float, float]] = None,
    y_range: Optional[Tuple[float, float]] = None,
) -> bool:
    """Check if text is present in a block fully contained within specified ranges.

    Parameters
    ----------
    blk : etree.Element
        XML element representing a PDF block
    text : str
        Text to search for
    x_range : Optional[Tuple[float, float]], optional
        Horizontal range (min_x, max_x)
    y_range : Optional[Tuple[float, float]], optional
        Vertical range (min_y, max_y)

    Returns
    -------
    bool
        True if text is found in block fully contained within ranges, False otherwise
    """
    lines = blk.xpath(f".//line[contains(@text,'{text}')]")
    for line in lines:
        if is_contained(line, x_range, y_range):
            return True
    return False


def filter_page_if(condition: Callable[[etree.Element], bool]) -> Callable:
    """Decorator factory for conditionally applying a PDF filter based on a predicate.

    Creates a decorator that will only execute the wrapped PDF filter function if the
    specified condition evaluates to True for the given XML root element.

    Parameters
    ----------
    condition : Callable[[etree.Element], bool]
        A function that takes an XML element and returns True if the filter should be applied

    Returns
    -------
    Callable
        A decorator that can be applied to PDF filter functions

    Notes
    -----
    The decorated PDF filter function must accept an XML root element and return a list of PdfBlocks
    """

    def wrapper(pdf_filter: Callable[[etree.Element], List[PdfBlock]]) -> Callable:
        def conditionated_pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
            parts = []
            if condition(xml_root):
                parts = pdf_filter(xml_root)
            return parts

        return conditionated_pdf_filter

    return wrapper


def get_lines_with_font(blk: etree.Element, font: str) -> List[etree.Element]:
    """Return all the line with a certain font in a tree

    Parameters
    ----------
    blk : etree.Element
        tree from which extract lines
    font : str
        font to extract

    Returns
    -------
    List[etree.Element]
        list of relevant blocks
    """
    return blk.xpath(".//line[font[@name='" + font + "']]")
