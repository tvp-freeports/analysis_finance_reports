"""Custom pipeline for ANIMA_SGR-IT23.A"""

from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet

header_set = [
    PdfLineSet.from_str('Lato,Bold "Titoli "'),
    PdfLineSet.from_str('Lato,Bold "Divisa "'),
]
subfund_set = PdfLineSet(
    font="Lato",
    font_size=7.92,
    area={
        "x_min": PdfLineSet(
            font="Lato", text="di Gestione del Risparmio", font_size=7.92
        ),
        "x_max": None,
        "y_min": None,
        "y_max": PdfLineSet(font="Lato,Bold", text="Titoli"),
    },
)
currency_set = PdfLineSet.from_str('Lato,Bold "Controvalore in "')
body_set = PdfLineSet(
    font="Lato",
    font_size=6.96,
    area={
        "x_min": None,
        "x_max": None,
        "y_min": PdfLineSet(
            font="Lato",
            text="Elenco analitico dei principali strumenti finanziari detenuti dal Fondo",
            font_size=12,
        ),
        "y_max": None,
    },
)


@standard_pdf_filtering(
    header_set=header_set,
    subfund_set=subfund_set,
    currency_set=currency_set,
    body_set=body_set,
)
def pdf_filter(xml_root):
    """Pdf filter that takes the subfund and the currency relative to different cells"""
    raise NotImplementedError
