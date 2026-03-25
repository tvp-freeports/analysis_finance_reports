"""Custom pdf filter for FINECO-EN23[IR] format"""

from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractFundStandard,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
)
from freeports_analysis.formats.utils.text_filter import ResultStandardFiltering
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.algorithms import PdfBlock, TextBlock
from freeports_analysis.output import (
    ManagementCompany,
    InvestmentsManager,
    Investment,
    AssetsManager,
    Fund,
)
from enum import Enum, auto


class BlockType(Enum):
    MANCO = auto()
    INV_MAN = auto()


def pdf_filter(page):
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


def text_extract(blks, filter_data):
    inv_funds = set(
        Fund(name=n.fund)
        for n in filter(lambda x: isinstance(x, Investment), filter_data)
    )
    a_funds = set(
        Fund(name=n)
        for inv in filter(lambda x: isinstance(x, InvestmentsManager), filter_data)
        for n in inv.managed_funds
    )
    return [
        TextBlock.from_content(
            BlockType.MANCO,
            {"funds": set(f.name for f in inv_funds.union(a_funds))},
            blks[0].content,
        ),
        TextBlock.from_content(
            BlockType.INV_MAN,
            {"funds": set(f.name for f in (inv_funds - a_funds))},
            blks[0].content,
        ),
    ]


def deserialize(blk):
    if blk.type_block == BlockType.INV_MAN:
        return InvestmentsManager(name=blk.content, managed_funds=blk.metadata["funds"])
    elif blk.type_block == BlockType.MANCO:
        return ManagementCompany(name=blk.content, managed_funds=blk.metadata["funds"])
