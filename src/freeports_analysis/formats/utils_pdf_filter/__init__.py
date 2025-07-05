"""Utilities for writing `pdf_filter` functions"""

from typing import List, Optional, Tuple, TypeAlias, Callable
from enum import Enum, auto
from lxml import etree
from freeports_analysis.formats import PdfBlock
from .xml.font import get_lines_with_font, is_present_txt_font, get_lines_with_txt_font
from .font import Font, TextSize
from .position import select_inside, get_table_positions, Area, XRange, YRange
from .xml.position import get_bounds

# , get_bounds
from ..utils_commons import overwrite_if_implemented
from .. import ExpectedPdfBlockNotFound
from page_layout import print_blocks
import pymupdf as pypdf
import copy


def get_page(file_name: str, page: int, offset: int = 0):
    pdf_file = pypdf.Document(file_name)
    parser = etree.XMLParser(recover=True)
    page_doc = pdf_file[page + offset]
    xml_str = page_doc.get_text("xml")
    xml_tree = etree.fromstring(xml_str.encode(), parser=parser)
    return xml_tree


def print_blocks(xml_tree: etree.Element, max_deeph: int = 0) -> None:
    etree_to_print = copy.deepcopy(xml_tree)

    def _remove_tree_to_depth(elem: etree.Element, depth: int = 0, max_depth: int = 0):
        for e in list(elem):
            if depth >= max_depth:
                elem.remove(e)
            else:
                _remove_tree_to_depth(e, depth + 1, max_depth)

    _remove_tree_to_depth(etree_to_print, depth=0, max_depth=max_deeph)
    print(etree.tostring(etree_to_print, pretty_print=True).decode(), end="")
    del etree_to_print



PdfBlockType: TypeAlias = Enum


class ExtractedPdfLine:
    def __init__(self, blk: etree.Element):
        self._blk = blk
        bounds = get_bounds(blk)
        self._geometry = Area(
            XRange(bounds[0][0], bounds[0][1]), YRange(bounds[1][0], bounds[1][1])
        )
        self._font = Font(blk.xpath(".//font/@name")[0])
        self._txt_size = TextSize(blk.xpath(".//font/@size")[0])

    @property
    def geometry(self):
        return self._geometry

    @property
    def c(self):
        return self._geometry.c

    @property
    def corners(self):
        return self._geometry.corners

    @property
    def font(self):
        return self._font

    @property
    def text_size(self):
        return self._txt_size

    def __str__(self):
        string = f"Line PDF - Font '{self.font}' [{self.text_size}]\n"
        ((tl, tr), (bl, br)) = self.corners
        string += f"\t{tl}\t{tr}\n"
        string += f"\t\t{self.c}\n"
        string += f"\t{bl}\t{br}\n"
        return string


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
            top_lines = select_inside(lines_with_font, None, subfund_height)
            subfund = None
            if len(top_lines) > 0:
                subfund = top_lines[0].xpath(".//@text")[0]
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
            y_range_numeric_top = None
            y_range_numeric_btm = None

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
                rows, None, (y_range_numeric_top, y_range_numeric_btm)
            )
            table_positions = get_table_positions(table_rows)
            parts = []
            i = 0
            while i < len(table_rows) - 1:
                if table_positions[i][1] == table_positions[i + 1][1]:
                    parts.append(
                        PdfBlock(
                            PdfBlockType.RELEVANT_BLOCK,
                            metadata,
                            [table_rows[i], table_rows[i + 1]],
                        )
                    )
                    i += 2  # Skip the next row since it's already grouped
                else:
                    parts.append(
                        PdfBlock(PdfBlockType.RELEVANT_BLOCK, metadata, table_rows[i])
                    )
                    i += 1

            # If there's an unprocessed row left at the end
            if i < len(table_rows):
                parts.append(
                    PdfBlock(PdfBlockType.RELEVANT_BLOCK, metadata, table_rows[i])
                )
            return parts

        return pdf_filter

    return decorator
