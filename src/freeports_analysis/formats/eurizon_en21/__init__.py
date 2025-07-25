"""EURIZON_EN21 format submodule"""

from freeports_analysis.formats_utils.pdf_filter import (
    standard_pdf_filtering,
    PdfLineSet,
)
from freeports_analysis.formats_utils.text_extract import standard_text_extraction
from freeports_analysis.formats_utils.deserialize import standard_deserialization


@standard_pdf_filtering(
    header_set=PdfLineSet("ArialMT-Bold", text="Face value/"),
    subfund_set=PdfLineSet("ArialMT", area=(82, 98)),
    body_set=PdfLineSet("Verdana", area=(195, 710)),
)
def pdf_filter(xml_root):
    raise NotImplementedError


@standard_text_extraction(
    nominal_quantity_pos=+1,
    currency=+2,
    acquisition_cost_pos=+3,
    market_value_pos=+4,
    perc_net_assets_pos=+5,
)
def text_extract(pdf_blks, targets):
    raise NotImplementedError


@standard_deserialization(cost_and_value_interpret_int=False)
def deserialize(txt_blks, targets):
    raise NotImplementedError
