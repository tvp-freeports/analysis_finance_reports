"""Module for text block processing and extraction in document analysis.

This module provides functionality for:
- Defining text block types through enumerations
- Matching text against targets using various matching strategies
- Extracting text blocks from PDF documents based on target matches
- Supporting different matching methods (exact, fuzzy, prefix-based)

Key components:
- Decorators for text block type definition (one_txt_blk, ResultStandardExtraction)
- Standard text extraction functionality through standard_text_filterion decorator
"""

from enum import Enum, auto
import re
import logging
import freeports_lib
from typing import List, Optional, Tuple, Set
from freeports_analysis.i18n import _
from freeports_analysis.logging import LOG_ADAPT_INVESTMENT_INFOS
from freeports_analysis.formats import (
    TextBlock,
    PdfBlock,
    ExpectedTextBlockNotFound,
    LineParseFail,
    PageParseFail,
)
from freeports_analysis.formats.utils.pdf_extract import ResultStandardExtraction
from . import match
from freeports_analysis.consts import Currency
from freeports_analysis import output


logger = logging.getLogger(__name__)


class OneTextBlockType(Enum):
    RELEVANT_BLOCK = auto()


class ResultStandardFiltering(Enum):
    """Enum representing two type of text blocks in document processing.

    Attributes
    ----------
    BOND_TARGET : enum
        Text block containing target `Bond` row.
    EQUITY_TARGET : enum
        Text block containing target `Equity` row.
    """

    BOND_TARGET = auto()
    EQUITY_TARGET = auto()
    FUND = auto()
    MANAGEMENT_COMPANY = auto()
    INVESTMENTS_MANAGER = auto()


class PdfBlocksTable:
    """Represents a table structure of PDF blocks organized by row and column.

    This class provides a tabular view of PDF blocks based on their
    row and column metadata, enabling efficient access and manipulation
    of blocks in a grid-like structure. It transforms a flat list of
    PDF blocks into a 2D table structure for easier navigation and
    manipulation of tabular data extracted from PDF documents.

    Parameters
    ----------
    pdf_blocks : List[PdfBlock]
        A list of PDF blocks that should have 'table-row' and 'table-col'
        metadata indicating their position in the table structure.

    Attributes
    ----------
    _blks : List[PdfBlock]
        Original list of PDF blocks
    _table_indexes : List[List[List[int]]]
        Index mapping from table coordinates to block indices
    _table : List[List[List[PdfBlock]]]
        Table structure containing PDF blocks organized by row and column

    Notes
    -----
    - The table structure allows for sparse tables (empty cells)
    - Multiple blocks can occupy the same cell (represented as lists)
    - Row and column indices start from 0
    - The shape property provides table dimensions

    Examples
    --------
    >>> # Assuming blocks have table-row and table-col metadata
    >>> table = PdfBlocksTable(pdf_blocks)
    >>> print(f"Table shape: {table.shape}")
    Table shape: (5, 3)  # 5 rows, 3 columns
    >>>
    >>> # Access a specific cell
    >>> cell_content = table[2, 1]  # Row 2, Column 1
    >>>
    >>> # Iterate through all blocks
    >>> for block in table:
    ...     process_block(block)
    """

    def _get_table(self, pdf_blocks):
        """Convert flat list of PDF blocks into a table structure.

        Parameters
        ----------
        pdf_blocks : List[PdfBlock]
            List of PDF blocks with table-row and table-col metadata

        Returns
        -------
        Tuple[List[List[List[int]]], List[List[List[PdfBlock]]]]
            Tuple containing index mapping and table structure
        """
        table = []
        indexes = []
        dict_table = {}
        col_max = 0
        for i, blk in enumerate(pdf_blocks):
            row = blk.metadata["table-row"]
            col = blk.metadata["table-col"]
            if row not in dict_table:
                dict_table[row] = {}
            if col in dict_table[row]:
                dict_table[row][col].append((i, blk))
            else:
                col_max = max(col, col_max)
                dict_table[row][col] = [(i, blk)]
        for row in sorted(dict_table.keys()):
            cols = []
            i_cols = []
            for col in range(col_max + 1):
                if col in dict_table[row]:
                    idxs, blks = zip(*dict_table[row][col])
                    cols.append(list(blks))
                    i_cols.append(list(idxs))
                else:
                    cols.append([])
                    i_cols.append([])
            table.append(cols)
            indexes.append(i_cols)
        return indexes, table

    def __init__(self, pdf_blocks):
        """Initialize PdfBlocksTable with PDF blocks.

        Parameters
        ----------
        pdf_blocks : List[PdfBlock]
            List of PDF blocks to organize into table structure
        """
        self._blks = pdf_blocks.copy()
        self._table_indexes, self._table = self._get_table(self._blks)

    @property
    def _rows(self):
        """Number of rows in the table.

        Returns
        -------
        int
            Number of rows
        """
        return len(self._table)

    @property
    def _cols(self):
        """Number of columns in the table.

        Returns
        -------
        int
            Number of columns
        """
        return max(map(len, self._table)) if self._rows > 0 else 0

    def __getitem__(self, i):
        """Get block(s) by index or coordinates.

        Parameters
        ----------
        i : Union[int, Tuple[int, int]]
            Either a linear index or (row, column) tuple

        Returns
        -------
        Union[PdfBlock, List[PdfBlock], None]
            Single block, list of blocks, or None if not found
        """
        if isinstance(i, tuple):
            j, k = i
            vals = self._table[j][k]
            if len(vals) == 1:
                return vals[0]
            if len(vals) == 0:
                return None
            return vals
        return self._blks[i]

    def __len__(self):
        """Number of blocks in the table.

        Returns
        -------
        int
            Total number of PDF blocks
        """
        return len(self._blks)

    @property
    def shape(self):
        """Table dimensions.

        Returns
        -------
        Tuple[int, int]
            (number of rows, number of columns)
        """
        return (self._rows, self._cols)

    def pop(self, j):
        """Remove a block from the table by index.

        Parameters
        ----------
        j : int
            Index of the block to remove

        Notes
        -----
        Updates the table structure and adjusts row numbers for blocks
        that come after the removed row.
        """
        blk = self._blks.pop(j)
        col_del = blk.metadata["table-col"]
        row_del = blk.metadata["table-row"]
        for jdx, jdx_blk in enumerate(self._table_indexes[row_del][col_del]):
            if jdx_blk == j:
                self._table_indexes[row_del][col_del].pop(jdx)
                self._table[row_del][col_del].pop(jdx)
                self._table_indexes = [
                    [
                        [(i_ele) if i_ele < jdx_blk else (i_ele - 1) for i_ele in col]
                        for col in row
                    ]
                    for row in self._table_indexes
                ]
                break
        if all(not col for col in self._table_indexes[row_del]):
            self._table_indexes.pop(row_del)
            self._table.pop(row_del)
            for blk in self._blks:
                if blk.metadata["table-row"] > row_del:
                    blk.metadata["table-row"] -= 1

    def merge(self, j, i):
        """Merge two blocks by combining their content.

        Parameters
        ----------
        j : int
            Index of first block to merge
        i : int
            Index of second block to merge

        Notes
        -----
        The content of both blocks is concatenated and stored in the
        block with the lower index. The higher-indexed block is removed.
        """
        first, last = (i, j) if i < j else (j, i)
        content = self._blks[first].content + self._blks[last].content
        self._blks[i].content = content
        col = self._blks[i].metadata["table-col"]
        row = self._blks[i].metadata["table-row"]
        for idx, idx_blk in enumerate(self._table_indexes[row][col]):
            if idx_blk == i:
                self._table[row][col][idx].content = content
        self.pop(j)


def standard_text_filterion_loop(geometrical_indexes=True, merge_prev=False):
    """Decorator for standard text extraction loop.

    This decorator wraps the function provided in the usual loop that gives a simplified
    and higher level context to the decorated `text_filterion` function.
    Specifically it expects that in the metadata of each `PdfBlock` is present
    an indicator of which column it is located graphically in the main table of the
    PDF page (it assumes that the data was tabular in some way) `table-col`.

    Parameters
    ----------
    geometrical_indexes : bool, optional
        Whether to use (row, column) coordinates instead of linear indices, by default True
    merge_prev : bool, optional
        Whether to merge with previous block instead of next block, by default False

    Returns
    -------
    Callable
        Decorator that wraps text extraction functions with standard processing logic

    Notes
    -----
    The loop performs the following steps:
    - Takes each block and concatenates the content with the subsequent if
      they are on the same column.
    - Uses `match_func` to see if one between the target provided to the
      extraction function matches with the content of the block.
    - If it does, it overwrites the list of `PdfBlock` to persist the concatenation
      of the block with its subsequent.
    - Adds `company` metadata with the match
    - Creates a `TextBlock` adding the metadata provided by the wrapped function.
    """

    def decorator(f):
        def text_filter(
            pdf_blocks: List[PdfBlock], targets: List[str]
        ) -> List[TextBlock]:
            text_part_list = []
            i = 0
            if len(pdf_blocks) == 0:
                return text_part_list
            pdf_blocks_table = PdfBlocksTable(pdf_blocks)
            n_cols = pdf_blocks_table.shape[1]
            while i < len(pdf_blocks_table) - 1:
                company_name = False
                split = False
                current_block = pdf_blocks_table[i]
                next_block = pdf_blocks_table[i + 1]
                col = current_block.metadata["table-col"]
                row = current_block.metadata["table-row"]

                LOG_ADAPT_INVESTMENT_INFOS.row = row
                next_col = next_block.metadata["table-col"]
                next_row = next_block.metadata["table-row"]
                cell_width = current_block.metadata["is-max-width"]
                content = current_block.content
                if col == next_col:
                    split = False
                    n_full_cols = 0
                    empty_adj = 0
                    for c in range(n_cols):
                        if (
                            pdf_blocks_table[(row if merge_prev else next_row, c)]
                            is not None
                        ):
                            n_full_cols += 1
                        else:
                            if c in (col - 1, col + 1):
                                empty_adj += 1
                    if n_full_cols == 1 or empty_adj == 2:
                        split = True
                        if cell_width or (len(content) > 0 and content[-1] in " \n"):
                            content += next_block.content
                company = None
                company = freeports_lib.text_filter.matcher.match_company(
                    content, targets
                )

                if company is not None:
                    LOG_ADAPT_INVESTMENT_INFOS.company = company
                    LOG_ADAPT_INVESTMENT_INFOS.company_match = content
                    company_name = True
                    if company_name and split:
                        if merge_prev:
                            pdf_blocks_table.merge(i, i + 1)
                        else:
                            pdf_blocks_table.merge(i + 1, i)
                    try:
                        txt_blk = f(
                            pdf_blocks_table,
                            i if not geometrical_indexes else (row, col),
                        )
                        txt_blk.metadata["company match"] = content
                        txt_blk.metadata["company"] = company
                        text_part_list.append(txt_blk)
                    except ExpectedTextBlockNotFound:
                        logger.warning(_("Skipping line..."))
                    LOG_ADAPT_INVESTMENT_INFOS.company_match = None
                    LOG_ADAPT_INVESTMENT_INFOS.company = None
                i += 1
                if i >= len(pdf_blocks_table) - 1:
                    break
            if i == len(pdf_blocks_table) - 1:
                row = pdf_blocks_table[-1].metadata["table-row"]
                LOG_ADAPT_INVESTMENT_INFOS.row = row
                content = pdf_blocks_table[-1].content
                company = None
                company = freeports_lib.text_filter.matcher.match_company(
                    content, targets
                )

                if company is not None:
                    LOG_ADAPT_INVESTMENT_INFOS.company = company
                    LOG_ADAPT_INVESTMENT_INFOS.company_match = content
                    try:
                        txt_blk = f(
                            pdf_blocks_table,
                            i if not geometrical_indexes else (row, col),
                        )
                        txt_blk.metadata["company match"] = content
                        txt_blk.metadata["company"] = company
                        text_part_list.append(txt_blk)
                    except ExpectedTextBlockNotFound:
                        logger.warning(_("Skipping line..."))
                    LOG_ADAPT_INVESTMENT_INFOS.company = None
                    LOG_ADAPT_INVESTMENT_INFOS.company_match = None
            LOG_ADAPT_INVESTMENT_INFOS.row = None
            return text_part_list

        return text_filter

    return decorator


date_regexes = [
    r".*(\d{2}[/\-.]\d{2}[/\-.]\d{4}).*",
    r".*(\d{4}[/\-.]\d{2}[/\-.]\d{2}).*",
    r".*(\d{2}[/\-.]\d{2}[/\-.]\d{2}).*",
    r".*\s(\d{2}[/\-]\d{2})\s.*",
]
perc_regexes = [r"[a-zA-Z].*((\d+[\.,]\d+)\s*%).*", r"[a-zA-Z].*((\d+[\.,]\d+)\s*).*"]


class TextFilterPageClassifyStandard:
    def __call__(self, pdf_blks, _):
        page_classification = None
        for blk in pdf_blks:
            page_type = blk.metadata["page_type"]
            if page_type is not None:
                if page_classification is None:
                    page_classification = page_type
                else:
                    raise Exception(
                        f"page cannot be classified both as `{page_classification}` and `{page_type}`"
                    )
        return [
            TextBlock(
                OneTextBlockType.RELEVANT_BLOCK, {"page_type": page_classification}, blk
            )
        ]


def extract_currency_from_text(txt: str) -> Currency:
    curr = txt
    res = None
    if isinstance(curr, Currency):
        res = curr
    else:
        currency_candidates = re.findall(r"\b[A-Z]{3}\b", curr)
        found = False
        for curr_cand in currency_candidates:
            try:
                res = Currency[curr_cand]
                break
            except KeyError:
                pass
        if not found:
            curr = curr.upper()
            for c in Currency.__members__:
                currency_candidates = re.findall(r"\b" + c + r"\b", curr)
                for curr_cand in currency_candidates:
                    try:
                        res = Currency[curr_cand]
                        break
                    except KeyError:
                        pass
        if res is None:
            raise ExpectedTextBlockNotFound(
                _('Currency not found in string: "%s"'), curr
            )
    return res


class TextFilterInvestmentsStandard:
    market_value_pos: int
    nominal_quantity_pos: Optional[int]
    perc_net_assets_pos: Optional[int]
    acquisition_currency_pos: Optional[int]
    acquisition_cost_pos: Optional[int]
    geometrical_indexes: bool
    merge_prev: bool

    def __init__(
        self,
        market_value_pos: int,
        nominal_quantity_pos: Optional[int] = None,
        perc_net_assets_pos: Optional[int] = None,
        acquisition_currency_pos: Optional[int] = None,
        acquisition_cost_pos: Optional[int] = None,
        geometrical_indexes: bool = True,
        merge_prev: bool = False,
    ):
        if nominal_quantity_pos is not None and perc_net_assets_pos is not None:
            if (
                nominal_quantity_pos == market_value_pos
                or nominal_quantity_pos == perc_net_assets_pos
                or market_value_pos == perc_net_assets_pos
            ):
                raise ValueError(_("All positions should be different"))
        self.market_value_pos = market_value_pos
        self.nominal_quantity_pos = nominal_quantity_pos
        self.perc_net_assets_pos = perc_net_assets_pos
        self.acquisition_currency_pos = acquisition_currency_pos
        self.acquisition_cost_pos = acquisition_cost_pos
        self.geometrical_indexes = geometrical_indexes
        self.merge_prev = merge_prev

        @standard_text_filterion_loop(self.geometrical_indexes, self.merge_prev)
        def text_filter(
            pdf_blocks_table: PdfBlocksTable, i: int | Tuple[int, int]
        ) -> TextBlock:
            def abs_idx(offset: int | Tuple[int, int]) -> int | Tuple[int, int]:
                """Convert relative offset to absolute index in PDF blocks table.

                Parameters
                ----------
                offset : int | Tuple[int, int]
                    Relative offset from current position. Can be:
                    - int: linear offset in flattened table
                    - Tuple[int, int]: (row_offset, column_offset) in 2D table

                Returns
                -------
                int | Tuple[int, int]
                    Absolute index in the table structure
                """
                if isinstance(i, tuple):
                    ro, co = (None, None)
                    r, c = i
                    if isinstance(offset, tuple):
                        ro, co = offset
                    else:
                        nc = pdf_blocks_table.shape[1]
                        co = (c + offset) % nc - c
                        ro = (c + offset) // nc
                    return (r + ro, c + co)
                return i + offset

            def try_extraction_of_field(
                metadata: dict,
                pos: int | Tuple[int, int] | None,
                name: str,
                pdf_blocks_table: PdfBlocksTable,
            ) -> dict:
                """Attempt to extract field content from PDF blocks table.

                Parameters
                ----------
                metadata : dict
                    Metadata dictionary to update
                pos : int | Tuple[int, int] | None
                    Position of the field in the table
                name : str
                    Name of the field to extract
                pdf_blocks_table : PdfBlocksTable
                    Table structure containing PDF blocks

                Returns
                -------
                dict
                    Updated metadata dictionary
                """
                if pos is not None:
                    try:
                        metadata[name] = pdf_blocks_table[abs_idx(pos)].content
                    except (KeyError, AttributeError):
                        row = None
                        col = None
                        if isinstance(abs_idx(pos), tuple):
                            row, col = abs_idx(pos)
                        logger.error(
                            _("Expected field not found, replacing with None..."),
                            extra={"col": col, "row": row, "field": name},
                        )
                        metadata[name] = None
                return metadata

            metadata = {}

            try:
                metadata["manco"] = pdf_blocks_table[i].metadata.get("manco")
            except AttributeError as e:
                logger.error(e)
                debug_msg = ""
                debug_msg += _("Line next to it (on row {}):\n").format(i[0])
                debug_msg += _("Column {}:\n").format(i[1] - 1)
                debug_msg += str(pdf_blocks_table[(i[0], i[1] - 1)])
                debug_msg += _("\nMatching column:\n")
                debug_msg += str(pdf_blocks_table[i])
                debug_msg += _("\nColumn {}:\n").format(i[1] + 1)
                debug_msg += str(pdf_blocks_table[(i[0], i[1] + 1)])
                logger.debug(debug_msg)
                raise ExpectedTextBlockNotFound(
                    _("Matching text block not found")
                ) from e
            try:
                metadata["market value"] = pdf_blocks_table[
                    abs_idx(self.market_value_pos)
                ].content
            except (KeyError, AttributeError) as e:
                logger.error("Field not found", extra={"field": "Market value"})
                logger.debug(_("Current metadata:\n%s"), str(metadata))
                logger.debug(_('Current content: "%s"'), pdf_blocks_table[i].content)
                logger.debug(
                    _("Requested index: %s"), str(abs_idx(self.market_value_pos))
                )
                raise ExpectedTextBlockNotFound from e

            for pos, name in [
                (self.perc_net_assets_pos, "% net assets"),
                (self.nominal_quantity_pos, "quantity"),
                (self.acquisition_currency_pos, "acquisition currency"),
                (self.acquisition_cost_pos, "acquisition cost"),
            ]:
                metadata = try_extraction_of_field(
                    metadata, pos, name, pdf_blocks_table
                )

            content = pdf_blocks_table[i].content.replace("\n", "")
            instrument = ResultStandardFiltering.EQUITY_TARGET
            for reg in perc_regexes:
                interest_rate_match = re.match(reg, content, re.DOTALL)
                if interest_rate_match:
                    instrument = ResultStandardFiltering.BOND_TARGET
                    metadata["interest rate"] = interest_rate_match[1]
                    break
            for reg in date_regexes:
                date_match = re.match(reg, content, re.DOTALL)
                if date_match:
                    instrument = ResultStandardFiltering.BOND_TARGET
                    metadata["maturity"] = date_match[1]
                    break
            # metadata.update(add_metadata(pdf_blocks_table, i))
            return TextBlock(instrument, metadata, pdf_blocks_table[i])

        self.__txt_filter = text_filter

    def __call__(self, pdf_blks, filter_data):
        investments_blks = []
        fund_found = None
        currency_found = None
        results = []
        for b in pdf_blks:
            if b.type_block == ResultStandardExtraction.FUND_NAME:
                if fund_found is not None:
                    raise Exception("Fund two subfunds in same page")
                fund_found = b.content
                results.append(StandardFundTextBlock(b))

            elif b.type_block == ResultStandardExtraction.CURRENCY_STATEMENT:
                if currency_found is not None:
                    raise Exception("Fund two currency in same page")
                try:
                    currency_found = extract_currency_from_text(b.content)
                except ExpectedTextBlockNotFound as e:
                    raise PageParseFail(e) from e
            else:
                investments_blks.append(b)
        inv = self.__txt_filter(investments_blks, filter_data)
        for i in inv:
            i.metadata["fund"] = fund_found
            i.metadata["currency"] = currency_found
        results.extend(inv)
        if len(inv) > 0:
            return results
        else:
            return []


class TextFilterAssetsStandard:
    def __init__(self, date_regex=None, remove_from_fund_regex=None):
        self.date_regex = re.compile(date_regex) if date_regex is not None else None
        self.remove_from_fund_regex = (
            re.compile(remove_from_fund_regex)
            if remove_from_fund_regex is not None
            else None
        )

    def __call__(self, blks, filter_data):

        filter_funds = set(
            map(
                lambda x: match.MatchFund(name=x.name),
                filter(lambda x: isinstance(x, output.Fund), filter_data),
            )
        )
        results = []
        for blk in blks:
            md = {**blk.metadata}
            if self.remove_from_fund_regex is not None:
                md["fund"] = self.remove_from_fund_regex.sub("", md["fund"])
            if match.MatchFund(name=md["fund"]) in filter_funds:
                if self.date_regex is not None:
                    md["date"] = self.date_regex.search(md["date"]).group(1)
                md["currency"] = extract_currency_from_text(md["currency"])
                results.append(
                    TextBlock.from_content(OneTextBlockType.RELEVANT_BLOCK, md, "")
                )
        return results


class StandardManagmentCompanyTextBlock(TextBlock):
    def __init__(self, pdf_blk: PdfBlock, funds: Set[match.MatchFund]):
        super().__init__(
            ResultStandardFiltering.MANAGEMENT_COMPANY,
            {"managed_funds": set((f.name for f in funds))},
            pdf_blk,
        )

    @classmethod
    def from_content(cls, name, funds: Set[match.MatchFund]):
        return super().from_content(
            ResultStandardFiltering.MANAGEMENT_COMPANY,
            {"managed_funds": set((f.name for f in funds))},
            name,
        )

    from_name = from_content


class StandardInvestmentsMangerTextBlock(TextBlock):
    def __init__(self, pdf_blk: PdfBlock, funds: Set[match.MatchFund]):
        super().__init__(
            ResultStandardFiltering.INVESTMENTS_MANAGER,
            {"managed_funds": set((f.name for f in funds))},
            pdf_blk,
        )

    @classmethod
    def from_content(cls, name, funds: Set[match.MatchFund]):
        return super().from_content(
            ResultStandardFiltering.INVESTMENTS_MANAGER,
            {"managed_funds": set((f.name for f in funds))},
            name,
        )

    from_name = from_content


class StandardFundTextBlock(TextBlock):
    def __init__(self, blk):
        super().__init__(ResultStandardFiltering.FUND, {}, blk)

    @classmethod
    def from_matched_fund(cls, fund: match.MatchFund):
        return super().from_content(ResultStandardFiltering.FUND, {}, fund.name)

    @classmethod
    def from_content(cls, fund: str):
        return super().from_content(ResultStandardFiltering.FUND, {}, fund)

    from_name = from_content


class TextFilterManagmentCompanyStandard:
    def __call__(self, pdf_blks, filter_data):
        filter_funds = set(
            map(
                lambda x: match.MatchFund(x.name),
                filter(lambda x: isinstance(x, output.Fund), filter_data),
            )
        )
        manco_block = next(
            filter(
                lambda x: x.type_block == ResultStandardExtraction.MANAGEMENT_COMPANY,
                pdf_blks,
            )
        )
        return [StandardManagmentCompanyTextBlock(manco_block, filter_funds)]
