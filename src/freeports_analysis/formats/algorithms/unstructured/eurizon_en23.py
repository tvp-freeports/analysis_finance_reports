from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractPageClassifyStandard,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdflines_from_pagedict,
    PdfLineSelection,
)
from freeports_analysis.formats.utils.text_filter.match import normalize_string
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.algorithms import PdfBlock, TextBlock
from freeports_analysis.output import (
    ManagementCompany,
    InvestmentsManager,
    Investment,
    AssetsManager,
)
from enum import Enum, auto


class TipiBlocco(Enum):
    INV = auto()
    MAN = auto()
    SUB = auto()


def compute_page_class(classification):
    last_value = None
    for i, val in enumerate(classification):
        if val == "inv_managers" and last_value != "inv_managers":
            last_value = val
        elif val == "inv_managers" and last_value == "inv_managers":
            last_value = None
        elif val is None and last_value == "inv_managers":
            classification[i] = last_value
    return classification


def pdf_extract_inv_managers(page):

    lines = pdflines_from_pagedict(page)
    condition_text = PdfLineSelection(
        text="INDEPENDENT AUDITOR OF THE INVESTMENT FUND AND OF THE MANAGEMENT COMPANY",
        font="frutiger-lightitalic",
    )

    bold_text = PdfLineSelection(
        font="frutiger-black", font_size=(8.98, 8.99)
    ) & PdfLineSelection.area_from_bounds(x0=0, y0=0, x1=1e6, y1=condition_text)
    fund_text = PdfLineSelection(
        font="frutiger-lightitalic", font_size=(8.98, 8.99)
    ) & PdfLineSelection.area_from_bounds(x0=0, y0=0, x1=1e6, y1=condition_text)

    lines_manager = bold_text.select(lines)
    lines_fund = fund_text.select(lines)

    v1 = [PdfBlock(TipiBlocco.INV, {}, l.text) for l in lines_manager]
    v1.extend([PdfBlock(TipiBlocco.SUB, {}, l.text) for l in lines_fund])
    return v1


def pdf_extract_manco(page):
    lines = pdflines_from_pagedict(page)
    condition_text = PdfLineSelection(
        text="BOARD OF DIRECTORS OF THE MANAGEMENT COMPANY", font="frutiger-lightitalic"
    )
    bold_text = PdfLineSelection(
        font="frutiger-black", font_size=(8.98, 8.99)
    ) & PdfLineSelection.area_from_bounds(x0=0, y0=0, x1=1e6, y1=condition_text)
    lines = bold_text.select(lines)
    return [PdfBlock(TipiBlocco.MAN, {}, l.text) for l in lines]


def text_filter_inv_managers(blocks, investments):
    subfunds = set([normalize_string(i.subfund) for i in investments])

    final = []
    inv = [b for b in blocks if b.type_block == TipiBlocco.INV]
    sub = [b.content for b in blocks if b.type_block == TipiBlocco.SUB]
    sub = "".join(sub)
    sub = sub.split(")")[:-1]
    for s in sub:
        final.append(
            [e.strip() for e in s.replace("(", "").replace(")", "").split(",")]
        )
    funds = []
    for s in final:
        funds.append([])
        s[0] = s[0].split("for the Sub-Funds")[-1].strip()
        for sub in s:
            funds[-1].extend(sub.split("and"))
    return [
        TextBlock(TipiBlocco.INV, {"funds": s}, i)
        for i, s in zip(inv, funds)
        if any(normalize_string(x) in subfunds for x in s)
    ]


def text_filter_manco(blocks, results):

    subfunds = [i.subfund for i in results if isinstance(i, Investment)]
    subfunds.extend(
        [s for a in results if isinstance(a, AssetsManager) for s in a.managed_funds]
    )
    subfunds = set(subfunds)

    return [
        TextBlock(TipiBlocco.MAN, {"funds": list(subfunds)}, b)
        for b in blocks
        if b.type_block == TipiBlocco.MAN
    ]


def deserialize_inv_managers(text_block):
    return InvestmentsManager(
        name=text_block.content, managed_funds=text_block.metadata["funds"]
    )


def deserialize_manco(text_block):
    return ManagementCompany(
        name=text_block.content, managed_funds=text_block.metadata["funds"]
    )


pipelines = {
    "": Pipeline(
        pdf_extract=PdfExtractPageClassifyStandard(
            header_sets=[
                PdfLineSelection.font("frutiger-lightitalic")
                & (
                    PdfLineSelection.text("INVESTMENT MANAGERS")
                    | PdfLineSelection.text(
                        "INDEPENDENT AUDITOR OF THE INVESTMENT FUND AND OF THE MANAGEMENT COMPANY"
                    )
                ),
                PdfLineSelection(
                    text="ORGANISATION OF THE FUND", font="frutiger-black"
                ),
            ],
            page_type="inv_managers",
        )
    ),
    "manco": Pipeline(
        pdf_extract=pdf_extract_manco,
        text_filter=text_filter_manco,
        deserialize=deserialize_manco,
    ),
    "inv_managers": Pipeline(
        pdf_extract=pdf_extract_inv_managers,
        text_filter=text_filter_inv_managers,
        deserialize=deserialize_inv_managers,
    ),
}
