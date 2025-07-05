"""Utilities for writing `pdf_filter` functions"""

from typing import List, Optional, Tuple, TypeAlias, Callable
from enum import Enum, auto
from lxml import etree
from freeports_analysis.formats import PdfBlock
from .xml.font import get_lines_with_font, is_present_txt_font, get_lines_with_txt_font
from .select_position import select_inside, get_table_positions
from .pdf_parts.position import YRange
from .pdf_parts import ExtractedPdfLine
from .select_font import deselect_txt_font
from .xml.position import get_bounds

# , get_bounds
from ..utils_commons import overwrite_if_implemented
from .. import ExpectedPdfBlockNotFound


PdfBlockType: TypeAlias = Enum


class one_PdfBlockType(Enum):
    """Enum representing one type of pdf blocks in document processing.

    Attributes
    ----------
    RELEVANT_BLOCK : enum
        Pdf block with relevant information to extract.
    """

    RELEVANT_BLOCK = auto()


def one_pdf_blk(_: PdfBlockType) -> one_PdfBlockType:
    """Decorator for overwriting classes definition with a unique
    pdf block type. It is helpful when there is only one type of pdf block

    Returns
    -------
    one_PdfBlockType
        The text block type enum.
    """
    return one_PdfBlockType


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

    def wrapper(pdf_filter: Callable[[etree.Element, int], List[PdfBlock]]) -> Callable:
        def conditionated_pdf_filter(
            xml_root: etree.Element, page_number: int
        ) -> List[PdfBlock]:
            parts = []
            if condition(xml_root):
                parts = pdf_filter(xml_root, page_number)
            return parts

        return conditionated_pdf_filter

    return wrapper


def standard_extraction_subfund(
    subfund_height: Tuple[float, float] | float,
    subfund_font: str,
):
    def decorator(old_page_metadata):
        def new_page_metadata(
            xml_root: etree.Element, page_number: int
        ) -> List[PdfBlock]:
            lines_with_font = get_lines_with_font(xml_root, subfund_font)
            lines = [ExtractedPdfLine(blk) for blk in lines_with_font]
            y_range = YRange(*subfund_height)
            top_lines = select_inside(lines, y_range)
            subfund = None
            if len(top_lines) > 0:
                subfund = top_lines[0].xml_blk.xpath(".//@text")[0]
            if subfund is None:
                raise ExpectedPdfBlockNotFound(
                    "subfound block on top of page not found"
                )
            metadata = old_page_metadata(xml_root, page_number)
            metadata["subfund"] = subfund
            return metadata

        return new_page_metadata

    return decorator


def standard_pdf_filtering(
    header_txt: str,
    header_font: str,
    subfund_height: Tuple[float, float] | float,
    subfund_font: str,
    body_font: str,
    y_range: Optional[
        Tuple[Optional[float | Tuple[str, str]], Optional[float | Tuple[str, str]]]
    ] = None,
    deselection_list: Optional[Tuple[str, str]] = None,
):
    """Decorator factory for creating PDF filters
    that process pages with specific header and body fonts and extract subfund information

    Creates a filter that:
    1. Only processes pages containing the specified header text in the specified header font
    2. Extracts all lines with the specified body font as relevant blocks
    3. Extracts subfund in a range or over a certain height using his font
    4. Allows customization of page metadata and block types through decorated functions

    Parameters
    ----------
    header_txt : str
        Text that must be present in header to process the page
    header_font : str
        Font name that header text must use
    subfund_height : Tuple[float,float] | float
        Range in which the subfund has to be extracted or height over which extract
    sufund_font: str
        Font name that the subfund uses
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
        @standard_extraction_subfund(subfund_height, subfund_font)
        @overwrite_if_implemented(f)
        def page_metadata(_: etree.Element, page_number: int) -> dict:
            return {"page": page_number}

        @one_pdf_blk
        class PdfBlockType(Enum):
            pass

        @filter_page_if(lambda x: is_present_txt_font(x, header_txt, header_font))
        def pdf_filter(xml_root: etree.Element, page_number: int) -> List[PdfBlock]:
            metadata = page_metadata(xml_root, page_number)
            rows = get_lines_with_font(xml_root, body_font)
            lines = [ExtractedPdfLine(r) for r in rows]
            y_range_numeric_top = None
            y_range_numeric_btm = None

            if deselection_list is not None:
                lines = deselect_txt_font(
                    deselection_list=deselection_list, lines=lines
                )

            if y_range is not None:
                if type(y_range[0]) is tuple:
                    txt, font = y_range[0]
                    top_limit_blk = get_lines_with_txt_font(xml_root, txt, font)
                    if top_limit_blk is not None:
                        y_range_numeric_top = get_bounds(top_limit_blk)[1][1]
                else:
                    y_range_numeric_top = y_range[0]
                if type(y_range[1]) is tuple:
                    txt, font = y_range[1]
                    btm_limit_blk = get_lines_with_txt_font(xml_root, txt, font)
                    if btm_limit_blk is not None:
                        y_range_numeric_btm = get_bounds(btm_limit_blk)[1][0]
                else:
                    y_range_numeric_btm = y_range[1]
            table_rows = select_inside(
                lines, YRange(y_range_numeric_top, y_range_numeric_btm)
            )
            table_positions = get_table_positions(table_rows)
            return [
                PdfBlock(
                    PdfBlockType.RELEVANT_BLOCK,
                    {**metadata, "table-col": table_positions[i]},
                    table_row.xml_blk,
                )
                for i, table_row in enumerate(table_rows)
            ]

        return pdf_filter

    return decorator
