"""Custom pipeline for ANIMA_SGR-IT24"""

from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSelection


h_font_selection = (
    PdfLineSelection.font("Lato,Bold")
    | PdfLineSelection.font("TrebuchetMS-Bold")
    | PdfLineSelection.font("Open Sans,Bold")
)
s_font_selection = (
    PdfLineSelection.font("Lato")
    | PdfLineSelection.font("Open Sans")
    | PdfLineSelection.font("Lato-Regular")
)

header_set = [
    PdfLineSelection.text("Titoli") & h_font_selection,
    PdfLineSelection.text("Divisa") & h_font_selection,
]

manco_set = PdfLineSelection.text("di Gestione del Risparmio") & s_font_selection

subfund_set = (
    PdfLineSelection.area_from_bounds(x0=manco_set, y1=header_set[0]) & s_font_selection
)


currency_font = (
    PdfLineSelection.font("Lato,Bold")
    | PdfLineSelection.font("TrebuchetMS-Bold")
    | PdfLineSelection.font("Open Sans,Bold")
)

currency_set = (
    PdfLineSelection(text="Controvalore in ")
    & currency_font - PdfLineSelection(text="in $")
) | (
    PdfLineSelection.area_from_movewindow(
        PdfLineSelection(text="Controvalore in ") & currency_font,
        vec=(0.0, 1.0),
        width_mult=1.2,
        height_mult=1.2,
    )
    & currency_font
)

b_font_selection = (
    PdfLineSelection.font("Lato")
    | PdfLineSelection.font("TrebuchetMS")
    | PdfLineSelection.font("Open Sans")
)

body_set = (
    PdfLineSelection.area_from_bounds(
        y0=PdfLineSelection(text="Elenco analitico", font_size=(11, 13))
        & b_font_selection
    )
    & PdfLineSelection.font_size(6.8, 7.2)
    & b_font_selection
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
