"""ANIMA_EN23 format submodule"""

import logging as log
from typing import List, TypeAlias
from shapely import box
from freeports_analysis.formats.utils.pdf_filter import (
    OnePdfBlockType,
    standard_pdf_filtering,
)
from freeports_analysis.formats.utils.text_extract import (
    EquityBondTextBlockType,
)
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import (
    PdfLineSet,
    ExtractedPdfLine,
)
from freeports_analysis.formats.utils.pdf_filter.xml.font import (
    get_lines_with_txt_font,
    get_lines_with_font,
)
from freeports_analysis.formats.utils.pdf_filter.xml.position import get_bounds
from freeports_analysis.formats.algorithms import PdfBlock


logger = log.getLogger(__name__)


PdfBlockType: TypeAlias = OnePdfBlockType
TextBlockType: TypeAlias = EquityBondTextBlockType


options = {
    "header_set": PdfLineSet(
        "Helvetica-Bold",
        text="Holdings",
    ),
    "subfund_set": PdfLineSet("Helvetica-Condensed-Blac", area=(62, 82)),
}


def pdf_filter(xml_root) -> List[PdfBlock]:
    """PDF filter for ANIMA_EN23 format with dynamic table bounds calculation.

    This PDF filter dynamically calculates the bounds of the table by
    using the position of "Fair Value" text as a reference point.

    Parameters
    ----------
    xml_root : etree.Element
        XML root element of the PDF page

    Returns
    -------
    List[PdfBlock]
        List of PDF blocks extracted from the page

    Notes
    -----
    The filter:
    - Locates "Fair Value" text to determine table position
    - Dynamically calculates currency set bounds
    - Identifies table areas based on font patterns
    - Uses standard PDF filtering with calculated parameters
    """
    fair_value_line = get_lines_with_txt_font(
        xml_root, "Fair Value", "Helvetica-Bold", exact_match=True
    )
    if fair_value_line is None:
        return []
    ((x0, x1), (y0, y1)) = get_bounds(fair_value_line)
    y_offset = 10
    currency_set = PdfLineSet(
        "Helvetica-Bold",
        font_size=8.9802,
        area=box(x0 - 5, y0 + y_offset, x1 + 5, y1 + y_offset + 10),
    )
    skeleton = get_lines_with_font(xml_root, "Helvetica-Bold")
    skeleton = [ExtractedPdfLine(line) for line in skeleton]
    tables = [
        line for line in skeleton if line in PdfLineSet(area=box(-1e6, -1e6, 105, 1e6))
    ]
    if len(tables) == 0:
        return []
    if len(tables) == 1:
        area = None
    else:
        if tables[-1].text == "Holdings":
            y0 = tables[-1].area.bounds[1]
            y1 = -1e6
        else:
            for i, table in enumerate(tables):
                if table.text == "Holdings":
                    y0 = table.area.bounds[1]
                    y1 = tables[i + 1].area.bounds[1]
        area = box(-1e6, y0, 1e6, y1)

    @standard_pdf_filtering(
        **options,
        body_set=PdfLineSet("Helvetica-Light", area=area),
        currency_set=currency_set,
    )
    def filter_page(xml_root):
        raise NotImplementedError

    return filter_page(xml_root)
