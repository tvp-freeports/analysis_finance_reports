"""CANE-EN23 custom functions"""

from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import (
    pdfline_selection_from_str,
    PdfLineSelection,
)

header_set = [
    pdfline_selection_from_str('Arial-BoldMT "Description"'),
    pdfline_selection_from_str('Arial-BoldMT "Currency"'),
]
subfund_set = PdfLineSelection(
    font="ArialMT", font_size=(6.95, 6.97)
) & PdfLineSelection.area_from_movewindow(
    target=pdfline_selection_from_str('ArialMT[6.96] "^Annual report including"'),
    vec=(0, 0.8),
    width_mult=1.1,
    height_mult=1.4,
)

currency_set = pdfline_selection_from_str('Arial-BoldMT "Valuation in"')
body_set = pdfline_selection_from_str("ArialMT[6.96](160:786)")


@standard_pdf_filtering(
    header_set=header_set,
    subfund_set=subfund_set,
    currency_set=currency_set,
    body_set=body_set,
)
def pdf_filter(xml_root):
    """Custom PDF filter

    It extract the subfund selecting an area relative to the \"Annual report\" string
    """
    raise NotImplementedError
