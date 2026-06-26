"""Utilities for writing `pdf_extract` functions.

This module provides decorators and utilities for filtering and processing PDF content
based on XML elements, fonts, and positional data.
"""

from typing import List, Optional, TypeAlias, Callable, Set
from enum import Enum, auto
import logging
from abc import ABC
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
from freeports_analysis.consts import SfdrArticle
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

    def __init__(self, selection: PdfLineSelection, name="expected text"):
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
            raise ExpectedPdfBlockNotFound(
                f'Pdf block during extraction of "{self.name}" not found'
            ) from exc


class ResultStandardExtraction(Enum):
    FUND_NAME = auto()
    CURRENCY_STATEMENT = auto()
    TABLE_BODY = auto()
    MANAGEMENT_COMPANY = auto()
    INVESTMENTS_MANAGER = auto()
    SFDR_ARTICLE = auto()
    PAGE_CLASS = auto()


class ExtractTextPdfBlockOrFailPage:
    extractor: SelectExpectedText
    type_block: Enum

    def __init__(self, selection: PdfLineSelection, name: str, type_block: Enum):
        self.extractor = SelectExpectedText(selection, name)
        self.type_block = type_block

    def __call__(self, dict_root):
        lines = pdflines_from_pagedict(dict_root)
        try:
            text = self.extractor(lines)
        except ExpectedPdfBlockNotFound as e:
            raise PageParseFail(e) from e
        return [PdfBlock(self.type_block, {}, text)]


class PdfExtractSfdrArticleStandard:
    def __init__(
        self,
        art9_selection=PdfLineSelection,
        art8_selection=PdfLineSelection,
        fund_selection=PdfLineSelection,
    ):
        self.art9_selection = art9_selection
        self.art8_selection = art8_selection
        self.fund_pdflineselection = fund_selection

    def __call__(self, page):
        lines = pdflines_from_pagedict(page)
        art = SfdrArticle.ART_6
        if self.art8_selection.select(lines):
            art = SfdrArticle.ART_8
        elif self.art9_selection.select(lines):
            art = SfdrArticle.ART_9
        funds_blks = self.fund_pdflineselection.select(lines)
        txt = None
        if len(funds_blks) == 1:
            txt = next(iter(funds_blks)).text
        elif len(funds_blks) > 1:
            txt = "".join(
                map(lambda sb: sb.text, sorted(funds_blks, key=lambda b: b.bbox[1]))
            )
        else:
            raise ExpectedPdfBlockNotFound("Fund name")
        return [PdfBlock(ResultStandardExtraction.SFDR_ARTICLE, {"article": art}, txt)]


class PdfExtractFundStandard(ExtractTextPdfBlockOrFailPage):
    def __init__(self, selection: PdfLineSelection):
        super().__init__(
            selection=selection,
            name="fund",
            type_block=ResultStandardExtraction.FUND_NAME,
        )


class PdfExtractCurrencyStandard(ExtractTextPdfBlockOrFailPage):
    def __init__(self, selection: PdfLineSelection):
        super().__init__(
            selection=selection,
            name="currency",
            type_block=ResultStandardExtraction.CURRENCY_STATEMENT,
        )


class PdfExtractManagmentCompanyStandard(ExtractTextPdfBlockOrFailPage):
    def __init__(self, selection: PdfLineSelection):
        super().__init__(
            selection=selection,
            name="managment company",
            type_block=ResultStandardExtraction.MANAGEMENT_COMPANY,
        )


class PdfExtractCurrencyConstant:
    def __init__(self, currency: Currency):
        self.currency = currency
        self._blk = PdfBlock(
            ResultStandardExtraction.CURRENCY_STATEMENT, {}, currency.name
        )

    def __call__(self, dict_root):
        return [self._blk]


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
        return [
            PdfBlock(ResultStandardExtraction.PAGE_CLASS, {"page_type": page_type}, "")
        ]


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
        manco_set=None,
        currency_set=None,
        deselection_list=[],
        algorithm_flags=TablePosAlgorithm(0),
        tolerance=0.0,
        row_algorithm_flags=TablePosAlgorithm(0),
        row_tolerance=0.0,
        company_index=None,
    ):
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
        # try:
        #     metadata = dict()
        #     metadata["currency"] = None
        #     if isinstance(self.currency_filter.selection, str):
        #         metadata["currency"] = Currency[self.currency_filter.selection]
        #     if isinstance(self.currency_filter.selection, Currency):
        #         metadata["currency"] = self.currency_filter.selection
        #     if metadata["currency"] is None:
        #         metadata["currency"] = self.currency_filter(lines)
        #     if self.manco_filter.selection is not None:
        #         metadata["manco"] = self.manco_filter(lines)
        # except ExpectedPdfBlockNotFound as e:
        #     raise PageParseFail(e) from e

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
                    "table-row": table_row_positions[i],
                    "table-col": table_col_positions[i],
                    "is-max-width": is_max_width[i],
                },
                table_row.text,
            )
            for i, table_row in enumerate(table_rows)
        ]


class OnePdfBlockType(Enum):
    """Enum representing types of PDF blocks in document processing.

    Attributes
    ----------
    RELEVANT_BLOCK : enum
        PDF block containing relevant information to extract.
    """

    RELEVANT_BLOCK = auto()


class PdfExtractAssetsStandard:
    fund_set: PdfLineSelection
    currency_set: Optional[PdfLineSelection]
    net_assets_set: PdfLineSelection
    liabilities_set: PdfLineSelection
    tot_assets_set: PdfLineSelection
    net_assets_vec: tuple[float, float]
    liabilities_vec: tuple[float, float]
    tot_assets_vec: tuple[float, float]
    net_assets_mult: tuple[float, float]
    liabilities_mult: tuple[float, float]
    tot_assets_mult: tuple[float, float]

    def __init__(
        self,
        fund_set,
        currency_set,
        net_assets_set,
        liabilities_set,
        tot_assets_set,
        net_assets_vec=(1.2, 0.0),
        liabilities_vec=(1.2, 0.0),
        tot_assets_vec=(1.2, 0.0),
        net_assets_mult=(100.0, 1.3),
        liabilities_mult=(100.0, 1.3),
        tot_assets_mult=(100.0, 1.3),
        date_set=None,
        table_condition=False,
        skip_column=1,
    ):
        if not table_condition:
            self.fund_selection = SelectExpectedText(fund_set, "fund")
            self.currency_selection = SelectExpectedText(currency_set, "currency")
        else:
            self.fund_selection = fund_set
            self.currency_selection = (
                SelectExpectedText(currency_set, "currency")
                if currency_set is not None
                else currency_set
            )

        self.table_condition = table_condition
        self.skip_column = skip_column
        self.tot_assets_selction = tot_assets_set
        self.liabilities_selection = liabilities_set
        self.net_assets_selection = net_assets_set
        self.tot_assets_vector = tot_assets_vec
        self.liabilities_vector = liabilities_vec
        self.net_assets_vector = net_assets_vec
        self.tot_assets_width = tot_assets_mult[0]
        self.liabilities_width = liabilities_mult[0]
        self.net_assets_width = net_assets_mult[0]
        self.tot_assets_height = tot_assets_mult[1]
        self.liabilities_height = liabilities_mult[1]
        self.net_assets_height = net_assets_mult[1]
        self.select_date = (
            SelectExpectedText(date_set, "fund assets date")
            if date_set is not None
            else None
        )

    def __call__(self, dict_root):
        lines = pdflines_from_pagedict(dict_root)
        lines = (PdfLineSelection.text("") / PdfLineSelection.text("^ $")).select(lines)
        tot_assets = PdfLineSelection.area_from_movewindow(
            self.tot_assets_selction,
            self.tot_assets_vector,
            self.tot_assets_width,
            self.tot_assets_height,
        ).select(lines)
        liabilities = PdfLineSelection.area_from_movewindow(
            self.liabilities_selection,
            self.liabilities_vector,
            self.liabilities_width,
            self.liabilities_height,
        ).select(lines)
        net_assets = PdfLineSelection.area_from_movewindow(
            self.net_assets_selection,
            self.net_assets_vector,
            self.net_assets_width,
            self.net_assets_height,
        ).select(lines)
        tot_assets, liabilities, net_assets = zip(
            *tuple(
                (tot_assets[i], liabilities[i], net_assets[i])
                for i in range(0, len(tot_assets), self.skip_column)
            )
        )

        if not self.table_condition:
            funds = [self.fund_selection(lines)]
            currencies = [self.currency_selection(lines)]

        elif self.table_condition:
            funds = self.fund_selection.select(lines)
            _, cols = zip(
                *get_table_coordinates(
                    funds,
                    algorithm_flags=TablePosAlgorithm.BIG_CELL_RULE
                    | TablePosAlgorithm.USE_RULER_AREA,
                )
            )
            n_cols = max(cols) + 1
            funds = [
                " ".join((f.text.strip() for c, f in zip(cols, funds) if c == col))
                for col in range(n_cols)
            ]

            if self.currency_selection is not None:
                currency = self.currency_selection(lines)
                currencies = [currency] * len(funds)

            else:
                funds, currencies = zip(
                    *((" ".join(f.split()[:-1]), f.split()[-1]) for f in funds)
                )

        else:
            raise ValueError("Invalid configuration: fund_selection maybe None")

        tot_assets = [t.text for t in tot_assets]
        liabilities = [l.text for l in liabilities]
        net_assets = [n.text for n in net_assets]
        d = self.select_date(lines) if self.select_date is not None else None

        return [
            PdfBlock(
                OnePdfBlockType.RELEVANT_BLOCK,
                {
                    "fund": f,
                    "currency": c,
                    "tot_assets": t,
                    "liabilities": l,
                    "net_assets": n,
                    "date": d,
                },
                "",
            )
            for f, c, t, l, n in zip(
                funds, currencies, tot_assets, liabilities, net_assets
            )
        ]
