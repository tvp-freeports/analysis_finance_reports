from enum import Enum, auto
from lxml import etree
from typing import List, Tuple
import re
from rapidfuzz import fuzz
from .. import PDF_Block, Text_Block
import logging as log

logger = log.getLogger(__name__)


def pdf_filter(xml_root: etree.Element) -> List[Tuple[etree.Element, PDF_Block]]:
    parts = []
    for blk in xml_root.findall(".//block"):
        parts.append((blk, PDF_Block(PDF_BlockType.TABLE, {}, "")))
    return parts


def text_extract(pdf_blocks: List[PDF_Block], targets: List[str]) -> List[Text_Block]:
    default_search_min_ratio = 90
    text_part_list = []
    for pdf_p in pdf_blocks:
        row = pdf_p.content
        for target in targets:
            if (
                fuzz.partial_ratio(target.lower(), row.lower())
                >= default_search_min_ratio
            ):
                text_part_list.append(Text_Block(Text_BlockType.TESTO, {}, pdf_p))
    return text_part_list


def tabularize(text_block: Text_Block) -> dict:
    complete_pattern = re.match(
        r"(?P<nominal>[0-9.,]+)\s+(?P<company>.+?)\s*(?P<rate>\d[0-9.,]+%)\s*(?P<maturity>\d{2}/\d{2}/\d{4})\s+(?P<currency>[A-Z]{3})\s+(?P<acquisition>[0-9.,]+)\s+(?P<carrying>[0-9.,]+)\s+(?P<netassets>[0-9.,]+)",
        text_block.content,
    )
    # Pattern senza interest rate e maturity
    reduced_pattern = re.match(
        r"(?P<nominal>[0-9.,]+)\s+(?P<company>.+?)\s+(?P<currency>[A-Z]{3})\s+(?P<acquisition>[0-9.,]+)\s+(?P<carrying>[0-9.,]+)\s+(?P<netassets>[0-9.,]+)",
        text_block.content,
    )
    parsed_row = None
    if complete_pattern:
        parsed_row = {
            "Matched Company": text_block.metadata["company_match"],
            "Nominal Value": complete_pattern.group("nominal"),
            "Company": complete_pattern.group("company"),
            "Interest Rate": complete_pattern.group("rate"),
            "Maturity": complete_pattern.group("maturity"),
            "Currency": complete_pattern.group("currency"),
            "Acquisition Cost": complete_pattern.group("acquisition"),
            "Carrying Value": complete_pattern.group("carrying"),
            "Net Assets": complete_pattern.group("netassets"),
        }
    elif reduced_pattern:
        parsed_row = {
            "Matched Company": text_block.metadata["company_match"],
            "Nominal Value": reduced_pattern.group("nominal"),
            "Company": reduced_pattern.group("company"),
            "Interest Rate": None,
            "Maturity": None,
            "Currency": reduced_pattern.group("currency"),
            "Acquisition Cost": reduced_pattern.group("acquisition"),
            "Carrying Value": reduced_pattern.group("carrying"),
            "Net Assets": reduced_pattern.group("netassets"),
        }
    else:
        logger.warning("Row not recognized")
        parsed_row = {}
    return parsed_row


class PDF_BlockType(Enum):
    TITLE = auto()
    TABLE = auto()


class Text_BlockType(Enum):
    TESTO = auto()
