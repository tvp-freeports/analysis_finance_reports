from enum import Enum, auto
from lxml import etree
from typing import List, Tuple
import re
from rapidfuzz import fuzz
from .. import PdfBlock, TextBlock
from ..utils_pdf_filter import one_pdf_blk, standard_header_font_filter
from ..utils_text_extract import one_txt_blk, standard_text_extraction
import logging as log

logger = log.getLogger(__name__)


@standard_header_font_filter("Holdings", "Helvetica-Bold", "Helvetica-Light")
def pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
    pass


@standard_text_extraction({"holdings": -1, "fair value": +1, "% net asset": +2})
def text_extract(pdf_blocks: List[PdfBlock], index: int) -> dict:
    pass


def tabularize(TextBlock: TextBlock) -> dict:
    holdings_regex = r"^\d{1,3}(?:,\d{3})*$"
    net_assets_regex = r"^\d{1,2}(?:\.\d+)?$"
    fair_value_regex = r"^\d{1,3}(?:,\d{3})*$"

    holdings_match = re.match(
        holdings_regex, "".join(TextBlock.metadata["holdings"].split())
    )
    net_assets_match = re.match(
        net_assets_regex, "".join(TextBlock.metadata["% net asset"].split())
    )
    fair_value_match = re.match(
        fair_value_regex, "".join(TextBlock.metadata["fair value"].split())
    )
    parsed_row = None
    if holdings_match and net_assets_match and fair_value_match:
        parsed_row = {
            "holdings": int(holdings_match[0].replace(",", "")),
            "company": TextBlock.metadata["match"],
            "fair value": int(fair_value_match[0].replace(",", "")),
            "% net asset": float(net_assets_match[0]),
        }
    else:
        logger.warning(
            "Row not recognized: %s %s", TextBlock.metadata, TextBlock.content
        )
        parsed_row = {}
    return parsed_row


@one_pdf_blk
class PdfBlockType(Enum):
    pass


@one_txt_blk
class TextBlockType(Enum):
    pass
