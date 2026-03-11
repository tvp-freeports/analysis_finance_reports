"""Custom pdf filter for FINECO-EN23[IR] format"""

from freeports_analysis.formats.utils.pdf_extract import PdfExtractInvestmentsStandard
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.algorithms.commons import Pipeline

tnrb = PdfLineSelection.font("TimesNewRoman,Bold")

subfund_set = (
    PdfLineSelection.font_size(9.95, 10.03)
    & PdfLineSelection.area_from_bounds(
        x0=0.0,
        x1=1e6,
        y0=PdfLineSelection.text("Condensed Schedule of Investments") & tnrb,
        y1=PdfLineSelection.text("Domicile") & tnrb,
    )
    & tnrb
)

currency_set = (
    PdfLineSelection.area_from_movewindow(
        target=PdfLineSelection.text("Fair Value") & tnrb,
        vec=(0.0, 1.0),
        width_mult=1.2,
        height_mult=1.2,
    )
    & tnrb
)

body_set = (
    (PdfLineSelection.font("TimesNewRoman") | tnrb)
    & PdfLineSelection.font_size(9.95, 10.03)
    & PdfLineSelection.area_from_bounds(
        x0=135.0,
        x1=1e6,
        y0=185.0,
        y1=(
            PdfLineSelection.text("SWAPS")
            | PdfLineSelection.text("FORWARDS")
            | PdfLineSelection.text("FEATURES")
        )
        & tnrb,
    )
    / PdfLineSelection.text("-$")
) & PdfLineSelection.area(0.0, 0.0, 1e6, 750)

pipelines = {
    "investments": Pipeline(
        pdf_extract=PdfExtractInvestmentsStandard(
            subfund_set=subfund_set, currency_set=currency_set, body_set=body_set
        )
    )
}
