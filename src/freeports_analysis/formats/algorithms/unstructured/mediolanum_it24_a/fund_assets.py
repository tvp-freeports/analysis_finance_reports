from freeports_analysis.formats.algorithms import PdfBlock, TextBlock
from freeports_analysis.formats.utils.pdf_extract import (
    OnePdfBlockType,
    PdfExtractAssetsStandard,
    PdfExtractCurrencyStandard,
    ResultStandardExtraction,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
)
from freeports_analysis.formats.utils.text_filter import (
    TextFilterAssetsStandard,
    OneTextBlockType,
    extract_currency_from_text,
)
from freeports_analysis.formats.utils.deserialize import (
    DeserializeAssetsStandard,
    to_int,
    to_currency,
    to_int_en_month,
    to_date_with_it_month,
)
from freeports_analysis.consts import Currency
from freeports_analysis.output import Fund, FundAssets
from freeports_analysis.formats.utils.text_filter.match import MatchFund
import datetime


pdf_extract_funds = PdfLineSelection.area_from_movewindow(
    PdfLineSelection(text="(valori espressi in"), (1.2, -0.1), 100.0, 2.3
)


pdf_extract = PdfExtractAssetsStandard(
    fund_set=pdf_extract_funds,
    currency_set=PdfLineSelection(text="(valori espressi in"),
    tot_assets_set=PdfLineSelection(text="TOTALE ATTIVITÀ"),
    liabilities_set=PdfLineSelection(
        text="sottoscrittori di quote di partecipazione riscattabili"
    ),
    net_assets_set=PdfLineSelection(text="RISCATTABILI"),
    tot_assets_vec=(1.2, 0.0),
    liabilities_vec=(1.2, 0.0),
    net_assets_vec=(1.2, 0.0),
    tot_assets_mult=(100.0, 1.3),
    liabilities_mult=(100.0, 1.3),
    net_assets_mult=(100.0, 1.3),
    date_set=PdfLineSelection(font_size=(11, 12.1), text="^AL"),
    table_condition=True,
    skip_column=2,
)


text_filter = TextFilterAssetsStandard("AL ([0-9]+ .+ [0-9]+)")
deserialize = DeserializeAssetsStandard(
    lambda txt: 0 if txt == "- " else to_int(txt), date_converter=to_date_with_it_month
)
