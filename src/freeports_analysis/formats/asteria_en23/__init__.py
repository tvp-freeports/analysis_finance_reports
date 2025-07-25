"""ASTERIA_EN23 format submodule"""

from freeports_analysis.formats_utils.pdf_filter import (
    standard_pdf_filtering,
    PdfLineSet,
)
from freeports_analysis.formats_utils.text_extract import standard_text_extraction
from freeports_analysis.formats_utils.deserialize import standard_deserialization


@standard_pdf_filtering(
    header_set=PdfLineSet("CenturyGothic-Bold", text="Number of Shares/"),
    subfund_set=PdfLineSet("CenturyGothic-Bold", area=(80, 95)),
    body_set=PdfLineSet("CenturyGothic"),
)
def pdf_filter(xml_root) -> dict:
    raise NotImplementedError


@standard_text_extraction(
    nominal_quantity_pos=+1,
    market_value_pos=+4,
    perc_net_assets_pos=+5,
    currency=+2,
    acquisition_cost_pos=+3,
)
def text_extract(pdf_blocks, targets):
    raise NotImplementedError


@standard_deserialization()
def deserialize(text_block, targets):
    raise NotImplementedError
