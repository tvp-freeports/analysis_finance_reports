from ..utils_pdf_filter import standard_pdf_filtering
from ..utils_text_extract import standard_text_extraction
from ..utils_deserialize import standard_deserialization


@standard_pdf_filtering(
    header_txt="Face value/",
    header_font="ArialMT-Bold",
    subfund_height=(82, 98),
    subfund_font="ArialMT",
    body_font="Verdana",
    y_range=(195, 710),
)
def pdf_filter(xml_root, page):
    pass


@standard_text_extraction(
    nominal_quantity_pos=+1,
    currency=+2,
    acquisition_cost_pos=+3,
    market_value_pos=+4,
    perc_net_assets_pos=+5,
)
def text_extract(pdf_blks, targets):
    pass


@standard_deserialization(cost_and_value_interpret_int=False)
def deserialize(txt_blks, targets):
    pass
