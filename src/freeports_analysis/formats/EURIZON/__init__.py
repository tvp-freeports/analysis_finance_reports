"""EURIZON format submodule"""

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


logger = log.getLogger(__name__)


@one_pdf_blk
class PdfBlock:
    pass


@equity_bond_blks
class TextBlock:
    pass


@standard_pdf_filtering(
    header_txt="PORTFOLIO AS AT",
    header_font="Frutiger-Black",
    subfund_height=(65, 85),
    subfund_font="Frutiger-Black",
    body_font="Frutiger-Light",
    y_range=(160, 765),
)
def pdf_filter(xml_root, page) -> dict:
    pass


@standard_text_extraction(
    nominal_quantity_pos=-1,
    market_value_pos=+3,
    perc_net_assets_pos=+4,
    currency=+1,
    acquisition_cost_pos=+2,
)
def text_extract(pdf_blocks, targets):
    pass


@standard_deserialization(cost_and_value_interpret_int=False)
def deserialize(text_block, targets):
    pass
