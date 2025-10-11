from shapely import box
from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet

header_set = [
    PdfLineSet.from_str('TrebuchetMS-Bold "Titoli $"'),
]
subfund_set = PdfLineSet.from_str("Open Sans((300:)(:48))")
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
    pass
