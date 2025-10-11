from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.formats.utils.pdf_filter.pdf_parts.font import FontSet

header_set = [
    PdfLineSet.from_str('TimesNewRoman,Bold "Domicile"'),
    PdfLineSet.from_str('TimesNewRoman,Bold "Shares/"'),
]
subfund_set = PdfLineSet.from_str("TimesNewRoman,Bold(112:136)")
currency_set = PdfLineSet(
    font="TimesNewRoman,Bold",
    area=(
        PdfLineSet(font="TimesNewRoman,Bold", text="Fair Value"),
        (0, 1),
        (1.2, 1.2),
    ),
)
body_set = PdfLineSet(
    font=FontSet("TimesNewRoman", "TimesNewRoman,Bold"),
    font_size=10.02,
    area=((150, None), (185, 780)),
) - PdfLineSet(text="-$")


@standard_pdf_filtering(
    header_set=header_set,
    subfund_set=subfund_set,
    currency_set=currency_set,
    body_set=body_set,
)
def pdf_filter(xml_root):
    pass
