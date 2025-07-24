"""AMUNDI format submodule"""

from typing import TypeAlias
from logging import getLogger
from freeports_analysis.formats_utils.pdf_filter import (
    OnePdfBlockType,
    standard_pdf_filtering,
)
from freeports_analysis.formats_utils.pdf_filter.xml.font import get_lines_with_font
from freeports_analysis.formats_utils.pdf_filter.select_position import select_inside
from freeports_analysis.formats_utils.text_extract import (
    standard_text_extraction,
    EquityBondTextBlockType,
)
from freeports_analysis.formats_utils.deserialize import standard_deserialization
from freeports_analysis.formats_utils.pdf_filter.pdf_parts.position import YRange
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import ExtractedPdfLine
from freeports_analysis.consts import Currency

logger = getLogger(__name__)

PdfBlockType: TypeAlias = OnePdfBlockType

TextBlockType: TypeAlias = EquityBondTextBlockType


@standard_pdf_filtering(
    header_txt="Securities Portfolio as at",
    header_font="ArialNarrow-BoldItalic",
    subfund_height=YRange(None, 27),
    subfund_font="ArialMT",
    body_font="ArialNarrow",
    y_range=(None, 768),
)
def pdf_filter(xml_root) -> dict:
    """Add currency metadata taking it from ceratin area"""
    lines = get_lines_with_font(xml_root, "ArialNarrow")
    lines = [ExtractedPdfLine(line) for line in lines]
    y_range = YRange(None, 208)
    currency = select_inside(lines, y_range)[0].xml_blk.xpath(".//@text")[0]
    return {"currency": currency}


@standard_text_extraction(
    nominal_quantity_pos=+1,
    market_value_pos=-1,
    perc_net_assets_pos=-2,
    currency=Currency.EUR,
)
def text_extract(pdf_blocks, i):
    """Propagate currency metadata from pdf block"""
    return {"currency": pdf_blocks[i].metadata["currency"]}


@standard_deserialization(True)
def deserialize(*_):
    """Standard deserialization"""
