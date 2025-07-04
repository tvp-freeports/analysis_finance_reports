from ..utils_pdf_filter import standard_pdf_filtering
from ..utils_text_extract import standard_text_extraction
from ..utils_deserialize import standard_deserialization


@standard_pdf_filtering(
    header_txt="Transferable securities admitted to an official stock",
    header_font="CenturyGothic-Bold",
    subfund_height=(None, 87),
    subfund_font="CenturyGothic-Bold",
    body_font="CenturyGothic",
    y_range=(None, 810),
)
def pdf_filter(xml_root, page):
    pass


@standard_text_extraction(
    currency=+2,
    nominal_quantity_pos=+1,
    market_value_pos=+4,
    perc_net_assets_pos=+5,
    acquisition_cost_pos=+3,
)
def text_extract(pdf_blks, targets):
    pass


@standard_deserialization(True)
def deserialize(txt_blks, targets):
    pass
