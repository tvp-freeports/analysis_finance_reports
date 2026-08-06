"""Utilities for writing `pdf_extract` functions.

This module provides decorators and utilities for filtering and processing PDF content
based on XML elements, fonts, and positional data.
"""

from typing import List, Optional, TypeAlias, Callable, Set
from enum import Enum
import logging
from freeports._internals.core.classes import (
    PdfBlock,
    TextBlock,
    ExpectedPdfBlockNotFound,
    PageParseFail,
)
from freeports._internals.core.promises import Promise
from freeports._internals.commons.consts import Currency, SfdrArticle
from freeports._internals.formats.utils.pdf_extract.common import (
    SelectExpectedText,
    ExtractTextPdfBlockOrFailPage,
)
from freeports.utils.pdf_extract import (
    TablePosAlgorithm,
    get_table_coordinates,
    CollapseAlgorithm,
    TableConfig,
    ColumnConfig,
    pdflines_from_pagedict,
    PdfLineSelection,
)
from freeports.interfaces.pdf_blks import ResultStandardExtraction, OnePdfBlockType

UpdateMetadataFunc: TypeAlias = Callable[[dict], dict]
"""Type alias for metadata update functions.

Functions that extract additional metadata from XML elements and return
metadata dictionaries for PDF blocks.
"""

FilterCondition: TypeAlias = Callable[[dict], bool]
"""Type alias for filter condition functions.

Predicate functions that determine whether a PDF filter should be applied
to a given XML element.
"""

PdfFilterFunc: TypeAlias = Callable[[dict], List[TextBlock]]
"""Type alias for PDF filter functions.

Functions that process XML elements and return lists of relevant PDF blocks.
"""

logger = logging.getLogger(__name__)


class PdfExtractSfdrArticleStandard:
    """Extracts SFDR article classification from a PDF page."""

    def __init__(
        self,
        art9_selection: PdfLineSelection = PdfLineSelection,
        art8_selection: PdfLineSelection = PdfLineSelection,
        fund_selection: PdfLineSelection = PdfLineSelection,
    ) -> None:
        """Initialize the SFDR article extractor.

        Parameters
        ----------
        art9_selection : PdfLineSelection
            Selection criteria for Article 9 indication.
        art8_selection : PdfLineSelection
            Selection criteria for Article 8 indication.
        fund_selection : PdfLineSelection
            Selection criteria for fund name.
        """
        self.art9_selection = art9_selection
        self.art8_selection = art8_selection
        self.fund_pdflineselection = fund_selection

    def __call__(self, page: dict) -> list[PdfBlock]:
        """Extract SFDR article classification from a PDF page.

        Parameters
        ----------
        page : dict
            The PDF page dictionary.

        Returns
        -------
        list[PdfBlock]
            List containing the SFDR article PDF block.

        Raises
        ------
        ExpectedPdfBlockNotFound
            If the fund name cannot be found.
        """
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
    """Extract the fund name from a PDF page."""

    def __init__(self, selection: PdfLineSelection) -> None:
        super().__init__(
            selection=selection,
            name="fund",
            type_block=ResultStandardExtraction.FUND_NAME,
        )


class PdfExtractCurrencyStandard(ExtractTextPdfBlockOrFailPage):
    """Extract the currency from a PDF page."""

    def __init__(self, selection: PdfLineSelection) -> None:
        super().__init__(
            selection=selection,
            name="currency",
            type_block=ResultStandardExtraction.CURRENCY_STATEMENT,
        )


class PdfExtractManagmentCompanyStandard(ExtractTextPdfBlockOrFailPage):
    """Extract the management company from a PDF page."""

    def __init__(self, selection: PdfLineSelection) -> None:
        super().__init__(
            selection=selection,
            name="managment company",
            type_block=ResultStandardExtraction.MANAGEMENT_COMPANY,
        )


class PdfExtractCurrencyConstant:
    """Returns a constant currency PDF block regardless of page content."""

    def __init__(self, currency: Currency) -> None:
        """Initialize with a constant currency.

        Parameters
        ----------
        currency : Currency
            The currency to always return.
        """
        self.currency = currency
        self._blk = PdfBlock(
            ResultStandardExtraction.CURRENCY_STATEMENT, {}, currency.name
        )

    def __call__(self, dict_root: dict) -> list[PdfBlock]:
        """Return a constant currency PDF block.

        Parameters
        ----------
        dict_root : dict
            The PDF page dictionary (unused).

        Returns
        -------
        list[PdfBlock]
            List containing the constant currency block.
        """
        return [self._blk]


class PdfExtractPageClassifyStandard:
    """Classifies pages based on header selection criteria matching."""

    header_sets: Set[PdfLineSelection]
    page_type: str

    def __init__(
        self, header_sets: PdfLineSelection | list[PdfLineSelection], page_type: str
    ) -> None:
        """Initialize the page classifier.

        Parameters
        ----------
        header_sets : PdfLineSelection | list[PdfLineSelection]
            One or more header selection criteria that must all match.
        page_type : str
            The page type to assign when all headers match.
        """
        self.header_sets = set()
        try:
            for h in header_sets:
                self.header_sets.add(h)
        except TypeError:
            self.header_sets.add(header_sets)
        self.page_type = page_type

    def __call__(self, dict_root: dict) -> list[PdfBlock]:
        """Classify a page based on header matching.

        Parameters
        ----------
        dict_root : dict
            The PDF page dictionary.

        Returns
        -------
        list[PdfBlock]
            List containing the page classification block.
        """
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
    """Extracts investment table blocks from a PDF page using positional criteria."""

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
        body_set: PdfLineSelection,
        manco_set: Optional[PdfLineSelection] = None,
        currency_set: Optional[PdfLineSelection] = None,
        deselection_list: list[PdfLineSelection] = [],
        algorithm_flags: list | TablePosAlgorithm = TablePosAlgorithm(0),
        tolerance: float = 0.0,
        row_algorithm_flags: list | TablePosAlgorithm = TablePosAlgorithm(0),
        row_tolerance: float = 0.0,
        company_index: Optional[int] = None,
    ) -> None:
        """Initialize the investments extractor.

        Parameters
        ----------
        body_set : PdfLineSelection
            Selection criteria for the table body rows.
        manco_set : Optional[PdfLineSelection]
            Selection criteria for the management company.
        currency_set : Optional[PdfLineSelection]
            Selection criteria for the currency.
        deselection_list : list[PdfLineSelection]
            Selections to subtract from body_set.
        algorithm_flags : list | TablePosAlgorithm
            Table position algorithm flags.
        tolerance : float
            Tolerance for table coordinate calculation.
        row_algorithm_flags : list | TablePosAlgorithm
            Row-level table position algorithm flags.
        row_tolerance : float
            Tolerance for row coordinate calculation.
        company_index : Optional[int]
            Index of the company column.
        """
        for dl in deselection_list:
            body_set /= dl
        self.body_set = body_set
        self.algorithm_flags = algorithm_flags
        self.tolerance = tolerance
        self.row_algorithm_flags = row_algorithm_flags
        self.row_tolerance = row_tolerance
        self.company_index = company_index

    def __call__(self, dict_root: dict) -> list[PdfBlock]:
        """Extract investment table blocks from a PDF page.

        Parameters
        ----------
        dict_root : dict
            The PDF page dictionary.

        Returns
        -------
        list[PdfBlock]
            List of PDF blocks representing table rows with position metadata.
        """
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
                TablePosAlgorithm.USE_TEST_POS,
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
                TablePosAlgorithm.USE_TEST_POS,
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


class PdfExtractAssetsStandard:
    """Extract fund assets data from a PDF page using positional criteria.

    Uses moving window area selections to locate net assets, liabilities,
    and total assets values relative to anchor text positions.
    """

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
        fund_set: PdfLineSelection,
        currency_set: Optional[PdfLineSelection],
        net_assets_set: PdfLineSelection,
        liabilities_set: PdfLineSelection,
        tot_assets_set: PdfLineSelection,
        net_assets_vec: tuple[float, float] = (1.2, 0.0),
        liabilities_vec: tuple[float, float] = (1.2, 0.0),
        tot_assets_vec: tuple[float, float] = (1.2, 0.0),
        net_assets_mult: tuple[float, float] = (100.0, 1.3),
        liabilities_mult: tuple[float, float] = (100.0, 1.3),
        tot_assets_mult: tuple[float, float] = (100.0, 1.3),
        date_set: Optional[PdfLineSelection] = None,
        table_condition: bool = False,
        skip_column: int = 1,
    ) -> None:
        """Initialize the assets extractor.

        Parameters
        ----------
        fund_set : PdfLineSelection
            Selection criteria for fund names.
        currency_set : Optional[PdfLineSelection]
            Selection criteria for currency.
        net_assets_set : PdfLineSelection
            Selection criteria for net assets anchor text.
        liabilities_set : PdfLineSelection
            Selection criteria for liabilities anchor text.
        tot_assets_set : PdfLineSelection
            Selection criteria for total assets anchor text.
        net_assets_vec : tuple[float, float]
            Direction vector from anchor to net assets value.
        liabilities_vec : tuple[float, float]
            Direction vector from anchor to liabilities value.
        tot_assets_vec : tuple[float, float]
            Direction vector from anchor to total assets value.
        net_assets_mult : tuple[float, float]
            Multiplier for net assets area size (width, height).
        liabilities_mult : tuple[float, float]
            Multiplier for liabilities area size (width, height).
        tot_assets_mult : tuple[float, float]
            Multiplier for total assets area size (width, height).
        date_set : Optional[PdfLineSelection]
            Selection criteria for the date.
        table_condition : bool
            Whether fund data is in table format.
        skip_column : int
            Number of columns to skip between entries.
        """
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

    def __call__(self, dict_root: dict) -> list[PdfBlock]:
        """Extract assets data from a PDF page.

        Parameters
        ----------
        dict_root : dict
            The PDF page dictionary.

        Returns
        -------
        list[PdfBlock]
            List of PDF blocks with fund assets metadata.
        """
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
