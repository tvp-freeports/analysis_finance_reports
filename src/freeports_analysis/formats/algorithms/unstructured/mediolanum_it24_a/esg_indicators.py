from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.algorithms import PdfBlock, TextBlock
from freeports_analysis.formats.utils.pdf_extract import OnePdfBlockType
from freeports_analysis.formats.utils.text_filter import (
    OneTextBlockType,
    investment_fund_filter_data,
)
from freeports_analysis.match import MatchFund
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
    ColumnConfig,
    SplittingState,
    RowConfig,
    TableConfig,
    TablePosAlgorithm,
)
from freeports_analysis.output import FundEsgIndicator
import re


def pdf_extract(page):
    lines = pdflines_from_pagedict(page)
    right_section = PdfLineSelection.area_from_bounds(
        0.0,
        PdfLineSelection.text("stata la prestazione degli indicatori"),
        1e6,
        PdfLineSelection.text("rispetto ai periodi precedenti"),
    )
    table_lines = (
        PdfLineSelection.area_from_bounds(
            130,
            PdfLineSelection.text("per il")
            & right_section
            & PdfLineSelection.area_from_movewindow(
                PdfLineSelection.text("4° T.") & right_section, (1.0, -0.1), 5.0, 6.0
            ),
            1e6,
            PdfLineSelection(text="rispetto ai periodi precedenti"),
        )
        / (
            PdfLineSelection.text("^ $")
            | PdfLineSelection.text("^  $")
            | PdfLineSelection.area(0.0, 780, 1e6, 1e6)
        )
    ).select(lines)
    f = PdfLineSelection.text("Nome del prodotto:").select(lines)
    rows, cols = zip(
        *get_table_coordinates(
            table_lines,
            algorithm_flags=TablePosAlgorithm.USE_RULER_AREA
            | TablePosAlgorithm.BIG_CELL_RULE,
            tolerance=0.0,
            collapse=True,
        )
    )

    res = []
    for row in sorted(set(rows)):
        key = " ".join(
            (
                table_lines.text
                for r, c, table_lines in zip(rows, cols, table_lines)
                if row == r and c == 0
            )
        ).strip()
        value = " ".join(
            (
                table_lines.text
                for r, c, table_lines in zip(rows, cols, table_lines)
                if row == r and c == 1
            )
        ).strip()
        res.append((key, value))
    return [PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {k: v for k, v in res}, f[0].text)]


suffix_regex = re.compile(r" \(.*Comparto.*\),.*")
prefix = "Nome del prodotto: "


@investment_fund_filter_data
def text_filter(pdf_blks, filter_funds):
    if len(pdf_blks) == 0:
        return []
    blk = next(iter(pdf_blks))
    fund_name = suffix_regex.sub("", blk.content).removeprefix(prefix)
    fund = MatchFund(name=fund_name)
    if fund in filter_funds:
        return [
            TextBlock.from_content(
                OnePdfBlockType.RELEVANT_BLOCK, {"index": k, "fund": fund_name}, v
            )
            for k, v in blk.metadata.items()
        ]
    else:
        return []


def deserialize(txt_blk):
    return FundEsgIndicator(
        name=txt_blk.metadata["index"],
        value=txt_blk.content,
        fund=txt_blk.metadata["fund"],
    )
