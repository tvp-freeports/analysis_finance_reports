from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.utils.deserialize import to_int
from freeports_analysis.formats.utils.pdf_extract import PdfExtractAssetsStandard
from freeports_analysis.formats.utils.text_filter import TextFilterAssetsStandard
from freeports_analysis.formats.utils.deserialize import DeserializeAssetsStandard


x0 = PdfLineSelection(font="arialnarrow-bold", font_size=(8.9, 9.1), text="Notes")

y0 = PdfLineSelection(
    font="arialnarrow-bold",
    font_size=(13.9, 14.1),
    text="Statement of Net Assets",
)
y1 = PdfLineSelection(font="arialnarrow-bold", font_size=(8.9, 9.1), text="^ASSETS")

x1 = 1e6


pdf_extract = PdfExtractAssetsStandard(
    fund_set=PdfLineSelection.area_from_bounds(x0=x0, y0=y0, x1=x1, y1=y1),
    currency_set=None,
    tot_assets_set=PdfLineSelection(
        font="arialnarrow-bold", font_size=(8.9, 9.1), text="LIABILITIES"
    ),
    liabilities_set=PdfLineSelection(
        font="arialnarrow-bold", font_size=(8.9, 9.1), text="TOTAL NET ASSETS"
    ),
    net_assets_set=PdfLineSelection(
        font="arialnarrow-bold", font_size=(8.9, 9.1), text="TOTAL NET ASSETS"
    ),
    tot_assets_vec=(1.2, -3.5),
    liabilities_vec=(1.2, -3.5),
    net_assets_vec=(1.2, 0.0),
    tot_assets_mult=(
        100.0,
        3.0,
    ),
    liabilities_mult=(100.0, 3.0),
    net_assets_mult=(100.0, 2.2),
)

text_filter = TextFilterAssetsStandard()
deserialize = DeserializeAssetsStandard(converter=to_int)
