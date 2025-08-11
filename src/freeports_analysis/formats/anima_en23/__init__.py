"""ANIMA _EN23 format submodule"""

import logging as log
from typing import List, TypeAlias
from freeports_analysis.formats_utils.pdf_filter import (
    OnePdfBlockType,
    standard_pdf_filtering,
)
from freeports_analysis.formats_utils.text_extract import (
    standard_text_extraction,
    EquityBondTextBlockType,
)
from freeports_analysis.formats_utils.deserialize import standard_deserialization
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import (
    YRange,
    PdfLineSet,
    XRange,
    Area,
    ExtractedPdfLine,
)
from freeports_analysis.formats_utils.pdf_filter.xml.font import (
    get_lines_with_txt_font,
    get_lines_with_font,
)
from freeports_analysis.formats_utils.pdf_filter.xml.position import get_bounds
from .. import PdfBlock


logger = log.getLogger(__name__)


PdfBlockType: TypeAlias = OnePdfBlockType
TextBlockType: TypeAlias = EquityBondTextBlockType


options = {
    "header_set": PdfLineSet(
        "Helvetica-Bold",
        text="Holdings",
    ),
    "subfund_set": PdfLineSet("Helvetica-Condensed-Blac", area=YRange(62, 82)),
}


def pdf_filter(xml_root) -> List[PdfBlock]:
    fair_value_line = get_lines_with_txt_font(
        xml_root, "Fair Value", "Helvetica-Bold", exact_match=True
    )
    if fair_value_line is None:
        return []
    ((x0, x1), (y0, y1)) = get_bounds(fair_value_line)
    y_offset = 10
    w_enlarge = 10
    h_enlarge = 10
    currency_set = PdfLineSet(
        "Helvetica-Bold",
        font_size=8.9802,
        area=Area(
            XRange(x0 - w_enlarge / 2, x1 + w_enlarge / 2),
            YRange(y0 + y_offset, y1 + y_offset + h_enlarge),
        ),
    )
    skeleton = get_lines_with_font(xml_root, "Helvetica-Bold")
    skeleton_lines = [ExtractedPdfLine(line) for line in skeleton]
    tables = [
        line for line in skeleton_lines if line in PdfLineSet(area=XRange(None, 105))
    ]
    if len(tables) == 0:
        return []
    elif len(tables) == 1:
        area = None
    else:
        if tables[-1].text == "Holdings":
            y0 = tables[-1].geometry.y_bounds.y0
            y1 = None
        else:
            for i, table in enumerate(tables):
                if table.text == "Holdings":
                    y0 = table.geometry.y_bounds.y0
                    y1 = tables[i + 1].geometry.y_bounds.y0
        area = YRange(y0, y1)
    body_set = PdfLineSet("Helvetica-Light", area=area)

    @standard_pdf_filtering(**options, body_set=body_set, currency_set=currency_set)
    def filter_page(xml_root):
        raise NotImplementedError

    return filter_page(xml_root)


@standard_text_extraction(
    nominal_quantity_pos=-1,
    market_value_pos=+1,
    perc_net_assets_pos=+2,
    geometrical_indexes=False,
)
def text_extract(pdf_blocks, targets):
    raise NotImplementedError


@standard_deserialization(True)
def deserialize(pdf_block, targets):
    raise NotImplementedError
