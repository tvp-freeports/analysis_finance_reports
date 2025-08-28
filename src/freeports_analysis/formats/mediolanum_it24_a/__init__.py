"""MEDIOLANUM_IT24_A format submodule"""

from freeports_analysis.formats_utils.pdf_filter import (
    standard_pdf_filtering,
    PdfLineSet,
)
from freeports_analysis.formats_utils.text_extract import standard_text_extraction
from freeports_analysis.formats_utils.deserialize import standard_deserialization


@standard_pdf_filtering(
    header_set=PdfLineSet(font_size=8.037657, text="Descrizione"),
    subfund_set=PdfLineSet(font_size=15.47555, area=(0, 80)),
    body_set=PdfLineSet(font_size=8.037657),
    currency_set=PdfLineSet(font_size=8.037657, text="valori espressi in"),
    deselection_list=[
        PdfLineSet(text="TABELLA DEGLI INVESTIMENTI AL"),
        PdfLineSet(text="^ "),
    ],
)
def pdf_filter(xml_root):
    raise NotImplementedError


@standard_text_extraction(
    nominal_quantity_pos=+1,
    market_value_pos=+4,
    perc_net_assets_pos=+5,
    acquisition_currency_pos=+2,
    acquisition_cost_pos=+3,
)
def text_extract(pdf_blocks, targets):
    raise NotImplementedError


@standard_deserialization()
def deserialize(text_block, targets):
    raise NotImplementedError
