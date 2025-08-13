"""AMUNDI_IT24 format submodule"""

import logging as log
from typing import TypeAlias
from freeports_analysis.formats_utils.pdf_filter import (
    OnePdfBlockType,
    standard_pdf_filtering,
    TablePosAlgorithm,
)
from freeports_analysis.formats_utils.text_extract import (
    standard_text_extraction,
    EquityBondTextBlockType,
)
from freeports_analysis.formats_utils.deserialize import standard_deserialization
from freeports_analysis.formats_utils.pdf_filter.pdf_parts.position import YRange
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.consts import Currency

logger = log.getLogger(__name__)


PdfBlockType: TypeAlias = OnePdfBlockType
TextBlockType: TypeAlias = EquityBondTextBlockType


@standard_pdf_filtering(
    deselection_list=[PdfLineSet(font="TrebuchetMS", text="^ ")],
    header_set=PdfLineSet(font="TrebuchetMS-Bold", text="Titolo"),
    subfund_set=PdfLineSet(font="Arial-BoldItalicMT", area=YRange(None, 60)),
    body_set=PdfLineSet(font="TrebuchetMS"),
    currency_set=Currency.EUR,
    algorithm_flags=TablePosAlgorithm.RULER_AREA,
)
def pdf_filter(xml_root) -> dict:
    raise NotImplementedError


@standard_text_extraction(
    nominal_quantity_pos=+1,
    market_value_pos=+2,
    perc_net_assets_pos=+5,
)
def text_extract(pdf_blocks, targets):
    raise NotImplementedError


@standard_deserialization(
    quantity_interpret_float=True, cost_and_value_interpret_int=False
)
def deserialize(text_block, targets):
    raise NotImplementedError
