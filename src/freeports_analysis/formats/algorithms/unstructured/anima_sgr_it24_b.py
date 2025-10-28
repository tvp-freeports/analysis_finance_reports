"""Custom pdf filter for ANIMA_SGR-IT24.B"""

from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.formats.utils.pdf_filter.pdf_parts.font import FontSet


header_set = [
    PdfLineSet.from_str('TrebuchetMS-Bold "Titoli $"'),
]
subfund_set = PdfLineSet(
    font=FontSet("Open Sans", "Lato"),
    font_size=7.98,
    area={
        "x_min": PdfLineSet(
            font="Lato", text="di Gestione del Risparmio", font_size=7.98
        ),
        "x_max": None,
        "y_min": None,
        "y_max": PdfLineSet(font="TrebuchetMS-Bold", text="Titoli"),
    },
)
currency_set = (
    PdfLineSet(font="TrebuchetMS-Bold", text="Controvalore in ")
    - PdfLineSet(text="in $")
) | PdfLineSet(
    font="TrebuchetMS-Bold",
    area=(
        PdfLineSet(font="TrebuchetMS-Bold", text="Controvalore in "),
        (0, 1),
        (1.2, 1.2),
    ),
)
body_set = PdfLineSet.from_str("TrebuchetMS[7.02]")


@standard_pdf_filtering(
    header_set=header_set,
    subfund_set=subfund_set,
    currency_set=currency_set,
    body_set=body_set,
)
def pdf_filter(xml_root):
    """Custom PDF filter that use a relative reference area for the subfund and for the currency"""
    raise NotImplementedError
