"""CANE-EN23 custom functions"""

from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractCurrencyStandard,
    PdfExtractFundStandard,
    PdfExtractManagmentCompanyStandard,
    PdfExtractAssetsStandard,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdfline_selection_from_str,
    PdfLineSelection,
)
from freeports_analysis.formats.utils.text_filter import (
    TextFilterManagmentCompanyStandard,
    TextFilterAssetsStandard,
)
from freeports_analysis.formats.utils.deserialize import (
    DeserializerManagmentCompanyStandard,
    DeserializeAssetsStandard,
    to_float,
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


currency_set_assets = PdfLineSelection.area_from_movewindow(
    target=PdfLineSelection(font="arial-boldmt", font_size=(7.9, 8.2), text="^Assets"),
    vec=(1.2, -1.0),
    width_mult=100,
    height_mult=1.5,
)

pdf_extract_assets = PdfExtractAssetsStandard(
    fund_set=subfund_set,
    currency_set=currency_set_assets,
    tot_assets_set=PdfLineSelection(
        font="arial-boldmt", font_size=(7.9, 8.2), text="^Total Assets"
    ),
    liabilities_set=PdfLineSelection(
        font="arial-boldmt", font_size=(7.9, 8.2), text="^Total Liabilities"
    ),
    net_assets_set=PdfLineSelection(
        font="arial-boldmt",
        font_size=(7.9, 8.2),
        text="^Net assets at the end of the financial year",
    ),
    tot_assets_vec=(1.2, 0.0),
    liabilities_vec=(1.2, 0.0),
    net_assets_vec=(1.2, 0.0),
    tot_assets_mult=(100.0, 1.02),
    liabilities_mult=(100.0, 1.02),
    net_assets_mult=(100.0, 1.02),
)

text_filter_assets = TextFilterAssetsStandard()
deserialize_assets = DeserializeAssetsStandard(converter=to_float)

pipelines = {
    "investments": Pipeline(
        pdf_extract=(
            PdfExtractInvestmentsStandard(currency_set=currency_set, body_set=body_set),
            PdfExtractFundStandard(subfund_set),
            PdfExtractCurrencyStandard(currency_set),
        )
    ),
    "fund_assets": Pipeline(pdf_extract_assets, text_filter_assets, deserialize_assets),
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
