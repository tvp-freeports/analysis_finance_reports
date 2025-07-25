"""EURIZON_EN23 format submodule"""

import logging as log
from typing import TypeAlias
from freeports_analysis.formats_utils.pdf_filter import (
    OnePdfBlockType,
    standard_pdf_filtering,
    PdfLineSet,
)
from freeports_analysis.formats_utils.text_extract import (
    standard_text_extraction,
    EquityBondTextBlockType,
)
from freeports_analysis.formats_utils.deserialize import standard_deserialization

logger = log.getLogger(__name__)


PdfBlockType: TypeAlias = OnePdfBlockType
TextBlockType: TypeAlias = EquityBondTextBlockType

subfund_set = PdfLineSet(font="Frutiger-Black", area=(65, 85))
header_set = PdfLineSet(
    text="PORTFOLIO AS AT",
    font="Frutiger-Black",
)
body_set = PdfLineSet(font="Frutiger-Light", area=(160, 765))


@standard_pdf_filtering(
    header_set=header_set, subfund_set=subfund_set, body_set=body_set
)
def pdf_filter(xml_root) -> dict:
    raise NotImplementedError


@standard_text_extraction(
    nominal_quantity_pos=-1,
    market_value_pos=+3,
    perc_net_assets_pos=+4,
    currency=+1,
    acquisition_cost_pos=+2,
)
def text_extract(pdf_blocks, targets):
    raise NotImplementedError


@standard_deserialization(cost_and_value_interpret_int=False)
def deserialize(text_block, targets):
    raise NotImplementedError
