"""ANIMA _EN23 format submodule"""

import logging as log
from typing import List, TypeAlias
from freeports_analysis.formats_utils.pdf_filter import (
    OnePdfBlockType,
    standard_pdf_filtering,
)
from freeports_analysis.formats_utils.text_extract import (
    standard_text_extraction,
    EquityBondTextBlockType,
)
from freeports_analysis.formats_utils.deserialize import standard_deserialization
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import (
    YRange,
    PdfLineSet,
    XRange,
    Area,
)
from freeports_analysis.formats_utils.pdf_filter.xml.font import is_present_txt_font
from freeports_analysis.consts import Currency
from .. import PdfBlock


logger = log.getLogger(__name__)


PdfBlockType: TypeAlias = OnePdfBlockType
TextBlockType: TypeAlias = EquityBondTextBlockType


options = {
    "header_set": PdfLineSet(
        "Helvetica-Bold",
        text="Holdings",
    ),
    "subfund_set": PdfLineSet("Helvetica-Condensed-Blac", area=YRange(62, 82)),
    "body_set": PdfLineSet("Helvetica-Light", area=YRange(103, 821)),
    "currency_set": PdfLineSet(
        "Helvetica-Bold", font_size=8.9802, area=Area(XRange(460, 500), YRange(95, 170))
    ),
}


@standard_pdf_filtering(**options)
def _filter_long_pages(xml_root) -> dict:
    raise NotImplementedError


# @standard_pdf_filtering(
#     **options,
#     y_range=(("Holdings", "Helvetica-Bold"), ("Futures contracts", "Helvetica-Bold")),
# )
# def _filter_short_pages(xml_root) -> dict:
#     raise NotImplementedError


def pdf_filter(xml_root) -> List[PdfBlock]:
    # if is_present_txt_font(xml_root, "Futures contracts", "Helvetica-Bold"):
    #     return _filter_short_pages(xml_root)
    return _filter_long_pages(xml_root)


@standard_text_extraction(
    nominal_quantity_pos=-1,
    market_value_pos=+1,
    perc_net_assets_pos=+2,
)
def text_extract(pdf_blocks, targets):
    raise NotImplementedError


@standard_deserialization(True)
def deserialize(pdf_block, targets):
    raise NotImplementedError
