"""EURIZON_IT24 format submodule"""

from freeports_analysis.consts import Currency
from freeports_analysis.formats_utils.pdf_filter import (
    standard_pdf_filtering,
    PdfLineSet,
)
from freeports_analysis.formats_utils.text_extract import standard_text_extraction
from freeports_analysis.formats_utils.deserialize import standard_deserialization


@standard_pdf_filtering(
    header_set=PdfLineSet("TrebuchetMS,Bold", text="Elenco"),
    subfund_set=PdfLineSet("TrebuchetMS,Italic", area=(793, 803)),
    body_set=PdfLineSet("TrebuchetMS"),
    currency_set=Currency.EUR,
)
def pdf_filter(xml_root) -> dict:
    raise NotImplementedError


@standard_text_extraction(
    nominal_quantity_pos=+1,
    market_value_pos=+2,
    perc_net_assets_pos=+4,
    geometrical_indexes=False,
)
def text_extract(pdf_blocks, targets):
    raise NotImplementedError


@standard_deserialization()
def deserialize(text_block, targets):
    raise NotImplementedError
