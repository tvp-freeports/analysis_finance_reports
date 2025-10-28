"""Custom pdf filter for FINECO-EN23[IR] format"""

from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.formats.utils.pdf_filter.pdf_parts.font import (
    FontSet,
    TextSet,
    FontSizeSet,
)

header_set = [
    PdfLineSet.from_str('TimesNewRoman,Bold "Domicile"'),
    PdfLineSet.from_str('TimesNewRoman,Bold "Shares/"'),
]
subfund_set = PdfLineSet(
    font="TimesNewRoman,Bold",
    font_size=FontSizeSet.from_range(9.95, 10.03),
    area={
        "x_min": None,
        "x_max": None,
        "y_min": PdfLineSet(
            font="TimesNewRoman,Bold", text="Condensed Schedule of Investments"
        ),
        "y_max": PdfLineSet(font="TimesNewRoman,Bold", text="Domicile"),
    },
)
currency_set = PdfLineSet(
    font="TimesNewRoman,Bold",
    area=(
        PdfLineSet(font="TimesNewRoman,Bold", text="Fair Value"),
        (0, 1),
        (1.2, 1.2),
    ),
)
body_set = (
    PdfLineSet(
        font=FontSet("TimesNewRoman", "TimesNewRoman,Bold"),
        font_size=FontSizeSet.from_range(9.95, 10.03),
        area={
            "x_min": 135,
            "x_max": None,
            "y_min": 185,
            "y_max": PdfLineSet(
                text=TextSet("SWAPS", "FORWARDS", "FUTURES"), font="TimesNewRoman,Bold"
            ),
        },
    )
    - PdfLineSet(text="-$")
) & PdfLineSet(area=(None, 750))


@standard_pdf_filtering(
    header_set=header_set,
    subfund_set=subfund_set,
    currency_set=currency_set,
    body_set=body_set,
)
def pdf_filter(xml_root):
    """A pdf filter that use relative areas and set algebra"""
    raise NotImplementedError
