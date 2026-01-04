"""Custom pdf filter for ANIMA_SGR-IT24.C"""

from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet

header_set = [
    PdfLineSet.from_str('TrebuchetMS-Bold "Titoli $"'),
]

manco_set = PdfLineSet(
    text="di Gestione del Risparmio", 
    font='Open Sans',
    font_size=7.92
)

subfund_set = PdfLineSet(
    font='Open Sans',
    font_size=7.92,
    area={
        "x_min": manco_set,
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
body_set = PdfLineSet.from_str("TrebuchetMS[6.96]")


@standard_pdf_filtering(
    header_set=header_set,
    subfund_set=subfund_set,
    currency_set=currency_set,
    body_set=body_set,
    manco_set=manco_set,
)
def pdf_filter(xml_root):
    """Custom PDF filter that use a relative reference area for the subfund and for the currency"""
    raise NotImplementedError
