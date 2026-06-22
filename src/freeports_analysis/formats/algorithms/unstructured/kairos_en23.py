"""KAIROS-EN23 format submodule"""

import re
from freeports_analysis.formats.utils.text_filter import (
    TextFilterInvestmentsStandard,
    TextFilterManagmentCompanyStandard,
    ResultStandardFiltering,
    OneTextBlockType,
    TextFilterAssetsStandard,
)
from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractManagmentCompanyStandard,
    OnePdfBlockType,
    PdfExtractAssetsStandard,
)
from freeports_analysis.formats.utils.deserialize import (
    DeserializerManagmentCompanyStandard,
    DeserializeAssetsStandard,
    to_int_en_month,
    to_float,
    DeserializerInvestmentsManagerFromManco,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract import PdfExtractSfdrArticleStandard
from freeports_analysis.formats.utils.text_filter import TextFilterSfdrArticleStandard
from freeports_analysis.formats.utils.deserialize import DeserializeSfdrArticleStandard
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_groups,
    get_table_coordinates,
    TablePosAlgorithm,
)
from freeports_analysis.formats.utils.text_filter.match import MatchFund
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats import TextBlock, PdfBlock
from freeports_analysis.output import (
    FundRename,
    FundMerge,
    Fund,
    FundSfdrClassification,
    FundEsgIndicator,
    Investment,
)
from freeports_analysis.consts import SfdrArticle
import datetime

market_value_regex = re.compile(r"(([0-9]+,)?[0-9]+,?[0-9]+\.[0-9]{2}) ")
# non sono sicuro di come ho riscritto questa regex e a cosa servivano le parentesi

std = TextFilterInvestmentsStandard(
    nominal_quantity_pos=0,
    perc_net_assets_pos=3,
    acquisition_currency_pos=1,
    market_value_pos=2,
)

remove_fund_regex = (re.compile("\\(.*\\)"),)
remove_fund_substr = ("*",)


def remove_fund_excess(txt):
    for r in remove_fund_regex:
        txt = r.sub("", txt)
    for r in remove_fund_substr:
        txt = txt.replace(r, "")
    return txt


def text_filter(pdf_blks, target_companies):
    """
    Text extract that extract quantity from the name of the company (is conained in the same cell)
    """
    txt_blks = std(pdf_blks, target_companies)
    for txt_blk in txt_blks:
        if txt_blk.type_block == ResultStandardFiltering.FUND:
            txt_blk.content = remove_fund_excess(txt_blk.content)
        elif (
            txt_blk.type_block == ResultStandardFiltering.BOND_TARGET
            or txt_blk.type_block == ResultStandardFiltering.EQUITY_TARGET
        ):
            txt_blk.metadata["fund"] = remove_fund_excess(txt_blk.metadata["fund"])
            c = txt_blk.content
            m = market_value_regex.match(c)
            txt_blk.metadata |= {"quantity": m[0]}
    return txt_blks


def pdf_extract_rename(page):
    lines = pdflines_from_pagedict(page)
    renames = PdfLineSelection.area_from_movewindow(
        PdfLineSelection.text("has changed its name"), (-0.1, 0.0), 1.2, 2.5
    ).select(lines)
    rename = "".join((r.text for r in renames))
    return [PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {}, rename)]


def pdf_extract_merges(page):
    lines = pdflines_from_pagedict(page)
    body = (
        PdfLineSelection.area_from_bounds(
            x0=0.0,
            y0=PdfLineSelection.text("merged during the year"),
            x1=1e6,
            y1=PdfLineSelection.text("Fund’s shares"),
        )
        / PdfLineSelection.text("^ $")
    ).select(lines)
    groups = get_groups(body, 20)
    merges = "".join((m.text for g, m in zip(groups, body) if g == 0))
    return [PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {}, merges)]


rename_regex = re.compile(
    "As at (.+ [0-9]+, [0-9]+), the Sub-Fund ([^*]+) has changed .+ in ([^*]+)"
)


def text_filter_rename(pdf_blks, filter_data):
    funds = set(
        map(
            lambda x: MatchFund(name=x.name),
            filter(lambda x: isinstance(x, Fund), filter_data),
        )
    )
    m = rename_regex.match(pdf_blks[0].content)
    current_name = MatchFund(m.group(3))
    if current_name in funds:
        return [
            TextBlock(
                OneTextBlockType.RELEVANT_BLOCK,
                {
                    "old_name": m.group(2),
                    "current_name": current_name.name,
                    "date": m.group(1),
                },
                pdf_blks[0],
            )
        ]
    else:
        return []


merges_regex = re.compile(r"([^,]+ [0-9]+, [0-9]+), ([^*]+)\*? merged into ([^*,]+)")


def text_filter_merges(pdf_blks, filter_data):
    funds = set(
        map(
            lambda x: MatchFund(name=x.name),
            filter(lambda x: isinstance(x, Fund), filter_data),
        )
    )
    merges = pdf_blks[0].content.split("- On ")[1:]
    res = []
    for mrg in merges:
        m = merges_regex.match(mrg)
        current_name = MatchFund(remove_fund_excess(m.group(3)))
        if current_name in funds:
            old_name = remove_fund_excess(m.group(2))
            date = m.group(1)
            res.append(
                TextBlock(
                    OneTextBlockType.RELEVANT_BLOCK,
                    {
                        "old_name": old_name,
                        "current_name": current_name.name,
                        "date": date,
                    },
                    pdf_blks[0],
                )
            )
    return res


def to_date(txt):
    parts = txt.replace(",", "").split()
    return datetime.date(int(parts[2]), to_int_en_month(parts[0]), int(parts[1]))


def deserialize_rename(txt_blk):
    md = txt_blk.metadata
    return FundRename(
        old_name=md["old_name"],
        current_name=md["current_name"],
        date=to_date(md["date"]),
    )


def deserialize_merges(txt_blk):
    md = txt_blk.metadata
    return FundMerge(
        old_name=md["old_name"],
        current_name=md["current_name"],
        date=to_date(md["date"]),
    )


condition_text = PdfLineSelection(
    font="arialnarrow-bold", font_size=(11.9, 12.1)
) & PdfLineSelection(
    text="Statement of Net Assets as at",
    font_size=(11.9, 12.1),
    font="arialnarrow-bold",
)

fund_curr_set = PdfLineSelection(
    font="arialnarrow-bold", font_size=(11.9, 12.1)
) & PdfLineSelection.area_from_bounds(x0=0, y0=0, x1=1e6, y1=condition_text)

currency_set_assets = PdfLineSelection(
    font="arialnarrow-bold", font_size=(11.9, 12.1), text="(in"
)


pdf_extract_assets = PdfExtractAssetsStandard(
    fund_set=fund_curr_set,
    currency_set=currency_set_assets,
    tot_assets_set=PdfLineSelection(
        font="arialnarrow-bold", font_size=(7.9, 8.1), text="^Total assets"
    ),
    liabilities_set=PdfLineSelection(
        font="arialnarrow-bold", font_size=(7.9, 8.1), text="^Total liabilities"
    ),
    net_assets_set=PdfLineSelection(
        font="arialnarrow-bold",
        font_size=(7.9, 8.1),
        text="^Total liabilities",  # "^Net assets at the end of the year"
    ),
    tot_assets_vec=(1.2, 0.0),
    liabilities_vec=(1.2, 0.0),
    net_assets_vec=(1.2, 1.2),
    tot_assets_mult=(50.0, 1.02),
    liabilities_mult=(50.0, 1.02),
    net_assets_mult=(5.0, 2.0),
)

text_filter_assets = TextFilterAssetsStandard(
    remove_from_fund_regexes=("\\(.*\\)", "\\*")
)
deserialize_assets = DeserializeAssetsStandard(num_converter=to_float)


def esg_indicators_pdf_extact_art8(page):
    lines = pdflines_from_pagedict(page)
    r = PdfLineSelection.text("RATING").select(lines)
    if len(r) == 0:
        return []
    l = PdfLineSelection.area_from_bounds(
        PdfLineSelection.text("Characteristics promoted"),
        PdfLineSelection.text("Indicator"),
        1e6,
        PdfLineSelection.text("RATING"),
    ).select(lines)
    f = PdfLineSelection.text("Product name: ").select(lines)
    rows, cols = zip(
        *get_table_coordinates(
            l,
            algorithm_flags=TablePosAlgorithm.USE_RULER_AREA
            | TablePosAlgorithm.BIG_CELL_RULE,
            tolerance=-0.2,
            collapse=True,
        )
    )
    # nrows=max(rows)+1
    # ncols=max(cols)+1
    # blks=[PdfBlock(OnePdfBlockType.RELEVANT_BLOCK,{"table-row":r,"table-col":c},ll.text) for ll,(r,c) in zip(l,cc)]
    # values=[ll.text for ll,r,c in zip(l,rows,cols) for row in range(n_rows) if c==1 and row==r]

    res = []
    for row in sorted(set(rows)):
        key = "".join(
            (ll.text for r, c, ll in zip(rows, cols, l) if row == r and c == 0)
        ).strip()
        value = "".join(
            (ll.text for r, c, ll in zip(rows, cols, l) if row == r and c == 1)
        ).strip()

        res.append((key, value))
    return [PdfBlock(OnePdfBlockType, {k: v for k, v in res}, f[0].text)]


def esg_indicators_text_filter_art8(pdf_blks, filter_data):
    if len(pdf_blks) == 0:
        return []
    blk = next(iter(pdf_blks))
    fund_name = blk.content.removeprefix("Product name: ")
    filter_funds = set(
        MatchFund(name=n.fund)
        for n in filter(lambda x: isinstance(x, Investment), filter_data)
    )
    fund = MatchFund(name=fund_name)
    if fund in filter_funds:
        return [
            TextBlock.from_content(OnePdfBlockType, {"index": k, "fund": fund_name}, v)
            for k, v in blk.metadata.items()
        ]
    else:
        return []


def esg_indicators_deserialize_art8(txt_blk):
    return FundEsgIndicator(
        name=txt_blk.metadata["index"],
        value=txt_blk.content,
        fund=txt_blk.metadata["fund"],
    )


pipelines = {
    "investments": Pipeline(text_filter=text_filter),
    "fund_assets": Pipeline(pdf_extract_assets, text_filter_assets, deserialize_assets),
    "manco": Pipeline(
        PdfExtractManagmentCompanyStandard(
            PdfLineSelection.area_from_movewindow(
                PdfLineSelection(font="arialnarrow-bold", text="Management Company"),
                (-0.1, 0.5),
                10.0,
                1.8,
            )
            / PdfLineSelection.text("^ $")
        ),
        TextFilterManagmentCompanyStandard(),
        (
            DeserializerManagmentCompanyStandard(),
            DeserializerInvestmentsManagerFromManco(),
        ),
    ),
    "sfdr": Pipeline(
        PdfExtractSfdrArticleStandard(
            PdfLineSelection.text("Disclosure pursuant to Article 9"),
            PdfLineSelection.text("Disclosure pursuant to Article 8"),
            PdfLineSelection.text("Product name: "),
        ),
        TextFilterSfdrArticleStandard("Product name: "),
        DeserializeSfdrArticleStandard(),
    ),
    "esg": Pipeline(
        esg_indicators_pdf_extact_art8,
        esg_indicators_text_filter_art8,
        esg_indicators_deserialize_art8,
    ),
    "renames": Pipeline(pdf_extract_rename, text_filter_rename, deserialize_rename),
    "merges": Pipeline(pdf_extract_merges, text_filter_merges, deserialize_merges),
}
