"""Custom pdf filter for EURIZON-IT24"""

from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSelection
from freeports_analysis.consts import Currency

header_set = [
    PdfLineSelection(font="TrebuchetMSBold", text="Titolo"),
    PdfLineSelection(font="TrebuchetMSBold", text="Controvalore in"),
]

subfund_set = PdfLineSelection(
    font="TrebuchetMSItalic", font_size=(4, 6.5), area=(270, 700, 595, 805)
)

currency_set = Currency.EUR

body_set = PdfLineSelection.font("TrebuchetMS")

deselection_list = [
    PdfLineSelection(font="TrebuchetMS", text="Totale"),
    PdfLineSelection(font="TrebuchetMS", text="Altri strumenti finanziari"),
]


@standard_pdf_filtering(
    header_set=header_set,
    subfund_set=subfund_set,
    currency_set=currency_set,
    body_set=body_set,
    deselection_list=deselection_list,
)
def pdf_filter(xml_root):
    """Custom PDF filter that use a relative reference area for the subfund and for the currency"""
    raise NotImplementedError
