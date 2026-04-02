"""CANE-EN23 custom functions"""

from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractCurrencyStandard,
    PdfExtractFundStandard,
    PdfExtractManagmentCompanyStandard,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdfline_selection_from_str,
    PdfLineSelection,
)
from freeports_analysis.formats.utils.text_filter import (
    TextFilterManagmentCompanyStandard,
)
from freeports_analysis.formats.utils.deserialize import (
    DeserializerManagmentCompanyStandard,
)
from freeports_analysis.formats.algorithms.commons import Pipeline

subfund_set = PdfLineSelection(
    font="ArialMT", font_size=(6.95, 6.97)
) & PdfLineSelection.area_from_movewindow(
    target=pdfline_selection_from_str('ArialMT[6.96] "^Annual report including"'),
    vec=(0.1, 0.8),
    width_mult=2.0,
    height_mult=1.4,
)

currency_set = pdfline_selection_from_str('Arial-BoldMT "Valuation in"')
body_set = pdfline_selection_from_str("ArialMT[6.96](160:786)")


pipelines = {
    "investments": Pipeline(
        pdf_extract=(
            PdfExtractInvestmentsStandard(currency_set=currency_set, body_set=body_set),
            PdfExtractFundStandard(subfund_set),
            PdfExtractCurrencyStandard(currency_set),
        )
    ),
    "manco": Pipeline(
        pdf_extract=PdfExtractManagmentCompanyStandard(
            PdfLineSelection.area_from_movewindow(
                PdfLineSelection(
                    font="arial-boldmt", font_size=(7.9, 8.1), text="Management Company"
                ),
                (-0.1, 0.8),
                100,
                2.5,
            )
            & PdfLineSelection.area_from_bounds(
                x1=PdfLineSelection(
                    font="arial-boldmt", font_size=(7.9, 8.1), text="Legal adviser"
                ),
                x0=0,
                y0=0.0,
                y1=1e6,
            )
        ),
        text_filter=TextFilterManagmentCompanyStandard(),
        deserialize=DeserializerManagmentCompanyStandard(),
    ),
}
