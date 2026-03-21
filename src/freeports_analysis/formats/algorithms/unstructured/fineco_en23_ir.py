"""Custom pdf filter for FINECO-EN23[IR] format"""

from freeports_analysis.formats.utils.pdf_extract import PdfExtractInvestmentsStandard
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
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

tnrb = PdfLineSelection.font("TimesNewRoman,Bold")

subfund_set = (
    PdfLineSelection.font_size(9.95, 10.03)
    & PdfLineSelection.area_from_bounds(
        x0=0.0,
        x1=1e6,
        y0=PdfLineSelection.text("Condensed Schedule of Investments") & tnrb,
        y1=PdfLineSelection.text("Domicile") & tnrb,
    )
    & tnrb
)

currency_set = (
    PdfLineSelection.area_from_movewindow(
        target=PdfLineSelection.text("Fair Value") & tnrb,
        vec=(0.0, 1.0),
        width_mult=1.2,
        height_mult=1.2,
    )
    & tnrb
)

body_set = (
    (PdfLineSelection.font("TimesNewRoman") | tnrb)
    & PdfLineSelection.font_size(9.95, 10.03)
    & PdfLineSelection.area_from_bounds(
        x0=135.0,
        x1=1e6,
        y0=185.0,
        y1=(
            PdfLineSelection.text("SWAPS")
            | PdfLineSelection.text("FORWARDS")
            | PdfLineSelection.text("FEATURES")
        )
        & tnrb,
    )
    / PdfLineSelection.text("-$")
) & PdfLineSelection.area(0.0, 0.0, 1e6, 750)


class BlockType(Enum):
    MANCO = auto()
    INV_MAN = auto()


def pdf_filter_manco(page):
    lines = pdflines_from_pagedict(page)
    b = (
        PdfLineSelection(font="timesnewromanbold", text="Investment Manager")
        .select(lines)[0]
        .bbox[1]
    )
    t = (
        PdfLineSelection(font="timesnewromanbold", text="^Manager")
        .select(lines)[0]
        .bbox[1]
        - 10.0
    )
    txt = (
        PdfLineSelection(font="timesnewroman", area=(0.0, t, 1e6, b))
        .select(lines)[0]
        .text
    )
    return [PdfBlock(BlockType.MANCO, {}, txt)]


def pdf_filter_inv_man(page):
    lines = pdflines_from_pagedict(page)
    s = (
        PdfLineSelection(font="timesnewroman", font_size=(10.0, 10.1))
        & PdfLineSelection.area_from_bounds(
            x0=0.0,
            y0=PdfLineSelection(
                font="timesnewromanbold",
                font_size=(10.0, 10.1),
                text="Date of Commencement",
            ),
            x1=1e6,
            y1=520.0,
        )
    ).select(lines)
    coord = get_table_coordinates(s)
    blks = [
        PdfBlock(BlockType.INV_MAN, {"table-row": cs[0], "table-col": cs[1]}, l.text)
        for cs, l in zip(coord, s)
    ]
    return blks


def text_extract_manco(blks, filter_data):
    i_subfunds = set([i.subfund for i in filter_data if isinstance(i, Investment)])
    a_subfunds = set(
        [
            s
            for a in filter_data
            if isinstance(a, InvestmentsManager)
            for s in a.managed_funds
        ]
    )
    return [
        TextBlock.from_content(
            BlockType.MANCO, {"funds": i_subfunds.union(a_subfunds)}, blks[0].content
        ),
        TextBlock.from_content(
            BlockType.INV_MAN, {"funds": i_subfunds - a_subfunds}, blks[0].content
        ),
    ]


def text_extract_inv_man(blks, filter_data):
    subfunds = set(
        [normalize_string(i.subfund) for i in filter_data if isinstance(i, Investment)]
    )
    funds = [b.content for b in blks if b.metadata["table-col"] == 0]
    inv_man = [b.content for b in blks if b.metadata["table-col"] == 2]
    inv_managers = {}
    for f, i in zip(funds, inv_man):
        if i not in inv_managers:
            inv_managers[i] = [f]
        else:
            inv_managers[i].append(f)
    return [
        TextBlock.from_content(BlockType.INV_MAN, {"funds": funds}, i)
        for i, funds in inv_managers.items()
    ]


def deserialize_manco(blk):
    if blk.type_block == BlockType.INV_MAN:
        return InvestmentsManager(name=blk.content, managed_funds=blk.metadata["funds"])
    else:
        return ManagementCompany(name=blk.content, managed_funds=blk.metadata["funds"])


def deserialize_inv_man(blk):
    return InvestmentsManager(name=blk.content, managed_funds=blk.metadata["funds"])


pipelines = {
    "investments": Pipeline(
        pdf_extract=PdfExtractInvestmentsStandard(
            subfund_set=subfund_set, currency_set=currency_set, body_set=body_set
        )
    ),
    "inv_managers_table": Pipeline(
        pdf_filter_inv_man, text_extract_inv_man, deserialize_inv_man
    ),
    "manco": Pipeline(pdf_filter_manco, text_extract_manco, deserialize_manco),
}
