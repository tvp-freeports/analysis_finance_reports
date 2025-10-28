"""Module for text block processing and extraction in document analysis.

This module provides functionality for:
- Defining text block types through enumerations
- Matching text against targets using various matching strategies
- Extracting text blocks from PDF documents based on target matches
- Supporting different matching methods (exact, fuzzy, prefix-based)

Key components:
- Decorators for text block type definition (one_txt_blk, EquityBondTextBlockType)
- Standard text extraction functionality through standard_text_extraction decorator
"""

from enum import Enum, auto
import re
import logging
from typing import List, Optional, Tuple
from freeports_analysis.i18n import _
from freeports_analysis.logging import LOG_ADAPT_INVESTMENT_INFOS
from freeports_analysis.formats import (
    TextBlock,
    PdfBlock,
    ExpectedTextBlockNotFound,
    LineParseFail,
)
from freeports_analysis.consts import Currency
from .match import match_company
from .. import overwrite_if_implemented


logger = logging.getLogger(__name__)


class EquityBondTextBlockType(Enum):
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


def standard_text_extraction_loop(geometrical_indexes=True, merge_prev=False):
    """Decorator for standard text extraction loop.

    This decorator wraps the function provided in the usual loop that gives a simplified
    and higher level context to the decorated `text_extraction` function.
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
        def text_extract(
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
                company = match_company(content, targets)
                if company is not None:
                    company_name = True
                    if company_name and split:
                        if merge_prev:
                            pdf_blocks_table.merge(i, i + 1)
                        else:
                            pdf_blocks_table.merge(i + 1, i)
                    LOG_ADAPT_INVESTMENT_INFOS.company = company
                    LOG_ADAPT_INVESTMENT_INFOS.company_match = content
                    LOG_ADAPT_INVESTMENT_INFOS.row = row
                    LOG_ADAPT_INVESTMENT_INFOS.col = col
                    try:
                        txt_blk = f(
                            pdf_blocks_table,
                            i if not geometrical_indexes else (row, col),
                        )
                        txt_blk.metadata["company match"] = content
                        txt_blk.metadata["company"] = company
                        text_part_list.append(txt_blk)
                    except ExpectedTextBlockNotFound:
                        LOG_ADAPT_INVESTMENT_INFOS.row = None
                        LOG_ADAPT_INVESTMENT_INFOS.col = None
                        LOG_ADAPT_INVESTMENT_INFOS.field = None
                        logger.warning(_("Skipping line..."))
                i += 1
                if i >= len(pdf_blocks_table) - 1:
                    break
            if i == len(pdf_blocks_table) - 1:
                content = pdf_blocks_table[-1].content
                company = match_company(content, targets)
                if company is not None:
                    try:
                        txt_blk = f(
                            pdf_blocks_table,
                            i if not geometrical_indexes else (row, col),
                        )
                        txt_blk.metadata["company match"] = content
                        txt_blk.metadata["company"] = company
                        text_part_list.append(txt_blk)
                    except ExpectedTextBlockNotFound:
                        LOG_ADAPT_INVESTMENT_INFOS.row = None
                        LOG_ADAPT_INVESTMENT_INFOS.col = None
                        LOG_ADAPT_INVESTMENT_INFOS.field = None
                        logger.warning(_("Skipping line..."))
            return text_part_list

        return text_extract

    return decorator


date_regexes = [
    r".*(\d{2}[/\-.]\d{2}[/\-.]\d{4}).*",
    r".*(\d{4}[/\-.]\d{2}[/\-.]\d{2}).*",
    r".*(\d{2}[/\-.]\d{2}[/\-.]\d{2}).*",
    r".*\s(\d{2}[/\-]\d{2})\s.*",
]
perc_regexes = [r"[a-zA-Z].*((\d+[\.,]\d+)\s*%).*", r"[a-zA-Z].*((\d+[\.,]\d+)\s*).*"]


def standard_text_extraction(
    market_value_pos: int,
    nominal_quantity_pos: Optional[int] = None,
    perc_net_assets_pos: Optional[int] = None,
    acquisition_currency_pos: Optional[int] = None,
    acquisition_cost_pos: Optional[int] = None,
    geometrical_indexes=True,
    merge_prev=False,
):
    """Decorator for defining standard text extraction logic
    from PDF blocks based on target matches.

    Parameters
    ----------
    nominal_quantity_pos : Optional[int], optional
        Relative position for nominal quantity metadata
    market_value_pos : int
        Relative position for market value metadata
    perc_net_assets_pos : Optional[int], optional
        Relative position for percentage of net assets metadata
    acquisition_currency_pos : Optional[Currency], optional
        Either relative position for currency metadata or Currency enum value, by default None
    acquisition_cost_pos : Optional[int], optional
        Relative position for acquisition cost metadata, by default None

    Returns
    -------
    callable
        A wrapped text extraction function that processes PDF blocks
        and returns matched TextBlock objects
    Notes
    -----
    The decorated function can optionally be specified with
    the purpose of including additional metadata.
    The extraction process:
    1. Normalizes and matches text against targets using the specified match_func
    2. Extracts metadata from surrounding blocks based on extract_positions
    3. Creates TextBlock objects for successful matches
    """

    def wrapper(f):
        @overwrite_if_implemented(f)
        def add_metadata(blks: PdfBlocksTable, i: int | Tuple[int, int]) -> dict:
            return {}

        @standard_text_extraction_loop(geometrical_indexes, merge_prev)
        def text_extract(
            pdf_blocks_table: PdfBlocksTable, i: int | Tuple[int, int]
        ) -> TextBlock:
            if nominal_quantity_pos is not None and perc_net_assets_pos is not None:
                if (
                    nominal_quantity_pos == market_value_pos
                    or nominal_quantity_pos == perc_net_assets_pos
                    or market_value_pos == perc_net_assets_pos
                ):
                    raise ValueError(_("All positions should be different"))

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
                metadata["subfund"] = pdf_blocks_table[i].metadata["subfund"]
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
                    abs_idx(market_value_pos)
                ].content
            except (KeyError, AttributeError) as e:
                logger.error("Field not found", extra={"field": "Market value"})
                logger.debug(_("Current metadata:\n%s"), str(metadata))
                logger.debug(_('Current content: "%s"'), pdf_blocks_table[i].content)
                logger.debug(_("Requested index: %s"), str(abs_idx(market_value_pos)))
                raise ExpectedTextBlockNotFound from e

            curr = pdf_blocks_table[i].metadata["currency"]
            if isinstance(curr, Currency):
                metadata["currency"] = curr
            else:
                currency_candidates = re.findall(r"\b[A-Z]{3}\b", curr)
                found = False
                for curr_cand in currency_candidates:
                    try:
                        metadata["currency"] = Currency[curr_cand]
                        found = True
                        break
                    except KeyError:
                        pass
                if not found:
                    curr = curr.upper()
                    for c in Currency.__members__:
                        currency_candidates = re.findall(r"\b" + c + r"\b", curr)
                        for curr_cand in currency_candidates:
                            try:
                                metadata["currency"] = Currency[curr_cand]
                                found = True
                                break
                            except KeyError:
                                pass
                if not found:
                    raise ExpectedTextBlockNotFound(
                        _('Currency not found in string: "%s"'), curr
                    )

            for pos, name in [
                (perc_net_assets_pos, "% net assets"),
                (nominal_quantity_pos, "quantity"),
                (acquisition_currency_pos, "acquisition currency"),
                (acquisition_cost_pos, "acquisition cost"),
            ]:
                metadata = try_extraction_of_field(
                    metadata, pos, name, pdf_blocks_table
                )

            content = pdf_blocks_table[i].content.replace("\n", "")
            instrument = EquityBondTextBlockType.EQUITY_TARGET
            for reg in perc_regexes:
                interest_rate_match = re.match(reg, content, re.DOTALL)
                if interest_rate_match:
                    instrument = EquityBondTextBlockType.BOND_TARGET
                    metadata["interest rate"] = interest_rate_match[1]
                    break
            for reg in date_regexes:
                date_match = re.match(reg, content, re.DOTALL)
                if date_match:
                    instrument = EquityBondTextBlockType.BOND_TARGET
                    metadata["maturity"] = date_match[1]
                    break
            metadata.update(add_metadata(pdf_blocks_table, i))
            return TextBlock(instrument, metadata, pdf_blocks_table[i])

        return text_extract

    return wrapper
