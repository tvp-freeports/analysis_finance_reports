from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.formats.utils.pdf_filter.pdf_parts.font import FontSet

header_set = [
    PdfLineSet.from_str('TrebuchetMS-Bold "Titoli"'),
]
subfund_set = PdfLineSet(
    font=FontSet("Calibri", "Lato-Regular"),
    area={"x_min": None, "x_max": None, "y_min": None, "y_max": 60},
)
currency_set = "EUR"
body_set = PdfLineSet(
    font="TrebuchetMS",
    font_size=6.96,
    area={
        "x_min": None,
        "x_max": None,
        "y_min": PdfLineSet(
            font="Lato-Regular",
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
    pass
