"""Custom pdf filter for EURIZON-IT24"""

from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractCurrencyConstant,
    PdfExtractFundStandard,
    ExtractTextPdfBlockOrFailPage,
    OnePdfBlockType,
)
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract.select_position import get_groups
from freeports_analysis.formats.utils.pdf_extract import OnePdfBlockType
from freeports_analysis.formats.utils.text_filter.match import MatchFund
from freeports_analysis.formats.utils.text_filter import (
    StandardManagmentCompanyTextBlock,
)
from freeports_analysis.formats.utils.deserialize import (
    DeserializerManagmentCompanyStandard,
    DeserializerInvestmentsManagerFromManco,
    to_date_with_it_month,
    deserialize_block_type,
)
from freeports_analysis.consts import Currency
from freeports_analysis.formats import PageParseFail, PdfBlock, TextBlock
from freeports_analysis.output import Fund, FundMerge, FundRename
import re
from enum import Enum, auto


class TypeChangeName(Enum):
    MERGING = auto()
    RENAMING = auto()


fund_set = PdfLineSelection(
    font="TrebuchetMSItalic", font_size=(4, 6.5), area=(270, 700, 595, 805)
)

body_set = PdfLineSelection.font("TrebuchetMS")

pdf_filter_manco = ExtractTextPdfBlockOrFailPage(
    PdfLineSelection.text("^La società di gestione"),
    "managment company",
    OnePdfBlockType.RELEVANT_BLOCK,
)

manco_regex = re.compile("gestione ([^,]+)")


def text_filter_manco(pdf_blks, filter_data):
    funds = set(
        map(
            lambda x: MatchFund(x.name),
            filter(lambda x: isinstance(x, Fund), filter_data),
        )
    )
    m = manco_regex.search(pdf_blks[0].content)
    found = None
    if m:
        found = m.group(1).strip()
    else:
        raise PageParseFail("Managment regex didn't matched anything")
    return [StandardManagmentCompanyTextBlock.from_name(found, funds)]


deselection_list = [
    PdfLineSelection(font="TrebuchetMS", text="Totale"),
    PdfLineSelection(font="TrebuchetMS", text="Altri strumenti finanziari"),
]


def pdf_extract_change_name(page):
    lines = pdflines_from_pagedict(page)
    body = PdfLineSelection.font("trebuchetms").select(lines)
    groups = get_groups(body, 10)
    text = (
        " ".join((b.text for g, b in zip(groups, body) if g == 0))
        .replace("”", '"')
        .replace("“", '"')
    )
    return [PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {}, text)]


regex_change_name = re.compile(
    'Il fondo "(.+)" \(già denominato (.+)\) è stato istituito'
)
regex_rename = re.compile('"(.+)" fino al ([0-9]+ [a-z]+ [0-9]+)')
regex_split_merges = re.compile("[iI]n data ")


def text_filter_change_name(pdf_blks, filter_data):
    funds = set(
        map(
            lambda x: MatchFund(x.name),
            filter(lambda x: isinstance(x, Fund), filter_data),
        )
    )
    text = pdf_blks[0].content
    m = regex_change_name.match(text)
    if not m:
        return []
    current_name = MatchFund(m.group(1))
    if current_name not in funds:
        return []

    rename = m.group(2).replace(" ed", ",").split(", ")[-1]
    m = regex_rename.match(rename)
    old_name_rename = m.group(1)
    date_rename = m.group(2)
    merges_text = text.partition("Il Fondo è operativo a partire dal")[2]
    merges = regex_split_merges.split(merges_text)[1:]

    res = [
        TextBlock(
            TypeChangeName.RENAMING,
            {
                "old_name": old_name_rename,
                "current_name": current_name.name,
                "date": date_rename,
            },
            pdf_blks[0],
        )
    ]
    for mrg in merges:
        mrg = mrg.replace("°", "")
        m = re.match("([0-9]+ .+ [0-9]+) ha incorporato il? fond[oi] (.+)", mrg)
        date_merge = m.group(1)
        tmp = m.group(2).split('"')
        for i in range(1, len(tmp), 2):
            res.append(
                TextBlock(
                    TypeChangeName.MERGING,
                    {
                        "old_name": tmp[i],
                        "current_name": current_name.name,
                        "date": date_merge,
                    },
                    pdf_blks[0],
                )
            )
    return res


@deserialize_block_type(TypeChangeName.RENAMING)
def deserialize_rename(txt_blk):
    md = txt_blk.metadata
    return FundRename(
        old_name=md["old_name"],
        current_name=md["current_name"],
        date=to_date_with_it_month(md["date"]),
    )


@deserialize_block_type(TypeChangeName.MERGING)
def deserialize_merge(txt_blk):
    md = txt_blk.metadata
    return FundMerge(
        old_name=md["old_name"],
        current_name=md["current_name"],
        date=to_date_with_it_month(md["date"]),
    )


pipelines = {
    "manco": Pipeline(
        pdf_extract=pdf_filter_manco,
        text_filter=text_filter_manco,
        deserialize=(
            DeserializerManagmentCompanyStandard(),
            DeserializerInvestmentsManagerFromManco(),
        ),
    ),
    "investments": Pipeline(
        pdf_extract=(
            PdfExtractInvestmentsStandard(
                body_set=body_set,
                deselection_list=deselection_list,
            ),
            PdfExtractFundStandard(fund_set),
            PdfExtractCurrencyConstant(Currency.EUR),
        )
    ),
    "merges": Pipeline(
        pdf_extract_change_name,
        text_filter_change_name,
        (deserialize_rename, deserialize_merge),
    ),
}
