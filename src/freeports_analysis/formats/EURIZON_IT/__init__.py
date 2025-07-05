from freeports_analysis.consts import Currency
from ..utils_pdf_filter import standard_pdf_filtering
from ..utils_text_extract import standard_text_extraction
from ..utils_deserialize import standard_deserialization


@standard_pdf_filtering(
    header_txt="Elenco",
    header_font="TrebuchetMS,Bold",
    subfund_height=(793, 803),
    subfund_font="TrebuchetMS,Italic",
    body_font="TrebuchetMS",
    y_range=None,
)
def pdf_filter(xml_root, page) -> dict:
    pass


@standard_text_extraction(
    nominal_quantity_pos=+1,
    market_value_pos=+2,
    perc_net_assets_pos=+4,
    currency=Currency.EUR,
)
def text_extract(pdf_blocks, targets):
    pass


@standard_deserialization()
def deserialize(text_block, targets):
    pass
