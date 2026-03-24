"""Utilities for writing `pdf_extract` functions.

This module provides decorators and utilities for filtering and processing PDF content
based on XML elements, fonts, and positional data.
"""

from typing import List, Optional, TypeAlias, Callable, Set
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
from .select_position import (
    TablePosAlgorithm,
    get_table_coordinates,
    CollapseAlgorithm,
    TableConfig,
    ColumnConfig,
)
from .pdf_parts import pdflines_from_pagedict, PdfLineSelection


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


class SelectExpectedText:
    selection: PdfLineSelection
    name: str

    def __init__(self, selection, name="expected text"):
        self.selection = selection
        self.name = name

    def __call__(self, lines):
        try:
            return self.selection.select(lines)[0].text
        except IndexError as exc:
            logger.error(exc)
            logger.debug("First lines where:")
            logger.debug(
                "%s",
                str(list(map(lambda x: x.text, lines))[: min(10, len(lines))]),
            )
            raise ExpectedPdfBlockNotFound(_(f"{self.name} not found")) from exc


class ResultStandardExtraction(Enum):
    FUND_NAME = auto()
    CURRENCY_STATEMENT = auto()
    TABLE_BODY = auto()


class PdfExtractFundStandard:
    extractor: SelectExpectedText

    def __init__(self, selection: PdfLineSelection):
        self.extractor = SelectExpectedText(selection, "fund")

    def __call__(self, dict_root):
        lines = pdflines_from_pagedict(dict_root)
        try:
            fund_name = self.extractor(lines)
        except ExpectedPdfBlockNotFound as e:
            raise PageParseFail(e) from e
        return [PdfBlock(ResultStandardExtraction.FUND_NAME, {}, fund_name)]


class PdfExtractCurrencyStandard:
    extractor: SelectExpectedText

    def __init__(self, selection: PdfLineSelection):
        self.extractor = SelectExpectedText(selection, "currency")

    def __call__(self, dict_root):
        lines = pdflines_from_pagedict(dict_root)
        try:
            fund_name = self.extractor(lines)
        except ExpectedPdfBlockNotFound as e:
            raise PageParseFail(e) from e
        return [PdfBlock(ResultStandardExtraction.CURRENCY_STATEMENT, {}, fund_name)]


class PdfExtractPageClassifyStandard:
    header_sets: Set[PdfLineSelection]
    page_type: str

    def __init__(self, header_sets, page_type):
        self.header_sets = set()
        try:
            for h in header_sets:
                self.header_sets.add(h)
        except TypeError:
            self.header_sets.add(header_sets)
        self.page_type = page_type

    def __call__(self, dict_root):
        lines = pdflines_from_pagedict(dict_root)
        page_type = self.page_type
        for hsa in self.header_sets:
            if len(hsa.select(lines)) == 0:
                page_type = None
                break
        return [PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {"page_type": page_type}, "")]


class PdfExtractInvestmentsStandard:
    body_set: PdfLineSelection
    currency_set: PdfLineSelection | Currency | str
    manco_set: Optional[PdfLineSelection]
    deselection_list: Optional[List[PdfLineSelection]]
    algorithm_flags: List | TablePosAlgorithm
    tolerance: float
    row_algorithm_flags: List | TablePosAlgorithm
    row_tolerance: float
    company_index: Optional[int]

    def __init__(
        self,
        body_set,
        currency_set,
        manco_set=None,
        deselection_list=[],
        algorithm_flags=TablePosAlgorithm(0),
        tolerance=0.0,
        row_algorithm_flags=TablePosAlgorithm(0),
        row_tolerance=0.0,
        company_index=None,
    ):
        self.manco_filter = StandardPageMetadataFilter(manco_set, "Management company")
        self.currency_filter = StandardPageMetadataFilter(currency_set, "Currency")
        for dl in deselection_list:
            body_set /= dl
        self.body_set = body_set
        self.algorithm_flags = algorithm_flags
        self.tolerance = tolerance
        self.row_algorithm_flags = row_algorithm_flags
        self.row_tolerance = row_tolerance
        self.company_index = company_index

    def __call__(self, dict_root):
        lines = pdflines_from_pagedict(dict_root)
        _algorithm_flags = self.algorithm_flags
        _row_algorithm_flags = self.row_algorithm_flags
        try:
            metadata = dict()
            metadata["currency"] = None
            if isinstance(self.currency_filter.selection, str):
                metadata["currency"] = Currency[self.currency_filter.selection]
            if isinstance(self.currency_filter.selection, Currency):
                metadata["currency"] = self.currency_filter.selection
            if metadata["currency"] is None:
                metadata["currency"] = self.currency_filter(lines)
            if self.manco_filter.selection is not None:
                metadata["manco"] = self.manco_filter(lines)
        except ExpectedPdfBlockNotFound as e:
            raise PageParseFail(e) from e

        table_rows = self.body_set.select(lines)
        # Check if the whole table is empty
        if table_rows == []:
            return []
        if isinstance(_algorithm_flags, list):
            all_flags = [
                TablePosAlgorithm.RETURN_ROWS,
                TablePosAlgorithm.BIG_CELL_RULE,
                TablePosAlgorithm.USE_RULER_AREA,
                TablePosAlgorithm.USE_TES_POS,
            ]
            algo = TablePosAlgorithm(0)  # valore vuoto (nessun flag attivo)
            for flag, enabled in zip(all_flags, _algorithm_flags):
                if enabled:
                    algo |= flag
            _algorithm_flags = algo
        if isinstance(_row_algorithm_flags, list):
            all_flags = [
                TablePosAlgorithm.RETURN_ROWS,
                TablePosAlgorithm.BIG_CELL_RULE,
                TablePosAlgorithm.USE_RULER_AREA,
                TablePosAlgorithm.USE_TES_POS,
            ]
            algo = TablePosAlgorithm(0)  # valore vuoto (nessun flag attivo)
            for flag, enabled in zip(all_flags, _row_algorithm_flags):
                if enabled:
                    algo |= flag
            _row_algorithm_flags = algo

        cfg = TableConfig()
        # cfg.cols=[]
        collapse_alg = CollapseAlgorithm.GEOMETRY
        coords = get_table_coordinates(
            table_rows,
            cfg,
            _algorithm_flags,
            collapse_alg,
            tolerance=self.row_tolerance,
            company_col=self.company_index,
            collapse=False,
        )
        table_row_positions, table_col_positions = zip(*coords)

        def _width(bounds):
            return bounds[2] - bounds[0]

        table_cell_widths = [_width(table_row.bbox) for table_row in table_rows]
        max_width = max(table_cell_widths)
        is_max_width = [width == max_width for width in table_cell_widths]
        return [
            PdfBlock(
                ResultStandardExtraction.TABLE_BODY,
                {
                    **metadata,
                    "table-row": table_row_positions[i],
                    "table-col": table_col_positions[i],
                    "is-max-width": is_max_width[i],
                },
                table_row.text,
            )
            for i, table_row in enumerate(table_rows)
        ]


class StandardPageMetadataFilter:
    selection: PdfLineSelection
    name: str

    def __init__(self, selection, name):
        self.selection = selection
        self.name = name

    def __call__(self, lines):
        if isinstance(self.selection, Promise):
            return self.selection
        try:
            return self.selection.select(lines)[0].text
        except IndexError as exc:
            logger.error(exc)
            logger.debug("First lines where:")
            logger.debug(
                "%s",
                str(list(map(lambda x: x.text, lines))[: min(10, len(lines))]),
            )
            raise ExpectedPdfBlockNotFound(_(f"{self.name} not found")) from exc


class OnePdfBlockType(Enum):
    """Enum representing types of PDF blocks in document processing.

    Attributes
    ----------
    RELEVANT_BLOCK : enum
        PDF block containing relevant information to extract.
    """

    RELEVANT_BLOCK = auto()
