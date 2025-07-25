"""MEDIOLLANUM_IT24 format submodule"""

from freeports_analysis.formats_utils.pdf_filter import (
    standard_pdf_filtering,
    YRange,
    is_present_txt_font,
)
from freeports_analysis.formats_utils.text_extract import standard_text_extraction
from freeports_analysis.formats_utils.deserialize import standard_deserialization


@standard_pdf_filtering(
    header_txt="Descrizione",
    header_font="TT5D22o00",
    subfund_height=YRange(47, 62),
    subfund_font="TT5CC2o00",
    body_font="TT5D42o00",
    y_range=None,
    algorithm_flags=[False, False, False, False],
    tolerance=0,
)
def pdf_filter_1(xml_root) -> dict:
    raise NotImplementedError


# bond
@standard_pdf_filtering(
    header_txt="Descrizione",
    header_font="TT6142o00",
    subfund_height=YRange(47, 62),
    subfund_font="TT60C2o00",
    body_font="TT6162o00",
    y_range=None,
    algorithm_flags=[False, False, False, False],
    tolerance=0,
)
def pdf_filter_2(xml_root) -> dict:
    raise NotImplementedError


def pdf_filter(xml_root) -> dict:
    if is_present_txt_font(xml_root, "Descrizione", "TT5D22o00"):
        return pdf_filter_1(xml_root)
    return pdf_filter_2(xml_root)


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
