"""MEDIOLANUM format submodule"""

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
    header_txt="Relazione di gestione al 30 dicembre 2024",
    header_font="Helvetica",
    subfund_height=(None, 76),
    subfund_font="Helvetica",
    body_font="Helvetica",
    y_range=(83, None),
)
def pdf_filter(xml_root, page) -> dict:
    pass


@standard_text_extraction(
    nominal_quantity_pos=+1,
    market_value_pos=+2,
    perc_net_assets_pos=+3,
    currency=Currency.EUR,
    acquisition_cost_pos=None,
)
def text_extract(pdf_blocks, targets):
    pass


@standard_deserialization()
def deserialize(text_block, targets):
    pass
