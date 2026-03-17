from freeports_analysis.formats.algorithms import PdfBlock
from enum import Enum, auto
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdflines_from_pagedict,
    PdfLineSelection,
)
from freeports_analysis.formats.algorithms import TextBlock

from typing import List
from freeports_analysis.formats.algorithms.commons import Pipeline

from freeports_analysis.formats.utils.pdf_extract import PdfExtractPageClassifyStandard
from freeports_analysis.formats.utils.text_filter import TextFilterPageClassifyStandard
from freeports_analysis.formats.utils.deserialize import (
    DeserializerPageClassifyStandard,
)

from test_single_page import get_doc_from_tests


class TipiBlocco(Enum):
    INV = auto()
    MAN = auto()
    SUB = auto()


def PdfExtract(page):

    lines = pdflines_from_pagedict(page)

    bold_text = PdfLineSelection(font="frutiger-black", font_size=(8.98, 8.99))
    fund_text = PdfLineSelection(font="frutiger-lightitalic", font_size=(8.98, 8.99))

    lines_manager = bold_text.select(lines)
    lines_fund = fund_text.select(lines)

    v1 = [PdfBlock(TipiBlocco.INV, {}, l.text) for l in lines_manager]
    v1.extend([PdfBlock(TipiBlocco.SUB, {}, l.text) for l in lines_fund])
    return v1


def PdfExtract_man(page):
    lines = pdflines_from_pagedict(page)
    condition_text = PdfLineSelection(
        text="INDEPENDENT AUDITOR OF THE INVESTMENT FUND AND OF THE MANAGEMENT COMPANY",
        font="frutiger-lightitalic",
    )

    bold_text_up = PdfLineSelection(
        font="frutiger-black", font_size=(8.98, 8.99)
    ) & PdfLineSelection.area_from_bounds(x0=0, y0=0, x1=1e6, y1=condition_text)
    fund_text = PdfLineSelection(
        font="frutiger-lightitalic", font_size=(8.98, 8.99)
    ) & PdfLineSelection.area_from_bounds(x0=0, y0=0, x1=1e6, y1=condition_text)
    bold_text_down = PdfLineSelection(
        font="frutiger-black", font_size=(8.98, 8.99)
    ) & PdfLineSelection.area_from_bounds(x0=0, y0=condition_text, x1=1e6, y1=1e6)
    lines_up = bold_text_up.select(lines)
    lines_fund = fund_text.select(lines)
    lines_down = bold_text_down.select(lines)
    final = [PdfBlock(TipiBlocco.INV, {}, l.text) for l in lines_up]
    final.extend([PdfBlock(TipiBlocco.MAN, {}, l.text) for l in lines_down])
    final.extend([PdfBlock(TipiBlocco.SUB, {}, l.text) for l in lines_fund])
    return final


def TextFilter(blocks, subfunds):
    # divisione dei sub
    final = []
    inv = [b for b in blocks if b.type_block == TipiBlocco.INV]
    sub = [b.content for b in blocks if b.type_block == TipiBlocco.SUB]
    sub = "".join(sub)
    sub = sub.split(")")[:-1]
    for s in sub:
        final.append(
            [e.strip() for e in s.replace("(", "").replace(")", "").split(",")]
        )

    sub = final
    final = []
    return [
        TextBlock(TipiBlocco.INV, {"funds": s}, i) for i, s in zip(inv, sub)
    ]  # if any(x in subfunds for x in s)]


def TextFilter_man(blocks, subfunds):

    m = [
        TextBlock(TipiBlocco.MAN, {}, b)
        for b in blocks
        if b.type_block == TipiBlocco.MAN
    ]

    final = []
    inv = [b for b in blocks if b.type_block == TipiBlocco.INV]
    sub = [b.content for b in blocks if b.type_block == TipiBlocco.SUB]
    sub = "".join(sub)
    sub = sub.split(")")[:-1]
    for s in sub:
        final.append(
            [e.strip() for e in s.replace("(", "").replace(")", "").split(",")]
        )

    sub = final
    m.extend(
        [TextBlock(TipiBlocco.INV, {"funds": s}, i) for i, s in zip(inv, sub)]
    )  # if any(x in subfunds for x in s)])
    return m


class InvestmentMenager:
    name: str
    sub: List[str]

    def __init__(self, name, sub):
        self.name = name
        self.sub = sub

    def __repr__(self):
        return f"{self.__class__.__name__}({self.name}) , sub={self.sub})"


class ManagementCompany:
    name: str

    def __init__(self, name):
        self.name = name

    def __repr__(self):
        return f"{self.__class__.__name__}({self.name}))"


def Deserialize(text_block):
    return InvestmentMenager(text_block.content, text_block.metadata["funds"])


def Deserialize_man(text_block):
    if text_block.type_block == TipiBlocco.INV:
        return InvestmentMenager(text_block.content, text_block.metadata["funds"])
    else:
        return ManagementCompany(text_block.content)


def finalizer(classification):
    last_value = None
    for i, val in enumerate(classification):
        val = val[0]
        if val == "Inv":
            last_value = val  # inizio a propagare INV
        elif val == "Man":
            last_value = None  # stoppa la propagazione
        elif val is None and last_value == "Inv":
            classification[i][0] = last_value
    return classification


format = "EURIZON-EN23"
document = None
page_number = 10
page_type = "investments"

doc = get_doc_from_tests(format, document)
page = doc[page_number - 1]

blocks = PdfExtract(page)
# text_blocks = TextFilter(blocks,["the Sub-Funds Eurizon Fund - Equity China A and Eurizon Fund - Asian EquityOpportunities are managed by Eurizon Capital S.A. as from 10 May 2023"])
text_blocks = TextFilter(blocks, [])

[Deserialize(t) for t in text_blocks]

pipe = Pipeline(PdfExtract, TextFilter, Deserialize)
pipe_man = Pipeline(PdfExtract_man, TextFilter_man, Deserialize_man)

pipe_man(page, [])


header_sets_inv = [
    PdfLineSelection(text="INVESTMENT MANAGERS", font="frutiger-lightitalic"),
    PdfLineSelection(text="ORGANISATION OF THE FUND", font="frutiger-black"),
]

header_sets_man = [
    PdfLineSelection(
        text="INDEPENDENT AUDITOR OF THE INVESTMENT FUND AND OF THE MANAGEMENT COMPANY",
        font="frutiger-lightitalic",
    ),
    PdfLineSelection(text="ORGANISATION OF THE FUND", font="frutiger-black"),
]

p1 = PdfExtractPageClassifyStandard(header_sets_inv, "Inv")
p2 = PdfExtractPageClassifyStandard(header_sets_man, "Man")
t = TextFilterPageClassifyStandard()
d = DeserializerPageClassifyStandard()


pipe_select_page(page, [])

classification = []
for p in range(len(doc)):
    page = doc[p]
    classification.append(pipe_select_page(page, []))

finalizer(classification)
