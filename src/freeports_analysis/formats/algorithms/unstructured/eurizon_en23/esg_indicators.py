from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract import OnePdfBlockType, PdfBlock
from freeports_analysis.formats.utils.text_filter import (
    OneTextBlockType,
    investment_fund_filter_data,
)
from freeports_analysis.formats.utils.text_filter.match import MatchFund
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
    TablePosAlgorithm,
)
from freeports_analysis.consts import Promise
from freeports_analysis.formats import PdfBlock, TextBlock
from freeports_analysis.output import (
    FundSfdrClassification,
    FundEsgIndicator,
    Investment,
)
from freeports_analysis.match import MatchFund
import datetime
import re
from enum import Enum, auto


def get_page(lines):
    return int(PdfLineSelection.area(0.0, 780.0, 1e6, 1e6).select(lines)[0].text)


# def pdf_extract_sfdr_page_(page):
#     lines=pdflines_from_pagedict(page)
#     page=get_page(lines)


def pdf_extract(page):
    lines = pdflines_from_pagedict(page)
    page = get_page(lines)
    l = (
        PdfLineSelection.area_from_bounds(
            110,
            PdfLineSelection(font="arial-boldmt", text="perform?"),
            1e6,
            PdfLineSelection(font="arial-boldmt", text="compared")
            | PdfLineSelection.text("The fund also promoted"),
        )
    ).select(lines)
    if len(l) == 0:
        return []
    rows, cols = zip(
        *get_table_coordinates(
            l,
            algorithm_flags=TablePosAlgorithm.USE_RULER_AREA,
            tolerance=0,
            collapse=True,
        )
    )
    ex_cols = sorted(set(cols))
    ex_rows = sorted(set(rows))[1:]
    k_col = ex_cols[1]
    v_col = ex_cols[-1]
    m = {}
    for r in ex_rows:
        key = " ".join(
            (
                ll.text
                for ll, row, col in zip(l, rows, cols)
                if row == r and col == k_col
            )
        )
        value = " ".join(
            (
                ll.text
                for ll, row, col in zip(l, rows, cols)
                if row == r and col == v_col
            )
        )
        m[key] = value
    return [
        PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {"page": page, "indicators": m}, "")
    ]


def text_filter(pdf_blks, _):
    if len(pdf_blks) == 0:
        return []
    blk = next(iter(pdf_blks))
    m = blk.metadata
    return [
        TextBlock(
            OneTextBlockType.RELEVANT_BLOCK,
            {"page": m["page"], "key": k, "value": v},
            blk,
        )
        for k, v in m["indicators"].items()
    ]


def deserialize(txt_blk):
    m = txt_blk.metadata
    prev_page = m["page"] - 1
    return FundEsgIndicator(
        fund=Promise(f"esg-fund-page-{prev_page}"), name=m["key"], value=m["value"]
    )


def pdf_extract_fund(page):
    lines = pdflines_from_pagedict(page)
    page = get_page(lines)
    fund_blks = (
        PdfLineSelection.text("Product name")
        | PdfLineSelection.area_from_bounds(
            0.0,
            PdfLineSelection.text("Product name"),
            1e6,
            PdfLineSelection.text("Legal entity identifier"),
        )
    ).select(lines)
    fund = "".join(map(lambda sb: sb.text, sorted(fund_blks, key=lambda b: b.bbox[1])))
    return [PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {"page": page}, fund)]


@investment_fund_filter_data
def text_filter_fund(pdf_blks, investment_funds):
    blk = next(iter(pdf_blks))
    fund_name = blk.content.replace("Product name: ", "")
    fund = MatchFund(name=fund_name)
    if fund in investment_funds:
        return [
            TextBlock.from_content(
                OneTextBlockType.RELEVANT_BLOCK, blk.metadata, fund_name
            )
        ]
    else:
        return []


def deserialize_fund(txt_blk):
    page = txt_blk.metadata["page"]
    return {f"esg-fund-page-{page}": txt_blk.content}
