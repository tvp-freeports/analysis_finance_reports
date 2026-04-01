from freeports_analysis.formats.utils.pdf_extract import PdfExtractAssetsStandard
from freeports_analysis.formats.utils.text_filter import TextFilterAssetsStandard
from freeports_analysis.formats.utils.deserialize import DeserializeAssetsStandard
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.utils.deserialize import to_float


condition_text = PdfLineSelection(
    text="STATEMENT OF NET ASSETS AS AT", font="frutiger-black"
)

pdf_extract = PdfExtractAssetsStandard(
    fund_set=PdfLineSelection.area_from_bounds(x0=0, y0=0, x1=1e6, y1=condition_text),
    currency_set=PdfLineSelection(
        text="STATEMENT OF NET ASSETS AS AT", font="frutiger-black"
    ),
    tot_assets_set=PdfLineSelection(text="Total assets"),
    liabilities_set=PdfLineSelection(text="Total liabilities"),
    net_assets_set=PdfLineSelection(text="Total net assets"),
    tot_assets_vec=(1.2, 0.0),
    liabilities_vec=(1.2, 0.0),
    net_assets_vec=(1.2, 0.0),
    tot_assets_mult=(100.0, 1.02),
    liabilities_mult=(100.0, 1.02),
    net_assets_mult=(100.0, 1.02),
)
text_filter = TextFilterAssetsStandard()
deserialize = DeserializeAssetsStandard(converter=to_float)
