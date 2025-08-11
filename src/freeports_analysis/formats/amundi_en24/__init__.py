"""AMUNDI_EN24 format submodule"""

from typing import TypeAlias
from logging import getLogger
from freeports_analysis.formats_utils.pdf_filter import (
    OnePdfBlockType,
    standard_pdf_filtering,
)
from freeports_analysis.formats_utils.text_extract import (
    standard_text_extraction,
    EquityBondTextBlockType,
)
from freeports_analysis.formats_utils.deserialize import standard_deserialization
from freeports_analysis.formats_utils.pdf_filter.pdf_parts.position import YRange
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import PdfLineSet

logger = getLogger(__name__)

PdfBlockType: TypeAlias = OnePdfBlockType

TextBlockType: TypeAlias = EquityBondTextBlockType


@standard_pdf_filtering(
    header_set=PdfLineSet(
        text="Securities Portfolio as at",
        font="ArialNarrow-BoldItalic",
    ),
    subfund_set=PdfLineSet(
        font="ArialMT",
        area=YRange(None, 27),
    ),
    body_set=PdfLineSet(font="ArialNarrow", area=YRange(None, 768)),
    currency_set=PdfLineSet(font="ArialNarrow", area=YRange(None, 208)),
)
def pdf_filter(xml_root) -> dict:
    raise NotImplementedError


@standard_text_extraction(
    nominal_quantity_pos=+1,
    market_value_pos=-1,
    perc_net_assets_pos=-2,
    geometrical_indexes=False,
)
def text_extract(pdf_blocks, i):
    raise NotImplementedError


@standard_deserialization(True)
def deserialize(*_):
    """Standard deserialization"""
