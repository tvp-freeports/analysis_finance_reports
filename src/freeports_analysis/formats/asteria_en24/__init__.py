"""ASTERIA_EN24 format submodule"""

from freeports_analysis.formats_utils.pdf_filter import (
    standard_pdf_filtering,
    PdfLineSet,
)
from freeports_analysis.formats_utils.text_extract import standard_text_extraction
from freeports_analysis.formats_utils.deserialize import standard_deserialization


@standard_pdf_filtering(
    header_set=PdfLineSet(
        font="CenturyGothic-Bold",
        text="Transferable securities admitted to an official stock",
    ),
    subfund_set=PdfLineSet("CenturyGothic-Bold", area=(None, 87)),
    body_set=PdfLineSet("CenturyGothic", area=(None, 810)),
    currency_set=PdfLineSet("CenturyGothic-Bold", 8.9802, text="(expressed in"),
)
def pdf_filter(xml_root):
    raise NotImplementedError


@standard_text_extraction(
    acquisition_currency_pos=+2,
    nominal_quantity_pos=+1,
    market_value_pos=+4,
    perc_net_assets_pos=+5,
    acquisition_cost_pos=+3,
)
def text_extract(pdf_blks, targets):
    raise NotImplementedError


@standard_deserialization(True)
def deserialize(txt_blks, targets):
    raise NotImplementedError
