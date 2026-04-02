from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractPageClassifyStandard,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdflines_from_pagedict,
    PdfLineSelection,
)
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.algorithms import PdfBlock, TextBlock
from freeports_analysis.formats.utils.text_filter import (
    ResultStandardFiltering,
    StandardManagmentCompanyTextBlock,
)
from freeports_analysis.formats.utils.text_filter.match import MatchFund
from freeports_analysis.formats.utils.deserialize import (
    DeserializerFundStandard,
    DeserializerManagmentCompanyStandard,
)
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    TableConfig,
    ColumnConfig,
    get_table_coordinates,
    TablePosAlgorithm,
)
from freeports_analysis import output
from enum import Enum, auto


class TipiBlocco(Enum):
    INV = auto()
    MAN = auto()


l = 190.0
r = 1e6
table_cfg = TableConfig(ColumnConfig(limits=(l, r)))

deselection = (
    PdfLineSelection.text("^1$")
    | PdfLineSelection.text("^2$")
    | PdfLineSelection.text("^3$")
    | PdfLineSelection.text("^4$")
    | PdfLineSelection.text("^5$")
    | PdfLineSelection.text("^6$")
    | PdfLineSelection.text("^7$")
    | PdfLineSelection.text("^8$")
    | PdfLineSelection.text("^9$")
)


def pdf_extract_body(body, type_block=TipiBlocco.INV):
    cs = get_table_coordinates(
        body,
        table_cfg,
        algorithm_flags=TablePosAlgorithm.USE_RULER_AREA
        | TablePosAlgorithm.USE_TEST_POS,
    )
    rows, _ = zip(*cs)
    nrows = max(rows) + 1
    rows_text = [
        "".join((l.text for row, l in zip(rows, body) if row == r))
        for r in range(nrows)
    ]
    return [PdfBlock(type_block, {"row": r}, text) for r, text in enumerate(rows_text)]


def pdf_extract_begin_page(page):
    lines = pdflines_from_pagedict(page)
    t = PdfLineSelection.text("^INVESTMENT ").select(lines)[0].bbox[1] - 8.0
    manco_selection = PdfLineSelection.area(l, 70.0, r, t) / deselection
    b = 705.0
    body_selection = PdfLineSelection.area(l, t, r, b) / deselection
    body = body_selection.select(lines)
    manco = manco_selection.select(lines)
    res = pdf_extract_body(manco, TipiBlocco.MAN)
    res.extend(pdf_extract_body(body, TipiBlocco.INV))
    return res


def pdf_extract(page):
    lines = pdflines_from_pagedict(page)
    t = 70.0
    b = 705.0
    body_selection = PdfLineSelection.area(l, t, r, b) / deselection
    body = body_selection.select(lines)
    return pdf_extract_body(body)


def pdf_extract_end_page(page):
    lines = pdflines_from_pagedict(page)
    t = 70.0
    b = PdfLineSelection.text("^BANCA ").select(lines)[0].bbox[1]
    body_selection = PdfLineSelection.area(l, t, r, b) / deselection
    body = body_selection.select(lines)
    return pdf_extract_body(body, TipiBlocco.INV)


def text_filter_with_subfunds(blocks, subfunds):
    inv_line = True
    invs = {}
    invs_blks = {}
    current_inv = None
    current_funds = None
    fund_line = False
    for rb in blocks:
        r = rb.content.strip()
        if inv_line:
            current_inv = r
            invs_blks[r] = rb
            inv_line = False
        elif r == "":
            inv_line = True
        elif r.startswith("("):
            current_fund = r.replace("(", "")
            fund_line = True
            if r.endswith(")"):
                invs[current_inv] = set(
                    (
                        MatchFund(name=s.strip())
                        for s in current_fund.replace(")", "").split(",")
                    )
                )
                fund_line = False

        else:
            if fund_line:
                current_fund += " " + r
                if r.endswith(")"):
                    invs[current_inv] = set(
                        (
                            MatchFund(name=s.strip())
                            for s in current_fund.replace(")", "").split(",")
                        )
                    )
                    fund_line = False
    res = []
    for i, s in invs.items():
        if not s.isdisjoint(subfunds):
            res.append(
                TextBlock.from_content(
                    TipiBlocco.INV, {"funds": set([f.name for f in s])}, i
                )
            )
            res.extend(
                [
                    TextBlock.from_content(ResultStandardFiltering.FUND, {}, f.name)
                    for f in s
                    if f not in subfunds
                ]
            )
    return res


def text_filter(blocks, results):
    funds = set(
        map(
            lambda x: MatchFund(x.name),
            filter(lambda x: isinstance(x, output.Fund), results),
        )
    )
    return text_filter_with_subfunds(blocks, funds)


def text_filter_begin_page(blocks, results):
    filter_funds = set(
        map(
            lambda x: MatchFund(x.name),
            filter(lambda x: isinstance(x, output.Fund), results),
        )
    )
    inv_managers = list(
        filter(lambda x: isinstance(x, output.InvestmentsManager), results)
    )
    a_subfunds = set([f for inv in inv_managers for f in inv.managed_funds])
    residual_funds = filter_funds - a_subfunds

    inv_blocks = [blk for blk in blocks if blk.type_block == TipiBlocco.INV]
    manco_blocks = [blk for blk in blocks if blk.type_block == TipiBlocco.MAN]

    res_inv = text_filter_with_subfunds(inv_blocks, filter_funds)
    res_manco = text_filter_with_subfunds(manco_blocks, filter_funds)
    additional_a_subfunds = set(
        [
            MatchFund(name=s)
            for inv in res_inv
            if isinstance(inv, output.InvestmentsManager)
            for s in inv.metadata["funds"]
        ]
    )
    additional_manco_subfunds = set(
        [
            MatchFund(name=s)
            for inv in res_manco
            if isinstance(inv, output.InvestmentsManager)
            for s in inv.metadata["funds"]
        ]
    )

    funds_manco = residual_funds - additional_a_subfunds - additional_manco_subfunds

    res = res_inv
    res.extend(res_manco)
    res.extend([TextBlock(TipiBlocco.MAN, r.metadata, r.content) for r in res_manco])
    res.append(
        TextBlock(
            TipiBlocco.INV,
            {"funds": set([f.name for f in funds_manco])},
            manco_blocks[0],
        )
    )
    res.append(
        StandardManagmentCompanyTextBlock(
            manco_blocks[0],
            filter_funds.union(additional_a_subfunds).union(additional_manco_subfunds),
        )
    )
    return res


def deserialize(text_block):
    if text_block.type_block == TipiBlocco.INV:
        return output.InvestmentsManager(
            name=text_block.content, managed_funds=text_block.metadata["funds"]
        )


deserialize_manco = DeserializerManagmentCompanyStandard()

deserialize_fund = DeserializerFundStandard()
