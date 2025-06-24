from enum import Enum, auto
from typing import List
from difflib import SequenceMatcher
from . import Text_Block, PdfBlock
from .utils_commons import normalize_string


def one_txt_blk(_):
    class Text_BlockType(Enum):
        TARGET = auto()

    return Text_BlockType


def equity_bond_blks(_):
    class Text_BlockType(Enum):
        BOND_TARGET = auto()
        EQUITY_TARGET = auto()

    return Text_BlockType


def prefix_similarity(a, b):
    """
    Compute a similarity ratio from the beginning of the two strings.
    Only matching prefixes are considered.
    """
    # Match from the start only
    min_len = min(len(a), len(b))
    i = 0
    while i < min_len and a[i] == b[i]:
        i += 1
    # Percentage of prefix match relative to the first string
    return i / len(a) if len(a) else 1.0


def target_match(text: str, target: str) -> bool:
    target = normalize_string(target)
    text = normalize_string(text)
    return target in text


def target_fuzzy_match(text: str, target: str, ratio: float) -> bool:
    text = normalize_string(text)
    target = normalize_string(target)
    return SequenceMatcher(None, target, text).ratio() >= ratio


def target_prefix_match(text: str, target: str, ratio: float) -> bool:
    text = normalize_string(text)
    target = normalize_string(target)
    return prefix_similarity(target, text) >= ratio


@one_txt_blk
class Text_BlockType(Enum):
    pass


def standard_text_extraction_func(
    pdf_blocks: List[PdfBlock],
    targets: List[str],
    match_func,
    extract_positions: dict,
    elaborate_func: lambda pdf_blocks, i: (Text_BlockType.TARGET, {}),
) -> List[Text_Block]:
    text_part_list = []
    for i, row_block in enumerate(pdf_blocks):
        for target in targets:
            target_n = normalize_string(target)
            if target_n != "" and match_func(row_block.content, target):
                metadata = dict()
                metadata["match"] = target
                for k, off in extract_positions.items():
                    metadata[k] = pdf_blocks[i + off].content

                type_block, add_metadata = elaborate_func(pdf_blocks, i)
                metadata.update(add_metadata)
                text_part_list.append(Text_Block(type_block, metadata, row_block))

    return text_part_list


def standard_text_extraction(
    match_func,
    extract_positions: dict,
    elaborate_func: lambda pdf_blocks, i: (Text_BlockType.TARGET, {}),
):
    def wrapper(_):
        def pdf_filter(pdf_blocks, targets):
            relevant_blocks = standard_text_extraction_func(
                pdf_blocks, targets, match_func, extract_positions, elaborate_func
            )
            return relevant_blocks

        return pdf_filter

    return wrapper
