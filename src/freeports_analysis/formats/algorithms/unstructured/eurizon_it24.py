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
from freeports_analysis.formats.utils.pdf_extract import PdfExtractSfdrArticleStandard
from freeports_analysis.formats.utils.text_filter import (
    TextFilterSfdrArticleStandard,
    OneTextBlockType,
    investment_fund_filter_data,
)
from freeports_analysis.formats.utils.deserialize import DeserializeSfdrArticleStandard
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_groups,
    get_table_coordinates,
    TablePosAlgorithm,
)
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
from freeports_analysis.consts import Currency, Promise, SfdrArticle
from freeports_analysis.match import MatchFund
from freeports_analysis.formats import PageParseFail, PdfBlock, TextBlock
from freeports_analysis.output import (
    Fund,
    FundMerge,
    FundRename,
    FundSfdrClassification,
    FundEsgIndicator,
)
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
    r'Il fondo "(.+)" \(già denominato (.+)\) è stato istituito'
)
regex_rename = re.compile(r'"(.+)" fino al ([0-9]+ [a-z]+ [0-9]+)')
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


def sfdr_pdf_extract_1(page):
    lines = pdflines_from_pagedict(page)
    fund_name = next(iter(PdfLineSelection.text("Nome prodotto: ").select(lines))).text
    return [PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {}, fund_name)]


@investment_fund_filter_data
def sfdr_text_filter_1(pdf_blks, investment_funds):
    fund_name = next(iter(pdf_blks)).content
    fund_name = fund_name.replace("Nome prodotto: ", "")
    fund = MatchFund(name=fund_name)
    if fund in investment_funds:
        return [TextBlock.from_content(OneTextBlockType.RELEVANT_BLOCK, {}, fund_name)]
    else:
        return []


def sfdr_deserialize_1(txt_blk):
    return FundSfdrClassification(article=Promise("sfdr-article"), fund=txt_blk.content)


def esg_indicators_deserialize_fund(txt_blk):
    return {"esg-indicator-fund": txt_blk.content}


def sfdr_pdf_extract_2(page):
    lines = pdflines_from_pagedict(page)
    art = SfdrArticle.ART_6
    if (
        len(
            PdfLineSelection.text("obiettivo di investimento sostenibile").select(lines)
        )
        > 0
    ):
        art = SfdrArticle.ART_9
    elif (
        len(
            PdfLineSelection.text(
                "soddisfatte le caratteristiche ambientali e/o sociali promosse"
            ).select(lines)
        )
        > 0
    ):
        art = SfdrArticle.ART_8
    return [PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {"article": art}, "")]


def sfdr_text_filter_2(pdf_blks, _):
    blk = next(iter(pdf_blks))
    return [TextBlock(OneTextBlockType.RELEVANT_BLOCK, blk.metadata, blk)]


def sfdr_deserialize_2(txt_blk):
    return {"sfdr-article": txt_blk.metadata["article"]}


def esg_indicators_pdf_extract(page):
    lines = pdflines_from_pagedict(page)
    table_lines = (
        PdfLineSelection.area_from_bounds(
            0.0,
            PdfLineSelection.text("stata la prestazione degli indicatori di sostenibi"),
            1e6,
            PdfLineSelection.text("il prodotto finanziario promuove l'interazione"),
        )
        / (
            PdfLineSelection.text("^ $")
            | PdfLineSelection.text("^  $")
            | PdfLineSelection.area(0.0, 780, 1e6, 1e6)
        )
    ).select(lines)
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
    for row in sorted(set(rows))[1:]:
        key = " ".join(
            (
                table_lines.text
                for r, c, table_lines in zip(rows, cols, table_lines)
                if row == r and c == 1
            )
        ).strip()
        value = " ".join(
            (
                table_lines.text
                for r, c, table_lines in zip(rows, cols, table_lines)
                if row == r and c == 2
            )
        ).strip()
        res.append((key, value))
    return [PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {k: v for k, v in res}, "")]


def esg_indicators_text_filter(pdf_blks, _):
    if len(pdf_blks) == 0:
        return []
    blk = next(iter(pdf_blks))
    m = blk.metadata
    return [
        TextBlock(OneTextBlockType.RELEVANT_BLOCK, {"key": k, "value": v}, blk)
        for k, v in m.items()
    ]


def esg_indicators_deserialize(txt_blk):
    m = txt_blk.metadata
    return FundEsgIndicator(
        fund=Promise(f"esg-indicator-fund"), name=m["key"], value=m["value"]
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
    "sfdr_page_1": Pipeline(
        sfdr_pdf_extract_1,
        sfdr_text_filter_1,
        (sfdr_deserialize_1, esg_indicators_deserialize_fund),
    ),
    "sfdr_page_2": Pipeline(sfdr_pdf_extract_2, sfdr_text_filter_2, sfdr_deserialize_2),
    "esg_indicators": Pipeline(
        esg_indicators_pdf_extract,
        esg_indicators_text_filter,
        esg_indicators_deserialize,
    ),
}
