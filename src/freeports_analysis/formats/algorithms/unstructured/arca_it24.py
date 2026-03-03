"""Custom pdf filter for ARCA-IT24 format"""

from freeports_analysis.formats.utils.pdf_filter import PdfExtractInvestmentsStandard
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import (
    pdfline_selection_from_str,
    PdfLineSelection,
)
from freeports_analysis.formats.utils.pdf_filter.pdf_parts.font import FontSet
from freeports_analysis.formats.algorithms.commons import Pipeline


subfund_set = PdfLineSelection.area(0.0, 0.0, 1e6, 60.0) & (
    PdfLineSelection.font("Calibri") | PdfLineSelection.font("Lato-Regular")
)


body_set = PdfLineSelection(
    font="TrebuchetMS", font_size=(6.95, 6.97)
) & PdfLineSelection.area_from_bounds(
    x0=0.0,
    y0=PdfLineSelection(
        font="Lato-Regular",
        text="Elenco analitico dei principali strumenti finanziari detenuti dal Fondo",
        font_size=(11.9, 12.1),
    ),
    x1=1e6,
    y1=1e6,
)

pipelines = {
    "investments": Pipeline(
        pdf_extract=PdfExtractInvestmentsStandard(
            subfund_set=subfund_set,
            currency_set="EUR",
            body_set=body_set,
        )
    )
}
