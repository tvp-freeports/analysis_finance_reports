from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.utils.deserialize import to_int
from freeports_analysis.formats.utils.pdf_extract import PdfExtractAssetsStandard
from freeports_analysis.formats.utils.text_filter import TextFilterAssetsStandard
from freeports_analysis.formats.utils.deserialize import DeserializeAssetsStandard


x0 = PdfLineSelection(font="timesnewromanbold", font_size=(8.9, 9.1), text="Notes")
y1 = PdfLineSelection(font="timesnewromanbold", font_size=(8.9, 9.1), text="^Assets")
pdf_extract = PdfExtractAssetsStandard(
    fund_set=PdfLineSelection.area_from_bounds(x0=x0, y0=0, x1=1e6, y1=y1),
    currency_set=None,
    tot_assets_set=PdfLineSelection(
        font="timesnewromanbold", font_size=(8.9, 9.1), text="Total assets"
    ),
    liabilities_set=PdfLineSelection(
        font="timesnewromanbold", font_size=(8.9, 9.1), text="Total liabilities"
    ),
    net_assets_set=PdfLineSelection(
        font="timesnewromanbold", font_size=(8.9, 9.1), text="Net assets"
    ),
    tot_assets_vec=(1.2, 0.0),
    liabilities_vec=(1.2, 0.0),
    net_assets_vec=(1.2, 0.0),
    tot_assets_mult=(100.0, 1.2),
    liabilities_mult=(100.0, 2.2),
    net_assets_mult=(100.0, 2.2),
)

text_filter = TextFilterAssetsStandard()
deserialize = DeserializeAssetsStandard(converter=to_int)
