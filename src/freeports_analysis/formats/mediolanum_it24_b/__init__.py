"""MEDIOLANUM_IT24_B format submodule"""

from freeports_analysis.formats_utils.pdf_filter import (
    standard_pdf_filtering,
    PdfLineSet,
)
from freeports_analysis.formats_utils.text_extract import standard_text_extraction
from freeports_analysis.formats_utils.deserialize import standard_deserialization
from freeports_analysis.consts import Currency
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import XRange, YRange, Area
from freeports_analysis.formats_utils.pdf_filter.xml.font import get_lines_with_txt_font
from freeports_analysis.formats_utils.pdf_filter.xml.position import get_bounds


def pdf_filter(xml_root):
    next_table = get_lines_with_txt_font(
        xml_root, "Strumenti finanziari quotati", "Helvetica-Bold"
    )
    body_low_limit = None if next_table is None else get_bounds(next_table)[1][1]

    @standard_pdf_filtering(
        header_set=[
            PdfLineSet(font_size=5.9981, text="Titolo", font="Helvetica-Bold"),
            PdfLineSet(font_size=5.9981, text="Controvalore", font="Helvetica-Bold"),
        ],
        subfund_set=PdfLineSet(
            font="Helvetica",
            area=Area(x_range=XRange(150, None), y_range=YRange(67, 76)),
        ),
        body_set=PdfLineSet(
            font="Helvetica",
            area=Area(x_range=XRange(None, None), y_range=YRange(100, body_low_limit)),
        ),
        currency_set=Currency.EUR,
        deselection_list=[PdfLineSet(text="^ ")],
    )
    def _pdf_filter(xml_root):
        raise NotImplementedError

    return _pdf_filter(xml_root)


@standard_text_extraction(
    nominal_quantity_pos=+1, market_value_pos=+2, perc_net_assets_pos=+3
)
def text_extract(pdf_blocks, targets):
    raise NotImplementedError


@standard_deserialization()
def deserialize(text_block, targets):
    raise NotImplementedError
