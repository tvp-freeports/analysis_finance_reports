"""Module for text block processing and extraction in document analysis.

This module provides functionality for:
- Defining text block types through enumerations
- Matching text against targets using various matching strategies
- Extracting text blocks from PDF documents based on target matches
- Supporting different matching methods (exact, fuzzy, prefix-based)

Key components:
- Matching functions (target_match, target_fuzzy_match, target_prefix_match)
- Decorators for text block type definition (one_txt_blk, EquityBondTextBlockType)
- Standard text extraction functionality through standard_text_extraction decorator
"""

from enum import Enum, auto
import re
import logging
from typing import List, Optional, Tuple
from freeports_analysis.i18n import _
from freeports_analysis.formats import TextBlock, PdfBlock
from .match import target_match
from .. import normalize_string, overwrite_if_implemented
from freeports_analysis.consts import Currency

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
    def _get_table(self, pdf_blocks):
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
                if col > col_max:
                    col_max = col
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
        self._blks = pdf_blocks.copy()
        self._table_indexes, self._table = self._get_table(self._blks)

    @property
    def _rows(self):
        return len(self._table)

    @property
    def _cols(self):
        return max(map(len, self._table)) if self._rows > 0 else 0

    def __getitem__(self, i):
        if isinstance(i, tuple):
            j, k = i
            vals = self._table[j][k]
            if len(vals) == 1:
                return vals[0]
            elif len(vals) == 0:
                return None
            else:
                return vals
        else:
            return self._blks[i]

    def __len__(self):
        return len(self._blks)

    @property
    def shape(self):
        return (self._rows, self._cols)

    def pop(self, j):
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
        first, last = (i, j) if i < j else (j, i)
        content = self._blks[first].content + self._blks[last].content
        self._blks[i].content = content
        col = self._blks[i].metadata["table-col"]
        row = self._blks[i].metadata["table-row"]
        for idx, idx_blk in enumerate(self._table_indexes[row][col]):
            if idx_blk == i:
                self._table[row][col][idx].content = content
        self.pop(j)


def standard_text_extraction_loop(
    geometrical_indexes=True, merge_prev=False, match_func=target_match
):
    """Decorator for standard text extraction loop.

    This decorator wrap the function provide in the usual loop that give a simplify
    and higher level context to the decorated `text_extraction` function.
    Specifically it expect that in the metadata of each `PdfBlock` is present
    an indicator of which column it is located graphycally in the main table of the
    pdf page (it suppose that the data was tabular in some way) `table-col`.
    The loop:
    - Take each block and concat the content with the subsequent if
      they are on the same column.
    - Use `match_func` to see if one between the target provided to the
      extraction function match with the content  of the block.
    - If it does it overwrite the list of `PdfBlock` to persist the concatenation
      of the block with is subsequent.
    - Add `company` metadata with the match
    - It create a `TextBlock` addint the metadata provided by the wrapped function.
      The wrapped function take as parameters the block list and the index
      of the matched block. It takes the modified list with merged content
      for block in the same column that matches the target.
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
            while True:
                company_name = False
                split = False
                current_block = pdf_blocks_table[i]
                next_block = pdf_blocks_table[i + 1]
                col = current_block.metadata["table-col"]
                row = current_block.metadata["table-row"]
                next_col = next_block.metadata["table-col"]
                cell_width = current_block.metadata["is-max-width"]

                content = current_block.content
                if col == next_col:
                    split = True
                    if cell_width or (len(content) > 0 and " " == content[-1]):
                        content += next_block.content

                for target in targets:
                    target_n = normalize_string(target)
                    if target_n != "" and match_func(content, target):
                        company_name = True
                        if company_name and split:
                            if merge_prev:
                                pdf_blocks_table.merge(i, i + 1)
                            else:
                                pdf_blocks_table.merge(i + 1, i)
                        txt_blk = f(
                            pdf_blocks_table,
                            i if not geometrical_indexes else (row, col),
                        )
                        if txt_blk is not None:
                            txt_blk.metadata["company match"] = content
                            txt_blk.metadata["company"] = target
                        text_part_list.append(txt_blk)
                        break
                i += 1
                if i >= len(pdf_blocks_table) - 1:
                    break
            if i == len(pdf_blocks_table) - 1:
                content = pdf_blocks_table[-1].content
                for target in targets:
                    target_n = normalize_string(target)
                    if target_n != "" and match_func(content, target):
                        txt_blk = f(
                            pdf_blocks_table,
                            i if not geometrical_indexes else (row, col),
                        )
                        txt_blk.metadata["company match"] = content
                        txt_blk.metadata["company"] = target
                        text_part_list.append(txt_blk)
            return text_part_list

        return text_extract

    return decorator


date_regexes = [
    r".*(\d{2}[/-]\d{2}[/-]\d{4}).*",
    r".*(\d{4}[/-]\d{2}[/-]\d{2}).*",
    r".*(\d{2}[/-]\d{2}[/-]\d{2}).*",
    r".*\s(\d{2}[/-]\d{2})\s.*",
]
perc_regexes = [r".*((\d+[\.,]\d+)\s*%).*", r".*((\d+[\.,]\d+)\s*).*"]


def standard_text_extraction(
    market_value_pos: int,
    nominal_quantity_pos: Optional[int] = None,
    perc_net_assets_pos: Optional[int] = None,
    acquisition_currency_pos: Optional[int] = None,
    acquisition_cost_pos: Optional[int] = None,
    geometrical_indexes=True,
    merge_prev=False,
    match_func=target_match,
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
    match_func : callable, optional
        Matching function to compare text against targets, by default target_match

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

        @standard_text_extraction_loop(geometrical_indexes, merge_prev, match_func)
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

            def abs_idx(offset):
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

            metadata = {}
            try:
                metadata["subfund"] = pdf_blocks_table[i].metadata["subfund"]
                metadata["page"] = pdf_blocks_table[i].metadata["page"]
                metadata["market value"] = pdf_blocks_table[
                    abs_idx(market_value_pos)
                ].content

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
                            re.findall(r"\b" + c + r"\b", curr)
                            for curr_cand in currency_candidates:
                                try:
                                    metadata["currency"] = Currency[curr_cand]
                                    break
                                except KeyError:
                                    pass

                if perc_net_assets_pos is not None:
                    metadata["% net assets"] = pdf_blocks_table[
                        abs_idx(perc_net_assets_pos)
                    ].content

                if nominal_quantity_pos is not None:
                    metadata["quantity"] = pdf_blocks_table[
                        abs_idx(nominal_quantity_pos)
                    ].content

                if acquisition_currency_pos is not None:
                    metadata["acquisition currency"] = pdf_blocks_table[
                        abs_idx(acquisition_currency_pos)
                    ].content

                if acquisition_cost_pos is not None:
                    metadata["acquisition cost"] = pdf_blocks_table[
                        abs_idx(acquisition_cost_pos)
                    ].content

            except AttributeError as e:
                logger.exception(str(e))
                return None
            except IndexError as e:
                logger.exception(str(e))
                return None
            except Exception as e:
                if isinstance(pdf_blocks_table[i], PdfBlock):
                    logger.error(_("Block:"))
                    logger.exception(pdf_blocks_table[i])
                elif len(pdf_blocks_table[i]) > 0:
                    logger.error(_("First block:"))
                    logger.exception(pdf_blocks_table[i][0])
                raise e

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
