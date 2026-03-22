"""ANIMA_EN23 format submodule"""

import logging as log
from typing import List, TypeAlias

from freeports_analysis.formats.algorithms.commons import Pipeline

from freeports_analysis.formats.utils.pdf_extract import (
    OnePdfBlockType,
    PdfExtractInvestmentsStandard,
)
from freeports_analysis.formats.utils.text_filter import (
    ResultStandardFiltering,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)

from freeports_analysis.formats.algorithms import PdfBlock


logger = log.getLogger(__name__)


PdfBlockType: TypeAlias = OnePdfBlockType
TextBlockType: TypeAlias = ResultStandardFiltering


options = {
    "subfund_set": PdfLineSelection(
        font="Helvetica-Condensed-Blac", area=(0.0, 62.0, 1e6, 82.0)
    ),
}


def pdf_extract(dict_root) -> List[PdfBlock]:
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
    lines = pdflines_from_pagedict(dict_root)
    fair_value_line = PdfLineSelection(
        font="Helvetica-Bold", text="^Fair Value$"
    ).select(lines)

    if len(fair_value_line) == 0:
        return []
    x0, y0, x1, y1 = fair_value_line[0].bbox
    y_offset = 10
    currency_set = PdfLineSelection(
        font="Helvetica-Bold",
        font_size=(8.9800, 8.9804),
        area=(x0 - 5, y0 + y_offset, x1 + 5, y1 + y_offset + 10),
    )
    tables = PdfLineSelection(font="Helvetica-Bold", area=(0.0, 0.0, 105, 1e6)).select(
        lines
    )
    if len(tables) == 0:
        return []
    if len(tables) == 1:
        area = None
    else:
        if tables[-1].text == "Holdings":
            y0 = tables[-1].bbox[1]
            y1 = 1e6
        else:
            for i, table in enumerate(tables):
                if table.text == "Holdings":
                    y0 = table.bbox[1]
                    y1 = tables[i + 1].bbox[1]
        area = (0.0, y0, 1e6, y1)

    std = PdfExtractInvestmentsStandard(
        **options,
        body_set=PdfLineSelection(font="Helvetica-Light", area=area),
        currency_set=currency_set,
    )

    return std(dict_root)


pipelines = {"investments": Pipeline(pdf_extract=pdf_extract)}
