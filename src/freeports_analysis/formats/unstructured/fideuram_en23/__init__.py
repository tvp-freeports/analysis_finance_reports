"""FIDEURAM_EN23 format submodule"""

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
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import (
    PdfLineSet,
    YRange,
    Area,
    XRange,
)

logger = log.getLogger(__name__)


PdfBlockType: TypeAlias = OnePdfBlockType
TextBlockType: TypeAlias = EquityBondTextBlockType


@standard_pdf_filtering(
    header_set=PdfLineSet("Arial", text="Country"),
    subfund_set=PdfLineSet(font="Arial-Bold", area=YRange(None, 82)),
    body_set=PdfLineSet(font="Arial", area=YRange(103, 749)),
    deselection_list=[
        PdfLineSet(text="SHARES, WARRANTS, RIGHTS", font="Arial"),
        PdfLineSet(
            text="TRANSFERABLE SECURITIES AND MONEY MARKET INSTRUMENTS ADMITTED TO AN OFFICIAL",
            font="Arial",
        ),
        PdfLineSet(
            text="EXCHANGE LISTING OR DEALT IN ON OTHER REGULATED MARKETS", font="Arial"
        ),
        PdfLineSet(text="BONDS AND ASSIMILATED STRUCTURED PRODUCTS", font="Arial"),
        PdfLineSet(text="INVESTMENT FUNDS", font="Arial"),
    ],
    currency_set=PdfLineSet(
        font="Arial", font_size=6.9846, area=Area(XRange(480, None), YRange(148, 155.5))
    ),
)
def pdf_filter(xml_root) -> dict:
    raise NotImplementedError


@standard_text_extraction(
    nominal_quantity_pos=-1,
    market_value_pos=+1,
    perc_net_assets_pos=+2,
    acquisition_currency_pos=-2,
)
def text_extract(pdf_blocks, targets):
    raise NotImplementedError


@standard_deserialization()
def deserialize(text_block, targets):
    raise NotImplementedError
