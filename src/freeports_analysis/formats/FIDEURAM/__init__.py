"""AMUNDI2 format submodule"""

from enum import Enum, auto
import logging as log
import datetime as dt
from typing import List
import re
from lxml import etree
from .. import PdfBlock, TextBlock
from ..utils_pdf_filter import one_pdf_blk, standard_pdf_filtering
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


@standard_pdf_filtering(
    header_txt="Titolo",
    header_font="TrebuchetMS-Bold",
    subfund_height=(None, 60),
    subfund_font="Arial-BoldItalicMT",
    body_font="TrebuchetMS",
    y_range=(None, None),
)
def pdf_filter(xml_root, page) -> dict:
    pass


@standard_text_extraction(
    nominal_quantity_pos=+1,
    market_value_pos=+4,
    perc_net_assets_pos=+5,
    currency=Currency.EUR,
    acquisition_cost_pos=None,
)
def text_extract(pdf_blocks, targets):
    pass


@standard_deserialization()
def deserialize(text_block, targets):
    pass
