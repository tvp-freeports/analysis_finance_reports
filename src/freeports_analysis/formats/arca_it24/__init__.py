"""ARCA_IT24 format submodule"""

import logging as log
from typing import TypeAlias
from freeports_analysis.formats_utils.pdf_filter import (
    OnePdfBlockType,
    standard_pdf_filtering,
)
from freeports_analysis.formats_utils.text_extract import (
    standard_text_extraction,
    EquityBondTextBlockType,
)
from freeports_analysis.formats_utils.deserialize import standard_deserialization
from freeports_analysis.formats_utils.text_extract.match import (
    target_fuzzy_match,
    target_prefix_match,
)
from freeports_analysis.formats_utils.pdf_filter.pdf_parts.position import YRange
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.consts import Currency

logger = log.getLogger(__name__)


PdfBlockType: TypeAlias = OnePdfBlockType
TextBlockType: TypeAlias = EquityBondTextBlockType


@standard_pdf_filtering(
    header_set=PdfLineSet(
        text="Titoli",
        font="TrebuchetMS-Bold",
    ),
    subfund_set=PdfLineSet(
        font="Calibri",
        area=YRange(None, 42),
    ),
    body_set=PdfLineSet(font="TrebuchetMS", area=YRange(83, None)),
    currency_set=Currency.EUR,
)
def pdf_filter(xml_root) -> dict:
    raise NotImplementedError


@standard_text_extraction(
    nominal_quantity_pos=+3,
    market_value_pos=+2,
    perc_net_assets_pos=+4,
    acquisition_currency_pos=+1,
    match_func=lambda x, y: target_fuzzy_match(x, y, 0.8)
    and target_prefix_match(x, y, 0.3),
)
def text_extract(pdf_blocks, targets):
    raise NotImplementedError


@standard_deserialization()
def deserialize(text_block, targets):
    raise NotImplementedError
