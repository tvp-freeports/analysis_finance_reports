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
from typing import List, TypeAlias
from difflib import SequenceMatcher
from . import TextBlock, PdfBlock
from .utils_commons import normalize_string, overwrite_if_implemented


TextBlockType: TypeAlias = Enum


class OneTextBlockType(TextBlockType):
    """Enum representing one type of text blocks in document processing.

    Attributes
    ----------
    TARGET : enum
        Text block containing target information.
    """

    TARGET = auto()


def one_txt_blk(_: TextBlockType) -> OneTextBlockType:
    """Decorator for overwriting classes definition with a unique
    text block type. It is helpful when there is only one type of text block

    Returns
    -------
    OneTextBlockType
        The text block type enum.
    """
    return OneTextBlockType


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


def prefix_similarity(a: str, b: str) -> float:
    """Compute a similarity ratio from the beginning of the two strings.
    Only matching prefixes are considered.

    Parameters
    ----------
    a : str
        first string to compare
    b : str
        second string to compare

    Returns
    -------
    float
        similarity ratio between 0.0 and 1.0
    """
    min_len = min(len(a), len(b))
    i = 0
    while i < min_len and a[i] == b[i]:
        i += 1
    return i / len(a) if len(a) else 1.0


def target_match(text: str, target: str) -> bool:
    """Check if normalized target string is contained within normalized text.

    Parameters
    ----------
    text : str
        The input text to search within
    target : str
        The target string to search for

    Returns
    -------
    bool
        True if target is found in text after normalization, False otherwise
    """
    target = normalize_string(target)
    text = normalize_string(text)
    return target in text


def target_fuzzy_match(text: str, target: str, ratio: float) -> bool:
    """Perform fuzzy string matching between normalized text and target.

    Parameters
    ----------
    text : str
        The input text to compare
    target : str
        The target string to compare against
    ratio : float
        The minimum similarity ratio threshold (0.0 to 1.0)

    Returns
    -------
    bool
        True if similarity ratio meets or exceeds threshold, False otherwise
    """
    text = normalize_string(text)
    target = normalize_string(target)
    return SequenceMatcher(None, target, text).ratio() >= ratio


def target_prefix_match(text: str, target: str, ratio: float) -> bool:
    """Check if the normalized prefix of the target string
    matches the normalized text with a given similarity ratio.

    Parameters
    ----------
    text : str
        The input text to compare against the target prefix
    target : str
        The target string whose prefix is being matched
    ratio : float
        The minimum similarity ratio threshold (0.0 to 1.0) for the prefix match

    Returns
    -------
    bool
        True if the normalized prefix similarity meets or exceeds the threshold, False otherwise
    """
    text = normalize_string(text)
    target = normalize_string(target)
    return prefix_similarity(target, text) >= ratio


def standard_text_extraction(extract_positions: dict, match_func=target_match):
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
        @one_txt_blk
        class TextBlockType:
            pass

        @overwrite_if_implemented(f)
        def add_metadata(pdf_blks, i):
            return {}

        def text_extract(
            pdf_blocks: List[PdfBlock], targets: List[str]
        ) -> List[TextBlock]:
            text_part_list = []
            for i, row_block in enumerate(pdf_blocks):
                for target in targets:
                    target_n = normalize_string(target)
                    if target_n != "" and match_func(row_block.content, target):
                        metadata = {}
                        metadata["match"] = target
                        for k, v in row_block.metadata.items():
                            metadata[k] = v
                        for k, off in extract_positions.items():
                            metadata[k] = pdf_blocks[i + off].content

                        metadata.update(add_metadata(pdf_blocks, i))
                        text_part_list.append(
                            TextBlock(TextBlockType.TARGET, metadata, row_block)
                        )
            return text_part_list

        return text_extract

    return wrapper
