"""Module for text block processing and extraction in document analysis.

This module provides functionality for:
- Defining text block types through enumerations
- Matching text against targets using various matching strategies
- Extracting text blocks from PDF documents based on target matches
- Supporting different matching methods (exact, fuzzy, prefix-based)

Key components:
- Matching functions (target_match, target_fuzzy_match, target_prefix_match)
- Decorators for text block type definition (one_txt_blk, equity_bond_blks)
- Standard text extraction functionality through standard_text_extraction decorator
"""

from enum import Enum, auto
import re
from typing import List, TypeAlias, Optional
from .match import target_match
from .. import TextBlock, PdfBlock
from ...consts import Currency
from ..utils_commons import normalize_string, overwrite_if_implemented


TextBlockType: TypeAlias = Enum


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


def equity_bond_blks(_: TextBlockType) -> EquityBondTextBlockType:
    """Decorator for overwriting classes definition with two
    text block types one for Equity rows, the other for Bond rows.
    It is helpful when there is only one type of text block

    Returns
    -------
    EquityBondTextBlockType
        The text block type enum.
    """
    return EquityBondTextBlockType


def standard_text_extraction_loop(match_func=target_match):
    def decorator(f):
        def text_extract(
            pdf_blocks: List[PdfBlock], targets: List[str]
        ) -> List[TextBlock]:
            text_part_list = []
            i = 0
            if len(pdf_blocks) == 0:
                return text_part_list
            while True:
                company_name = False
                split = False
                current_block = pdf_blocks[i]
                next_block = pdf_blocks[i + 1]
                col = current_block.metadata["table-col"]
                next_col = next_block.metadata["table-col"]
                content = current_block.content
                if col == next_col:
                    split = True
                    content += pdf_blocks[i + 1].content
                for target in targets:
                    target_n = normalize_string(target)
                    if target_n != "" and match_func(content, target):
                        company_name = True
                        if company_name and split:
                            pdf_blocks[i].content = content
                            pdf_blocks.pop(i + 1)
                        txt_blk = f(pdf_blocks, i)
                        txt_blk.metadata["company"] = target
                        text_part_list.append(txt_blk)
                        break
                i += 1
                if i >= len(pdf_blocks) - 1:
                    break
            if i == len(pdf_blocks) - 1:
                content = pdf_blocks[-1].content
                for target in targets:
                    target_n = normalize_string(target)
                    if target_n != "" and match_func(content, target):
                        txt_blk = f(pdf_blocks, i)
                        txt_blk.metadata["company"] = target
                        text_part_list.append(txt_blk)
            return text_part_list

        return text_extract

    return decorator


date_regexes = [
    r".*(\d{2}[/-]\d{2}[/-]\d{4}).*",
    r".*(\d{4}[/-]\d{2}[/-]\d{2}).*",
    r".*(\d{2}[/-]\d{2}[/-]\d{2}).*",
]
perc_regexes = [r".*((\d+[\.,]\d+)\s*%).*"]


def standard_text_extraction(
    nominal_quantity_pos: int,
    market_value_pos: int,
    perc_net_assets_pos: int,
    currency: Optional[int | Currency] = None,
    acquisition_cost_pos: Optional[int] = None,
    match_func=target_match,
):
    """Decorator for defining standard text extraction
    logic from PDF blocks based on target matches.

    Parameters
    ----------
    extract_positions : dict
        Dictionary specifying metadata keys and
        their relative positions (offsets) from matched blocks
    match_func : callable, optional
        Matching function to compare text against targets (default: target_match)

    Returns
    -------
    callable
        A wrapped text extraction function that processes
        PDF blocks and returns matched TextBlock objects

    Notes
    -----
    The decorated function can optionally be specified
    with the purpose of including additional metadata.
    The extraction process:
    1. Normalizes and matches text against targets using the specified match_func
    2. Extracts metadata from surrounding blocks based on extract_positions
    3. Creates TextBlock objects for successful matches
    """

    def wrapper(f):
        @equity_bond_blks
        class TextBlockType:
            pass

        @overwrite_if_implemented(f)
        def add_metadata(pdf_blks, i):
            return {}

        @standard_text_extraction_loop(match_func)
        def text_extract(pdf_blocks: List[PdfBlock], i: int) -> TextBlock:
            if nominal_quantity_pos * market_value_pos * perc_net_assets_pos == 0:
                raise ValueError("All positions must be non-zero")
            if (
                nominal_quantity_pos == market_value_pos
                or nominal_quantity_pos == perc_net_assets_pos
                or market_value_pos == perc_net_assets_pos
            ):
                raise ValueError("All positions should be different")

            metadata = {}
            metadata["subfund"] = pdf_blocks[i].metadata["subfund"]
            metadata["page"] = pdf_blocks[i].metadata["page"]
            metadata["quantity"] = pdf_blocks[i + nominal_quantity_pos].content
            metadata["market value"] = pdf_blocks[i + market_value_pos].content
            metadata["% net assets"] = pdf_blocks[i + perc_net_assets_pos].content
            if type(currency) is int:
                metadata["currency"] = pdf_blocks[i + currency].content
            elif type(currency) is Currency:
                metadata["currency"] = currency.name

            if acquisition_cost_pos is not None:
                metadata["acquisition cost"] = pdf_blocks[
                    i + acquisition_cost_pos
                ].content

            content = pdf_blocks[i].content
            instrument = TextBlockType.EQUITY_TARGET
            for reg in perc_regexes:
                interest_rate_match = re.match(reg, content, re.DOTALL)
                if interest_rate_match:
                    instrument = TextBlockType.BOND_TARGET
                    metadata["interest rate"] = interest_rate_match[1]
                    break
            for reg in date_regexes:
                date_match = re.match(reg, content, re.DOTALL)
                if date_match:
                    instrument = TextBlockType.BOND_TARGET
                    metadata["maturity"] = date_match[1]
                    break

            metadata.update(add_metadata(pdf_blocks, i))
            return TextBlock(instrument, metadata, pdf_blocks[i])

        return text_extract

    return wrapper
