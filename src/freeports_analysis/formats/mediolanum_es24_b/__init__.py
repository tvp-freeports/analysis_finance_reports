"""MEDIOLANUM_ES24_B format submodule"""

from freeports_analysis.formats_utils.pdf_filter import (
    standard_pdf_filtering,
    PdfLineSet,
)
from freeports_analysis.formats_utils.text_extract import standard_text_extraction
from freeports_analysis.formats_utils.deserialize import standard_deserialization


@standard_pdf_filtering(
    header_set=PdfLineSet(font="Helvetica-Bold", text="Cartera Exterior"),
    subfund_set=PdfLineSet(font="Helvetica-Bold", area=(0, 80)),
    body_set=PdfLineSet(font="Helvetica"),
    currency_set=PdfLineSet(font="Helvetica", text="Expresado en"),
    deselection_list=[PdfLineSet(text="Cartera de inversiones financieras a")],
)
def pdf_filter(xml_root):
    raise NotImplementedError


@standard_text_extraction(
    market_value_pos=+4,
    acquisition_currency_pos=+1,
    acquisition_cost_pos=+2,
)
def text_extract(pdf_blocks, targets):
    raise NotImplementedError


@standard_deserialization()
def deserialize(text_block, targets):
    raise NotImplementedError
