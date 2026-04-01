"""Custom pdf filter for FINECO-EN23[IR] format"""

from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractFundStandard,
    PdfExtractManagmentCompanyStandard,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
)
from freeports_analysis.formats.utils.text_filter import (
    ResultStandardFiltering,
    StandardManagmentCompanyTextBlock,
)
from freeports_analysis.formats.utils.deserialize import (
    DeserializerManagmentCompanyStandard,
)
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.algorithms import PdfBlock, TextBlock
from freeports_analysis import output
from freeports_analysis.formats.utils.text_filter import match
from enum import Enum, auto


class BlockType(Enum):
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
    std_pdf_filter = PdfExtractManagmentCompanyStandard(
        PdfLineSelection(font="timesnewroman", area=(0.0, t, 1e6, b))
    )
    return std_pdf_filter(page)


def text_extract(blks, filter_data):
    inv_funds = set(
        match.MatchFund(name=n.fund)
        for n in filter(lambda x: isinstance(x, output.Investment), filter_data)
    )
    a_funds = set(
        match.MatchFund(name=n)
        for inv in filter(
            lambda x: isinstance(x, output.InvestmentsManager), filter_data
        )
        for n in inv.managed_funds
    )
    return [
        StandardManagmentCompanyTextBlock(blks[0], inv_funds.union(a_funds)),
        TextBlock.from_content(
            BlockType.INV_MAN,
            {"funds": set(f.name for f in (inv_funds - a_funds))},
            blks[0].content,
        ),
    ]


deserialize = DeserializerManagmentCompanyStandard()
