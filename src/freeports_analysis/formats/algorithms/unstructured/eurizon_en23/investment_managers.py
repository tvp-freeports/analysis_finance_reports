from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractPageClassifyStandard,
    PdfExtractManagmentCompanyStandard,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdflines_from_pagedict,
    PdfLineSelection,
)
from freeports_analysis.formats.utils.text_filter.match import normalize_string
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.algorithms import PdfBlock, TextBlock
from freeports_analysis.formats.utils.text_filter import (
    ResultStandardFiltering,
    TextFilterManagmentCompanyStandard,
    StandardInvestmentsMangerTextBlock,
    StandardFundTextBlock,
)
from freeports_analysis.formats.utils.text_filter.match import MatchFund
from freeports_analysis.formats.utils.deserialize import (
    DeserializerFundStandard,
    DeserializerManagmentCompanyStandard,
    DeserializerInvestmentsManagerStandard,
)
from freeports_analysis import output
import logging
from enum import Enum, auto

logger = logging.getLogger(__name__)


class TipiBlocco(Enum):
    INV = auto()
    MAN = auto()
    SUB = auto()
    ALT_INV = auto()


def pdf_extract_inv_managers_begin(page):

    lines = pdflines_from_pagedict(page)
    condition_text = PdfLineSelection(
        text="INVESTMENT MANAGERS", font="frutiger-lightitalic"
    )
    end = PdfLineSelection(
        text="INDEPENDENT AUDITOR OF THE INVESTMENT FUND AND OF THE MANAGEMENT COMPANY",
        font="frutiger-lightitalic",
    ).select(lines)
    btm = 1e6
    if end:
        btm = end[0].bbox[1]
    bold_text = PdfLineSelection(
        font="frutiger-black", font_size=(8.98, 8.99)
    ) & PdfLineSelection.area_from_bounds(x0=0, y0=condition_text, x1=1e6, y1=btm)
    fund_text = PdfLineSelection(
        font="frutiger-lightitalic", font_size=(8.98, 8.99)
    ) & PdfLineSelection.area_from_bounds(x0=0, y0=condition_text, x1=1e6, y1=btm)

    lines_manager = bold_text.select(lines)
    lines_fund = fund_text.select(lines)

    v1 = [PdfBlock(TipiBlocco.INV, {}, l.text) for l in lines_manager]
    v1.extend([PdfBlock(TipiBlocco.SUB, {}, l.text) for l in lines_fund])

    return v1


# def pdf_extract_inv_managers_begin_alt(lines):
#     condition_text = PdfLineSelection(
#         text="INVESTMENT MANAGERS", font="frutiger-lightitalic"
#     )

#     bold_text = PdfLineSelection(
#         font="frutiger-black", font_size=(8.98, 8.99)
#     ) & PdfLineSelection.area_from_bounds(x0=0, y0=condition_text, x1=1e6, y1=1e6)

#     funds = ((
#         PdfLineSelection.area_from_bounds(
#             x0=0.0,y0=PdfLineSelection.text("This function has been delegated by"),x1=1e6,y1=1e6
#         ) / PdfLineSelection.area_from_movewindow(
#             bold_text & PdfLineSelection.area_from_bounds(
#                 x0=0.0,
#                 y0=PdfLineSelection.text("This function has been delegated by"),
#                 x1=1e6,
#                 y1=1e6
#             ),(-0.1,0.0),1.0,4.0
#         )
#     ) & PdfLineSelection.font_size(8.9,9.0)).select(lines)

#     return [

#     ]


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


pdf_extract_manco = PdfExtractManagmentCompanyStandard(
    PdfLineSelection(font="frutiger-black", font_size=(8.98, 8.99))
    & PdfLineSelection.area_from_bounds(
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
)


def text_filter_inv_managers(blocks, results):

    inv_funds = set(
        MatchFund(name=n.fund)
        for n in filter(lambda x: isinstance(x, output.Investment), results)
    )

    final = []
    inv = [b for b in blocks if b.type_block == TipiBlocco.INV]
    sub = [b.content for b in blocks if b.type_block == TipiBlocco.SUB]
    sub = " ".join(sub)
    sub = sub.split(")")[:-1]
    for s in sub:
        final.append(
            [e.strip() for e in s.replace("(", "").replace(")", "").split(",")]
        )
    funds = []
    for s in final:
        funds.append(set())
        s[0] = (
            s[0]
            .replace("for the Sub-Funds", "for the Sub-Fund")
            .split("for the Sub-Fund")[-1]
            .strip()
        )
        for sub in s:
            funds[-1] = funds[-1].union(
                set([MatchFund(name=f) for f in sub.split("and")])
            )
    res = []
    for i, s in zip(inv, funds):
        if not s.isdisjoint(inv_funds):
            res.append(StandardInvestmentsMangerTextBlock(i, s))
            res.extend(
                [
                    StandardFundTextBlock.from_matched_fund(f)
                    for f in s
                    if f not in inv_funds
                ]
            )
    return res


def text_filter_inv_managers_begin(blocks, results):
    filter_funds = set(
        MatchFund(name=n.fund)
        for n in filter(lambda x: isinstance(x, output.Investment), results)
    )
    inv_managers = set(
        filter(lambda x: isinstance(x, output.InvestmentsManager), results)
    )
    a_subfunds = set([f for inv in inv_managers for f in inv.managed_funds])
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
            funds[-1] = funds[-1].union(
                set([MatchFund(name=f) for f in sub.split("and")])
            )

    additional_funds = set([f for im in funds for f in im])
    try:
        funds[0] = residual_funds - additional_funds
    except IndexError:
        logger.error("Fund not found probably the layout is not the expected one")
    res = [
        StandardInvestmentsMangerTextBlock(i, s)
        for i, s in zip(inv, funds)
        if not s.isdisjoint(filter_funds)
    ]
    res.extend(
        [
            StandardFundTextBlock.from_matched_fund(f)
            for im in funds
            for f in im
            if f not in filter_funds
        ]
    )

    return res


text_filter_manco = TextFilterManagmentCompanyStandard()

deserialize_inv_managers = DeserializerInvestmentsManagerStandard()

deserialize_manco = DeserializerManagmentCompanyStandard()

deserialize_fund = DeserializerFundStandard()
