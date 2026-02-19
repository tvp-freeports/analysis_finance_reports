"""Custom pdf filter for ARCA-IT24 format"""

from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import (
    pdfline_selection_from_str,
    PdfLineSelection,
)
from freeports_analysis.formats.utils.pdf_filter.pdf_parts.font import FontSet

header_set = [
    pdfline_selection_from_str('TrebuchetMS-Bold "Titoli"'),
    pdfline_selection_from_str('TrebuchetMS-Bold "Divisa"'),
]

subfund_set = PdfLineSelection.area(0.0, 0.0, 1e6, 60.0) & (
    PdfLineSelection.font("Calibri") | PdfLineSelection.font("Lato-Regular")
)


body_set = PdfLineSelection(
    font="TrebuchetMS", font_size=(6.95, 6.97)
) & PdfLineSelection.area_from_bounds(
    x0=0.0,
    y0=PdfLineSelection(
        font="Lato-Regular",
        text="Elenco analitico dei principali strumenti finanziari detenuti dal Fondo",
        font_size=(11.9, 12.1),
    ),
    x1=1e6,
    y1=1e6,
)


@standard_pdf_filtering(
    header_set=header_set,
    subfund_set=subfund_set,
    currency_set="EUR",
    body_set=body_set,
)
def pdf_filter(xml_root):
    """A pdf filter that set constant currency to EUR and takes
    the area of the body relative to another cell
    """
    raise NotImplementedError
