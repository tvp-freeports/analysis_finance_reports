"""ANIMA format submodule"""

import logging as log
from typing import List
import re
from enum import Enum
from lxml import etree
from .. import PdfBlock, TextBlock
from ..utils_pdf_filter import one_pdf_blk, standard_pdf_filtering, is_present_txt_font
from ..utils_text_extract import standard_text_extraction, equity_bond_blks
from ..utils_deserialize import standard_deserialization
from freeports_analysis.consts import Currency


logger = log.getLogger(__name__)


@one_pdf_blk
class PdfBlock:
    pass


@equity_bond_blks
class TextBlock:
    pass


options = {
    "header_txt": "Holdings",
    "header_font": "Helvetica-Bold",
    "subfund_height": (62, 82),
    "subfund_font": "Helvetica-Condensed-Blac",
    "body_font": "Helvetica-Light",
}


@standard_pdf_filtering(**options, y_range=(103, 821))
def _filter_long_pages(xml_root, page_number) -> dict:
    pass


@standard_pdf_filtering(
    **options,
    y_range=(("Holdings", "Helvetica-Bold"), ("Futures contracts", "Helvetica-Bold")),
)
def _filter_short_pages(xml_root, page_number) -> dict:
    pass


def pdf_filter(xml_root, page_number) -> List[PdfBlock]:
    if is_present_txt_font(xml_root, "Futures contracts", "Helvetica-Bold"):
        return _filter_short_pages(xml_root, page_number)
    else:
        return _filter_long_pages(xml_root, page_number)


@standard_text_extraction(
    nominal_quantity_pos=-1,
    market_value_pos=+1,
    perc_net_assets_pos=+2,
    currency=Currency.EUR,
)
def text_extract(pdf_blocks, targets):
    pass


@standard_deserialization(True)
def deserialize(pdf_block, targets):
    pass
