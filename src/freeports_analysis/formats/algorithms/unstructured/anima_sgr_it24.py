"""Custom pipeline for ANIMA_SGR-IT24"""

from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.formats.utils.pdf_filter.pdf_parts.font import (
    FontSet,
    FontSizeSet,
)

header_set = [
    PdfLineSet(
        font=FontSet('Lato,Bold','TrebuchetMS-Bold','Open Sans,Bold'),
        text="Titoli"
    ),
    PdfLineSet(
        font=FontSet('Lato,Bold','TrebuchetMS-Bold','Open Sans,Bold'),
        text="Divisa"
    ),
]

manco_set = PdfLineSet(
    text="di Gestione del Risparmio",
    font=FontSet("Lato","Open Sans", "Lato-Regular")
    )

subfund_set = PdfLineSet(
    font=FontSet("Lato","Open Sans","Lato-Regular"),
    area={
        "x_min": manco_set,
        "x_max": None,
        "y_min": None,
        "y_max": header_set[0],
    },
)

currency_font = FontSet("Lato,Bold","TrebuchetMS-Bold","Open Sans,Bold")

currency_set = (
    PdfLineSet(font=currency_font, text="Controvalore in ") - PdfLineSet(text="in $")
) | PdfLineSet(
    font=currency_font,
    area=(
        PdfLineSet(font=currency_font, text="Controvalore in "),
        (0, 1),
        (1.2, 1.2),
    ),
)

body_set = PdfLineSet(
    font=FontSet("Lato","TrebuchetMS","Open Sans"),
    font_size=FontSizeSet.from_range(6.8, 7.2),
    area={
        "x_min": None,
        "x_max": None,
        "y_min": PdfLineSet(
            font=FontSet("Lato","TrebuchetMS","Open Sans"),
            text="Elenco analitico",
            font_size=FontSizeSet.from_range(11,13),
        ),
        "y_max": None,
    },
)

@standard_pdf_filtering(
    header_set=header_set,
    subfund_set=subfund_set,
    currency_set=currency_set,
    body_set=body_set,
    manco_set=manco_set,
)
def pdf_filter(xml_root):
    """Pdf filter that takes the subfund and the currency relative to different cells"""
    raise NotImplementedError
