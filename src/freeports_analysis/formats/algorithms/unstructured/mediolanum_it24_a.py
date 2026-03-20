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
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    TableConfig,
    ColumnConfig,
    get_table_coordinates,
    TablePosAlgorithm,
)
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


l = 190.0
r = 1e6
table_cfg = TableConfig(ColumnConfig(limits=(l, r)))


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


def pdf_extract_inv_managers_begin(page):
    lines = pdflines_from_pagedict(page)
    t = PdfLineSelection.text("^INVESTMENT ").select(lines)[0].bbox[1]
    manco_selection = PdfLineSelection.area(l, 70.0, r, t) / deselection
    b = 705.0
    body_selection = PdfLineSelection.area(l, t, r, b) / deselection
    body = body_selection.select(lines)
    manco = manco_selection.select(lines)
    res = pdf_extract_body(manco, TipiBlocco.MAN)
    res.extend(pdf_extract_body(body))
    return res


def pdf_extract_inv_managers(page):
    lines = pdflines_from_pagedict(page)
    t = 70.0
    b = 705.0
    body_selection = PdfLineSelection.area(l, t, r, b) / deselection
    body = body_selection.select(lines)
    return pdf_extract_body(body)


def pdf_extract_inv_managers_end(page):
    lines = pdflines_from_pagedict(page)
    t = 70.0
    b = PdfLineSelection.text("^BANCA ").select(lines)[0].bbox[1]
    body_selection = PdfLineSelection.area(l, t, r, b) / deselection
    body = body_selection.select(lines)
    return pdf_extract_body(body)


def text_filter_inv_managers_with_subfunds(blocks, investments_subfunds):
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
                invs[current_inv] = [
                    s.strip() for s in current_fund.replace(")", "").split(",")
                ]
                fund_line = False

        else:
            if fund_line:
                current_fund += " " + r
                if r.endswith(")"):
                    invs[current_inv] = [
                        s.strip() for s in current_fund.replace(")", "").split(",")
                    ]
                    fund_line = False

    return [
        TextBlock(TipiBlocco.INV, {"funds": funds}, invs_blks[i])
        for i, funds in invs.items()
        if any(normalize_string(f) in investments_subfunds for f in funds)
    ]


def text_filter_inv_managers(blocks, investments):
    subfunds = set([normalize_string(i.subfund) for i in investments])
    return text_filter_inv_managers_with_subfunds(blocks, subfunds)


def text_filter_inv_managers_begin(blocks, results):
    i_subfunds = set([i.subfund for i in results if isinstance(i, Investment)])
    investments = [i for i in results if isinstance(i, Investment)]
    i_subfunds_n = [normalize_string(s) for s in i_subfunds]
    a_subfunds = set(
        [s for a in results if isinstance(a, AssetsManager) for s in a.managed_funds]
    )

    inv_blocks = [blk for blk in blocks if blk.type_block == TipiBlocco.INV]
    manco_blocks = [blk for blk in blocks if blk.type_block == TipiBlocco.MAN]

    res_inv = text_filter_inv_managers_with_subfunds(inv_blocks, i_subfunds)
    res_manco = text_filter_inv_managers_with_subfunds(manco_blocks, i_subfunds)
    additional_a_subfunds = set([s for inv in res_inv for s in inv.metadata["funds"]])
    additional_manco_subfunds = set(
        [s for inv in res_manco for s in inv.metadata["funds"]]
    )

    funds_manco = list(
        i_subfunds - a_subfunds - additional_a_subfunds - additional_manco_subfunds
    )

    res = res_inv
    res.extend(res_manco)
    res.extend([TextBlock(TipiBlocco.MAN, r.metadata, r.content) for r in res_manco])
    res.append(
        TextBlock(
            TipiBlocco.INV,
            {
                "funds": list(
                    i_subfunds
                    - a_subfunds
                    - additional_a_subfunds
                    - additional_manco_subfunds
                )
            },
            manco_blocks[0],
        )
    )
    res.append(
        TextBlock(
            TipiBlocco.MAN,
            {"funds": list(i_subfunds.union(additional_a_subfunds).union(a_subfunds))},
            manco_blocks[0],
        )
    )
    return res


def deserialize_inv_managers(text_block):
    if text_block.type_block == TipiBlocco.INV:
        return InvestmentsManager(
            name=text_block.content, managed_funds=text_block.metadata["funds"]
        )
    else:
        return ManagementCompany(
            name=text_block.content, managed_funds=text_block.metadata["funds"]
        )


pipelines = {
    "inv_managers_begin": Pipeline(
        pdf_extract=pdf_extract_inv_managers_begin,
        text_filter=text_filter_inv_managers_begin,
        deserialize=deserialize_inv_managers,
    ),
    "inv_managers": Pipeline(
        pdf_extract=pdf_extract_inv_managers,
        text_filter=text_filter_inv_managers,
        deserialize=deserialize_inv_managers,
    ),
    "inv_managers_end": Pipeline(
        pdf_extract=pdf_extract_inv_managers_end,
        text_filter=text_filter_inv_managers,
        deserialize=deserialize_inv_managers,
    ),
}
