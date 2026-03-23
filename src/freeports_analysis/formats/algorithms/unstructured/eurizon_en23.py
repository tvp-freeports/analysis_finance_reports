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
from freeports_analysis.formats.utils.text_filter import ResultStandardFiltering
from freeports_analysis.formats.utils.deserialize import DeserializerFundStandard
from freeports_analysis.output import (
    ManagementCompany,
    InvestmentsManager,
    Investment,
    AssetsManager,
    Fund,
)
from enum import Enum, auto


class TipiBlocco(Enum):
    INV = auto()
    MAN = auto()
    SUB = auto()


def compute_page_class(classification):
    inv_managers = False
    for i, val in enumerate(classification):
        if inv_managers and val is None:
            classification[i] = "inv_managers"
        elif val == "inv_managers_begin":
            inv_managers = True
        elif val == "inv_managers_end":
            inv_managers = False
    return classification


def pdf_extract_inv_managers_begin(page):

    lines = pdflines_from_pagedict(page)
    condition_text = PdfLineSelection(
        text="INVESTMENT MANAGERS", font="frutiger-lightitalic"
    )

    bold_text = PdfLineSelection(
        font="frutiger-black", font_size=(8.98, 8.99)
    ) & PdfLineSelection.area_from_bounds(x0=0, y0=condition_text, x1=1e6, y1=1e6)
    fund_text = PdfLineSelection(
        font="frutiger-lightitalic", font_size=(8.98, 8.99)
    ) & PdfLineSelection.area_from_bounds(x0=0, y0=condition_text, x1=1e6, y1=1e6)

    lines_manager = bold_text.select(lines)
    lines_fund = fund_text.select(lines)

    v1 = [PdfBlock(TipiBlocco.INV, {}, l.text) for l in lines_manager]
    v1.extend([PdfBlock(TipiBlocco.SUB, {}, l.text) for l in lines_fund])

    return v1


def pdf_extract_inv_managers(page):

    lines = pdflines_from_pagedict(page)

    bold_text = PdfLineSelection(font="frutiger-black", font_size=(8.98, 8.99))
    fund_text = PdfLineSelection(font="frutiger-lightitalic", font_size=(8.98, 8.99))

    lines_manager = bold_text.select(lines)
    lines_fund = fund_text.select(lines)

    v1 = [PdfBlock(TipiBlocco.INV, {}, l.text) for l in lines_manager]
    v1.extend([PdfBlock(TipiBlocco.SUB, {}, l.text) for l in lines_fund])

    return v1


def pdf_extract_inv_managers_end(page):

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
    bold_text = PdfLineSelection(
        font="frutiger-black", font_size=(8.98, 8.99)
    ) & PdfLineSelection.area_from_bounds(
        x0=0,
        y0=PdfLineSelection(
            text="MANAGEMENT COMPANY AND PROMOTER", font="frutiger-lightitalic"
        ),
        x1=1e6,
        y1=PdfLineSelection(
            text="BOARD OF DIRECTORS OF THE MANAGEMENT COMPANY",
            font="frutiger-lightitalic",
        ),
    )
    lines = bold_text.select(lines)
    return [PdfBlock(TipiBlocco.MAN, {}, l.text) for l in lines]


def text_filter_inv_managers(blocks, results):
    inv_funds = set(
        Fund(name=n.fund) for n in filter(lambda x: isinstance(x, Investment), results)
    )

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
        funds.append(set())
        s[0] = s[0].split("for the Sub-Funds")[-1].strip()
        for sub in s:
            funds[-1] = funds[-1].union(set([Fund(name=f) for f in sub.split("and")]))
    res = []
    for i, s in zip(inv, funds):
        if not s.isdisjoint(inv_funds):
            res.append(TextBlock(TipiBlocco.INV, {"funds": s}, i))
            res.extend(
                [TextBlock.from_content(ResultStandardFiltering.FUND, {}, f.name)]
                for f in s
                if f not in inv_funds
            )
    return res


def text_filter_inv_managers_begin(blocks, results):
    filter_funds = set(
        Fund(name=n.fund) for n in filter(lambda x: isinstance(x, Investment), results)
    )
    inv_managers = set(filter(lambda x: isinstance(x, InvestmentsManager), results))
    a_subfunds = set([f for inv in inv_managers for f in inv.funds])
    residual_funds = filter_funds - a_subfunds

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
        funds.append(set())
        s[0] = s[0].split("for the Sub-Funds")[-1].strip()
        for sub in s:
            funds[-1] = funds[-1].union(set([Fund(name=f) for f in sub.split("and")]))

    additional_funds = set([f for im in funds for f in im])
    funds[0] = residual_funds - additional_funds

    res = [
        TextBlock(TipiBlocco.INV, {"funds": s}, i)
        for i, s in zip(inv, funds)
        if not s.isdisjoint(filter_funds)
    ]
    res.extend(
        [
            TextBlock.from_content(ResultStandardFiltering.FUND, {}, f.name)
            for im in funds
            for f in im
            if f not in filter_funds
        ]
    )

    return res


def text_filter_manco(blocks, results):
    filter_funds = set(filter(lambda x: isinstance(x, Fund), results))

    return [
        TextBlock(TipiBlocco.MAN, {"funds": filter_funds}, b)
        for b in blocks
        if b.type_block == TipiBlocco.MAN
    ]


def deserialize_inv_managers(text_block):
    if text_block.type_block == TipiBlocco.INV:
        return InvestmentsManager(
            name=text_block.content,
            managed_funds=set((f.name for f in text_block.metadata["funds"])),
        )


def deserialize_manco(text_block):
    return ManagementCompany(
        name=text_block.content,
        managed_funds=set((f.name for f in text_block.metadata["funds"])),
    )


deserialize_fund = DeserializerFundStandard()
pipelines = {
    "manco": Pipeline(
        pdf_extract=pdf_extract_manco,
        text_filter=text_filter_manco,
        deserialize=deserialize_manco,
    ),
    "inv_managers_begin": Pipeline(
        pdf_extract=pdf_extract_inv_managers_begin,
        text_filter=text_filter_inv_managers_begin,
        deserialize=(deserialize_inv_managers, deserialize_fund),
    ),
    "inv_managers": Pipeline(
        pdf_extract=pdf_extract_inv_managers,
        text_filter=text_filter_inv_managers,
        deserialize=(deserialize_inv_managers, deserialize_fund),
    ),
    "inv_managers_end": Pipeline(
        pdf_extract=pdf_extract_inv_managers_end,
        text_filter=text_filter_inv_managers,
        deserialize=(deserialize_inv_managers, deserialize_fund),
    ),
}
