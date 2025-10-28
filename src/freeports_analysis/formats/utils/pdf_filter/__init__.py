"""Utilities for writing `pdf_filter` functions.

This module provides decorators and utilities for filtering and processing PDF content
based on XML elements, fonts, and positional data.
"""

from typing import List, Optional, TypeAlias, Callable
from enum import Enum, auto
import logging
from lxml import etree
from freeports_analysis.formats import (
    PdfBlock,
    ExpectedPdfBlockNotFound,
    PageParseFail,
    TextBlock,
)
from freeports_analysis.i18n import _
from freeports_analysis.consts import Promise
from freeports_analysis.consts import Currency
from .xml.font import get_lines_with_font, get_lines_with_txt_font
from .select_position import get_table_positions, TablePosAlgorithm
from .pdf_parts import ExtractedPdfLine, PdfLineSet
from .. import overwrite_if_implemented

UpdateMetadataFunc: TypeAlias = Callable[[etree.Element], dict]
"""Type alias for metadata update functions.

Functions that extract additional metadata from XML elements and return
metadata dictionaries for PDF blocks.
"""

FilterCondition: TypeAlias = Callable[[etree.Element], bool]
"""Type alias for filter condition functions.

Predicate functions that determine whether a PDF filter should be applied
to a given XML element.
"""

PdfFilterFunc: TypeAlias = Callable[[etree.Element], List[TextBlock]]
"""Type alias for PDF filter functions.

Functions that process XML elements and return lists of relevant PDF blocks.
"""

logger = logging.getLogger(__name__)


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

    Notes
    -----
    This decorator is useful for creating filters that should only run on
    specific types of pages (e.g., pages containing certain headers or
    specific layout elements).
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
            subfund = None
            if isinstance(subfund_set, Promise):
                subfund = subfund_set
            else:
                subfund_set_c = subfund_set.contextualize(xml_root)
                if subfund_set_c.is_simple and subfund_set_c._left.font is not None:
                    xml_lines = get_lines_with_font(xml_root, subfund_set_c._left.font)
                else:
                    xml_lines = xml_root.findall(".//line")
                lines = [ExtractedPdfLine(blk) for blk in xml_lines]
                try:
                    subfund = [line.text for line in lines if line in subfund_set_c][0]
                except IndexError as exc:
                    logger.error(exc)
                    logger.debug("Subfund set:")
                    logger.debug(str(subfund_set_c))
                    logger.debug("First lines where:")
                    logger.debug(
                        "%s",
                        str(list(map(lambda x: x.text, lines))[: min(10, len(lines))]),
                    )
                    raise ExpectedPdfBlockNotFound(
                        _("Subfund block on top of page not found")
                    ) from exc
            metadata = old_page_metadata(xml_root)
            metadata["subfund"] = subfund
            return metadata

        return new_page_metadata

    return decorator


def standard_extraction_currency(
    currency_set: PdfLineSet | Currency | str,
) -> Callable[[UpdateMetadataFunc], UpdateMetadataFunc]:
    """Decorator for extracting currency information and updating metadata.

    Parameters
    ----------
    currency_set : PdfLineSet | Currency | str
        The source of currency information. It can be:
        - a PdfLineSet containing raw text lines to search for a currency,
        - a Currency object directly,
        - or a string representing the currency code (e.g., "USD").

    Returns
    -------
    Callable[[UpdateMetadataFunc], UpdateMetadataFunc]
        A decorator that enhances a metadata update function by extracting
        the currency and storing it in the metadata.
    """

    def decorator(old_page_metadata):
        def new_page_metadata(xml_root: etree.Element) -> List[PdfBlock]:
            metadata = old_page_metadata(xml_root)
            if isinstance(currency_set, str):
                metadata["currency"] = Currency[currency_set]
                return metadata
            if isinstance(currency_set, Currency):
                metadata["currency"] = currency_set
                return metadata

            xml_lines = None
            currency_set_c = currency_set.contextualize(xml_root)
            if currency_set_c.is_simple and currency_set_c._left.font is not None:
                xml_lines = get_lines_with_font(xml_root, currency_set_c._left.font)
            else:
                xml_lines = xml_root.findall(".//line")

            lines = [ExtractedPdfLine(blk) for blk in xml_lines]
            currency = None
            try:
                currency = [line.text for line in lines if line in currency_set_c][0]
            except IndexError as exc:
                logger.error(exc)
                logger.debug("Currency set:")
                logger.debug(str(currency_set_c))
                logger.debug("First lines where:")
                logger.debug(
                    "%s",
                    str(list(map(lambda x: x.text, lines))[: min(10, len(lines))]),
                )
                raise ExpectedPdfBlockNotFound(_("Currency block  not found")) from exc

            metadata["currency"] = currency
            return metadata

        return new_page_metadata

    return decorator


def standard_pdf_filtering(
    header_set: PdfLineSet | List[PdfLineSet],
    subfund_set: PdfLineSet,
    body_set: PdfLineSet,
    currency_set: PdfLineSet | Currency | str,
    deselection_list: Optional[List[PdfLineSet]] = None,
    algorithm_flags: List | TablePosAlgorithm = TablePosAlgorithm(0),
    tolerance: float = 0.0,
    row_algorithm_flags: List | TablePosAlgorithm = TablePosAlgorithm(0),
    row_tolerance: float = 0.0,
) -> Callable[[PdfFilterFunc], PdfFilterFunc]:
    """Decorator factory for creating PDF filters with standardized processing.

    This decorator factory creates a comprehensive PDF processing pipeline that:
    - Identifies relevant pages based on header criteria
    - Extracts structured data from tabular content
    - Handles subfund and currency information
    - Applies geometric analysis for table detection
    - Supports deselection of unwanted content

    Parameters
    ----------
    header_set : PdfLineSet | List[PdfLineSet]
        Criteria for identifying page headers. Can be a single set or list of sets
        for multiple header conditions. Pages must match all header criteria.
    subfund_set : PdfLineSet
        Criteria for extracting subfund information from the page.
    body_set : PdfLineSet
        Criteria for identifying the main body content (tabular data).
    currency_set : PdfLineSet | Currency | str
        Source of currency information. Can be:
        - PdfLineSet: Extract currency from page content
        - Currency: Use fixed currency value
        - str: Currency code (e.g., "USD")
    deselection_list : Optional[List[PdfLineSet]], optional
        List of criteria for content to exclude from extraction.
        Default is empty list.
    algorithm_flags : List | TablePosAlgorithm, optional
        Algorithm flags for column position detection.
        Default is TablePosAlgorithm(0) - no special flags.
    tolerance : float, optional
        Tolerance for column position matching.
        Default is 0.0.
    row_algorithm_flags : List | TablePosAlgorithm, optional
        Algorithm flags for row position detection.
        Default is TablePosAlgorithm(0) - no special flags.
    row_tolerance : float, optional
        Tolerance for row position matching.
        Default is 0.0.

    Returns
    -------
    Callable[[PdfFilterFunc], PdfFilterFunc]
        A decorator that applies the standardized PDF filter processing.

    Notes
    -----
    The created filter performs the following operations:
    1. **Header Detection**: Checks if page contains specified header(s)
    2. **Subfund Extraction**: Extracts subfund information from specified area
    3. **Currency Extraction**: Determines currency for financial data
    4. **Body Content Filtering**: Identifies relevant tabular content
    5. **Table Structure Analysis**: Detects rows and columns using geometric algorithms
    6. **Deselection**: Removes unwanted content based on deselection criteria
    7. **Metadata Enrichment**: Adds table position information to blocks

    The algorithm supports complex table layouts through configurable
    position detection algorithms and tolerance settings.

    Examples
    --------
    >>> @standard_pdf_filtering(
    ...     header_set=PdfLineSet(font="Arial-Bold", text="PORTFOLIO HOLDINGS"),
    ...     subfund_set=PdfLineSet(font="Arial", area=((0, 100), (700, 750))),
    ...     body_set=PdfLineSet(font="Arial", font_size=10),
    ...     currency_set="USD",
    ...     deselection_list=[PdfLineSet(text="TOTAL")]
    ... )
    >>> def my_pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
    ...     # Custom page metadata extraction
    ...     return {}
    """
    if deselection_list is None:
        deselection_list = []
    for deselection_set in deselection_list:
        body_set = body_set / deselection_set

    def decorator(f):
        @standard_extraction_subfund(subfund_set)
        @standard_extraction_currency(currency_set)
        @overwrite_if_implemented(f)
        def page_metadata(_: etree.Element) -> dict:
            return {}

        def _is_header(xml_root, header_set) -> bool:
            if not isinstance(header_set, list):
                header_set = [header_set]
            for hsa in header_set:
                hs = hsa.contextualize(xml_root)
                if (
                    hs.is_simple
                    and hs._left.font is not None
                    and len(hs._left.font) == 1
                ):
                    if hs._left.text is not None and hs._left.text.is_simple:
                        rows = get_lines_with_txt_font(
                            xml_root,
                            list(hs._left.text._left)[0],
                            list(hs._left.font)[0],
                            all_elem=True,
                        )
                    else:
                        rows = get_lines_with_font(xml_root, list(hs._left.font)[0])
                else:
                    rows = xml_root.findall(".//line")
                lines = [ExtractedPdfLine(line) for line in rows]
                lines = [line for line in lines if line in hs]
                if len(lines) == 0:
                    return False
            return True

        @filter_page_if(lambda x: _is_header(x, header_set))
        def pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
            body_set_c = body_set.contextualize(xml_root)
            _algorithm_flags = algorithm_flags
            _row_algorithm_flags = row_algorithm_flags
            metadata = {}
            try:
                metadata = page_metadata(xml_root)
            except ExpectedPdfBlockNotFound as e:
                raise PageParseFail(e) from e

            rows = []
            if (
                (body_set_c.is_simple)
                and (body_set_c._left.font is not None)
                and len(body_set_c._left.font) == 1
            ):
                rows = get_lines_with_font(xml_root, list(body_set_c._left.font)[0])
            else:
                rows = xml_root.findall(".//line")
            rows = [ExtractedPdfLine(r) for r in rows]
            table_rows = [row for row in rows if row in body_set_c]
            # Check if the whole table is empty
            if table_rows == []:
                return []

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
            if isinstance(_row_algorithm_flags, list):
                all_flags = [
                    TablePosAlgorithm.ROW,
                    TablePosAlgorithm.BIG_RULE,
                    TablePosAlgorithm.RULER_AREA,
                    TablePosAlgorithm.TEST_POS,
                ]
                algo = TablePosAlgorithm(0)  # valore vuoto (nessun flag attivo)
                for flag, enabled in zip(all_flags, _row_algorithm_flags):
                    if enabled:
                        algo |= flag
                _row_algorithm_flags = algo

            table_col_positions = get_table_positions(
                table_rows, algorithm_flags=_algorithm_flags, tolerance=tolerance
            )
            table_row_positions = get_table_positions(
                table_rows,
                algorithm_flags=_row_algorithm_flags | TablePosAlgorithm.ROW,
                tolerance=row_tolerance,
            )

            def _width(area):
                bounds = area.bounds
                return bounds[2] - bounds[0]

            table_cell_widths = [_width(table_row.area) for table_row in table_rows]
            max_width = max(table_cell_widths)
            is_max_width = [width == max_width for width in table_cell_widths]
            return [
                PdfBlock(
                    OnePdfBlockType.RELEVANT_BLOCK,
                    {
                        **metadata,
                        "table-row": table_row_positions[i],
                        "table-col": table_col_positions[i],
                        "is-max-width": is_max_width[i],
                    },
                    table_row.xml_blk,
                )
                for i, table_row in enumerate(table_rows)
            ]

        return pdf_filter

    return decorator
