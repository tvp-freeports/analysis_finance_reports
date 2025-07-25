"""Utilities for writing `pdf_filter` functions.

This module provides decorators and utilities for filtering and processing PDF content
based on XML elements, fonts, and positional data.
"""

from typing import List, Optional, TypeAlias, Callable
from enum import Enum, auto
import logging as log
from lxml import etree
from freeports_analysis.formats import PdfBlock, ExpectedPdfBlockNotFound, TextBlock
from freeports_analysis.i18n import _
from .xml.font import get_lines_with_font, get_lines_with_txt_font
from .select_position import get_table_positions, TablePosAlgorithm
from .pdf_parts import ExtractedPdfLine, PdfLineSet
from .. import overwrite_if_implemented
from freeports_analysis.consts import Currency

logger = log.getLogger(__name__)


UpdateMetadataFunc: TypeAlias = Callable[[etree.Element], dict]
FilterCondition: TypeAlias = Callable[[etree.Element], bool]
PdfFilterFunc: TypeAlias = Callable[[etree.Element], List[TextBlock]]


class OnePdfBlockType(Enum):
    """Enum representing types of PDF blocks in document processing.

    Attributes
    ----------
    RELEVANT_BLOCK : enum
        PDF block containing relevant information to extract.
    """

    RELEVANT_BLOCK = auto()


def filter_page_if(
    condition: FilterCondition,
) -> Callable[[PdfFilterFunc], PdfFilterFunc]:
    """Decorator factory for conditionally applying a PDF filter based on a predicate.

    Creates a decorator that will only execute the wrapped PDF filter function if the
    specified condition evaluates to True for the given XML root element.

    Parameters
    ----------
    condition : Callable[[etree.Element], bool]
        A predicate function that determines whether the filter should be applied.

    Returns
    -------
    Callable[[PdfFilterFunc], PdfFilterFunc]
        A decorator that conditionally applies the PDF filter.
    """

    def wrapper(pdf_filter: PdfFilterFunc) -> PdfFilterFunc:
        def conditionated_pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
            parts = []
            if condition(xml_root):
                parts = pdf_filter(xml_root)
            return parts

        return conditionated_pdf_filter

    return wrapper


def standard_extraction_subfund(
    subfund_set: PdfLineSet,
) -> Callable[[UpdateMetadataFunc], UpdateMetadataFunc]:
    """Decorator for extracting subfund text and updating metadata.

    Parameters
    ----------
    subfund_height : YRange
        The vertical range in which the subfund text is expected.
    subfund_font : str
        The font used by the subfund text.

    Returns
    -------
    Callable[[UpdateMetadataFunc], UpdateMetadataFunc]
        A decorator that updates metadata with the extracted subfund text.
    """

    def decorator(old_page_metadata):
        def new_page_metadata(xml_root: etree.Element) -> List[PdfBlock]:
            xml_lines = None
            if subfund_set.font is not None:
                xml_lines = get_lines_with_font(xml_root, subfund_set.font)
            else:
                xml_lines = xml_root.findall(".//line")

            lines = [ExtractedPdfLine(blk) for blk in xml_lines]
            subfund = None
            try:
                subfund = [line.text for line in lines if line in subfund_set][0]
            except IndexError as exc:
                raise ExpectedPdfBlockNotFound(
                    _("subfund block on top of page not found")
                ) from exc
            metadata = old_page_metadata(xml_root)
            metadata["subfund"] = subfund
            return metadata

        return new_page_metadata

    return decorator


def standard_extraction_currency(
    currency_set: PdfLineSet | Currency | str,
) -> Callable[[UpdateMetadataFunc], UpdateMetadataFunc]:
    """Decorator for extracting currency text and updating metadata.

    Parameters
    ----------
    subfund_height : YRange
        The vertical range in which the currency text is expected.
    subfund_font : str
        The font used by the currency text.

    Returns
    -------
    Callable[[UpdateMetadataFunc], UpdateMetadataFunc]
        A decorator that updates metadata with the extracted currency text.
    """

    def decorator(old_page_metadata):
        def new_page_metadata(xml_root: etree.Element) -> List[PdfBlock]:
            metadata = old_page_metadata(xml_root)

            if isinstance(currency_set, str):
                metadata["currency"] = Currency[currency_set]
                return metadata
            elif isinstance(currency_set, Currency):
                metadata["currency"] = currency_set
                return metadata

            xml_lines = None
            if currency_set.font is not None:
                xml_lines = get_lines_with_font(xml_root, currency_set.font)
            else:
                xml_lines = xml_root.findall(".//line")

            lines = [ExtractedPdfLine(blk) for blk in xml_lines]
            currency = None
            try:
                currency = [line.text for line in lines if line in currency_set][0]
            except IndexError as exc:
                raise ExpectedPdfBlockNotFound(_("currency block  not found")) from exc

            metadata["currency"] = currency
            return metadata

        return new_page_metadata

    return decorator


def standard_pdf_filtering(
    header_set: PdfLineSet | List[PdfLineSet],
    subfund_set: PdfLineSet,
    body_set: PdfLineSet,
    currency_set: PdfLineSet | Currency | str,
    deselection_list: Optional[List[PdfLineSet]] = [],
    algorithm_flags: List | TablePosAlgorithm = TablePosAlgorithm(0),
    tolerance: float = 0.0,
) -> Callable[[PdfFilterFunc], PdfFilterFunc]:
    """Decorator factory for creating PDF filters with standardized processing.

    Creates a filter that:
    1. Processes pages containing the specified header text in the specified header font.
    2. Extracts lines with the specified body font as relevant blocks.
    3. Extracts subfund text within a specified range or height.
    4. Allows customization of page metadata and block types.

    Parameters
    ----------
    header_txt : str
        The text that must be present in the header to process the page.
    header_font : Font
        The font used by the header text.
    subfund_height : YRange
        The vertical range or height for subfund extraction.
    subfund_font : Font
        The font used by the subfund text.
    body_font : Union[str, List[str]]
        The font or list of fonts used by the body text to extract as relevant blocks.
    y_range : Optional[Tuple[Optional[float | Tuple[str, str]], Optional[float | Tuple[str, str]]]
        The vertical range for filtering lines, by default None.
    deselection_list : Optional[Tuple[str, Font]], optional
        A list of text and font pairs to exclude from extraction, by default None.

    Returns
    -------
    Callable[[PdfFilterFunc], PdfFilterFunc]
        A decorator that applies the standardized PDF filter.
    """

    def decorator(f):
        @standard_extraction_subfund(subfund_set)
        @standard_extraction_currency(currency_set)
        @overwrite_if_implemented(f)
        def page_metadata(_: etree.Element) -> dict:
            return {}

        def _is_header(xml_root, header_set) -> bool:
            if not isinstance(header_set, list):
                header_set = [header_set]
            for hs in header_set:
                if hs.font is not None:
                    if hs.text is not None:
                        rows = get_lines_with_txt_font(
                            xml_root, hs.text, hs.font, all_elem=True
                        )
                    else:
                        rows = get_lines_with_font(xml_root, hs.font)
                else:
                    rows = xml_root.findall(".//line")
                lines = [ExtractedPdfLine(line) for line in rows]
                lines = [line for line in lines if line in hs]
                if len(lines) == 0:
                    return False
            return True

        @filter_page_if(lambda x: _is_header(x, header_set))
        def pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
            _algorithm_flags = algorithm_flags
            metadata = {}
            try:
                metadata = page_metadata(xml_root)
            except ExpectedPdfBlockNotFound as e:
                logger.warning(e)

            rows = []
            if body_set.font is not None:
                rows = get_lines_with_font(xml_root, body_set.font)
            else:
                rows = xml_root.findall(".//line")
            rows = [ExtractedPdfLine(r) for r in rows]

            table_rows = [row for row in rows if row in body_set]
            for deselection_set in deselection_list:
                table_rows = [
                    table_row
                    for table_row in table_rows
                    if (table_row not in deselection_set)
                ]
            if isinstance(_algorithm_flags, list):
                all_flags = [
                    TablePosAlgorithm.ROW,
                    TablePosAlgorithm.BIG_RULE,
                    TablePosAlgorithm.RULER_AREA,
                    TablePosAlgorithm.TEST_POS,
                ]
                algo = TablePosAlgorithm(0)  # valore vuoto (nessun flag attivo)
                for flag, enabled in zip(all_flags, _algorithm_flags):
                    if enabled:
                        algo |= flag
                _algorithm_flags = algo

            table_positions = get_table_positions(
                table_rows, algorithm_flags=_algorithm_flags, tolerance=tolerance
            )
            return [
                PdfBlock(
                    OnePdfBlockType.RELEVANT_BLOCK,
                    {**metadata, "table-col": table_positions[i]},
                    table_row.xml_blk,
                )
                for i, table_row in enumerate(table_rows)
            ]

        return pdf_filter

    return decorator
