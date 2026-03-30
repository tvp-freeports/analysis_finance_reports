from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractFundStandard,
    PdfExtractCurrencyStandard,
    ResultStandardExtraction,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdflines_from_pagedict,
    PdfLineSelection,
)
from freeports_analysis.formats.utils.text_filter import extract_currency_from_text
from freeports_analysis.formats.utils.deserialize import to_float
from freeports_analysis.output import Fund, FundAssets
from freeports_analysis.formats.algorithms import PdfBlock
from freeports_analysis.formats.algorithms import TextBlock
import copy
from enum import Enum, auto


class TipiBlocco(Enum):
    ASS = auto()


condition_text = PdfLineSelection(
    text="STATEMENT OF NET ASSETS AS AT", font="frutiger-black"
)
fund_text = PdfLineSelection.area_from_bounds(x0=0, y0=0, x1=1e6, y1=condition_text)
pdf_extract_fund = PdfExtractFundStandard(fund_text)
pdf_extract_currency = PdfExtractCurrencyStandard(condition_text)


def pdf_extract(page):
    lines = pdflines_from_pagedict(page)
    total_assets = (
        PdfLineSelection.area_from_movewindow(
            PdfLineSelection(text="Total assets"), (1.2, 0.0), 200.0, 1.3
        )
        .select(lines)[0]
        .text
    )
    total_liabilities = (
        PdfLineSelection.area_from_movewindow(
            PdfLineSelection(text="Total liabilities"), (1.2, 0.0), 100.0, 1.3
        )
        .select(lines)[0]
        .text
    )
    total_net_assets = (
        PdfLineSelection.area_from_movewindow(
            PdfLineSelection(text="Total net assets"), (1.2, 0.0), 100.0, 1.3
        )
        .select(lines)[0]
        .text
    )
    data = {
        "assets": total_assets,
        "liabilities": total_liabilities,
        "net_assets": total_net_assets,
    }
    v1 = [PdfBlock(TipiBlocco.ASS, data, "")]
    return v1


def text_filter(blocks, subfunds):
    fund = [b for b in blocks if b.type_block == ResultStandardExtraction.FUND_NAME][0]
    currency = [
        b for b in blocks if b.type_block == ResultStandardExtraction.CURRENCY_STATEMENT
    ][0]
    ass = [b for b in blocks if b.type_block == TipiBlocco.ASS][0]
    sub = set(filter(lambda x: isinstance(x, Fund), subfunds))
    fund = Fund(name=fund.content)
    if fund not in sub:
        return []
    currency = extract_currency_from_text(currency.content)
    all_meta = copy.deepcopy(ass.metadata)
    all_meta["fund"] = fund.name
    all_meta["currency"] = currency

    return [TextBlock(TipiBlocco.ASS, all_meta, ass)]


def deserialize(text_block):
    ass = copy.deepcopy(text_block.metadata)
    ass["assets"] = to_float(ass["assets"])
    ass["liabilities"] = to_float(ass["liabilities"].replace("(", "").replace(")", ""))
    ass["net_assets"] = to_float(ass["net_assets"])
    return FundAssets(
        tot_assets=ass["assets"],
        liabilities=ass["liabilities"],
        net_assets=ass["net_assets"],
        fund=ass["fund"],
        currency=ass["currency"],
    )
