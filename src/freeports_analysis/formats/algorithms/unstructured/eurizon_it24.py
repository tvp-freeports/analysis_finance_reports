"""Custom pdf filter for EURIZON-IT24"""

from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet, TextSet
from freeports_analysis.formats.utils.pdf_filter.pdf_parts.font import FontSet, FontSizeSet
from freeports_analysis.consts import Currency

header_set = [
    PdfLineSet(
    font=FontSet("TrebuchetMSBold","TrebuchetMS,Bold"),
    text="Titolo"),
    PdfLineSet(
        font=FontSet("TrebuchetMSBold","TrebuchetMS,Bold"),
        text="Controvalore in"
    )]

subfund_set = PdfLineSet(
	font=FontSet("TrebuchetMSItalic", "TrebuchetMS,Italic"),
    font_size=FontSizeSet.from_range(4,6.5),
	area=(
		(270,595),
		(700,805)
	)
)

currency_set = Currency.EUR

body_set = PdfLineSet(
	font="TrebuchetMS"
)

deselection_list = [
    PdfLineSet(
        font="TrebuchetMS",
        text="Totale"
    ),
    PdfLineSet(
        font="TrebuchetMS",
        text="Altri strumenti finanziari"
    ),
]

@standard_pdf_filtering(
    header_set=header_set,
    subfund_set=subfund_set,
    currency_set=currency_set,
    body_set=body_set,
    deselection_list=deselection_list
)
def pdf_filter(xml_root):
    """Custom PDF filter that use a relative reference area for the subfund and for the currency"""
    raise NotImplementedError
