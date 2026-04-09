"""KAIROS-EN23 format submodule"""

import re
from freeports_analysis.formats.utils.text_filter import (
    TextFilterInvestmentsStandard,
    TextFilterManagmentCompanyStandard,
    ResultStandardFiltering,
    OneTextBlockType,
)
from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractManagmentCompanyStandard,
    OnePdfBlockType,
)
from freeports_analysis.formats.utils.deserialize import (
    DeserializerManagmentCompanyStandard,
    to_int_en_month,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract.select_position import get_groups
from freeports_analysis.formats.utils.text_filter.match import MatchFund
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats import TextBlock, PdfBlock
from freeports_analysis.output import FundRename, FundMerge, Fund
import datetime

market_value_regex = re.compile(r"(([0-9]+,)?[0-9]+,?[0-9]+\.[0-9]{2}) ")
# non sono sicuro di come ho riscritto questa regex e a cosa servivano le parentesi

std = TextFilterInvestmentsStandard(
    nominal_quantity_pos=0,
    perc_net_assets_pos=3,
    acquisition_currency_pos=1,
    market_value_pos=2,
)


def text_filter(pdf_blks, target_companies):
    """
    Text extract that extract quantity from the name of the company (is conained in the same cell)
    """
    txt_blks = std(pdf_blks, target_companies)
    for txt_blk in txt_blks:
        if (
            txt_blk.type_block == ResultStandardFiltering.BOND_TARGET
            or txt_blk.type_block == ResultStandardFiltering.EQUITY_TARGET
        ):
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


merges_regex = re.compile("([^,]+ [0-9]+, [0-9]+), ([^*]+)\*? merged into ([^*,]+)")


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
        current_name = MatchFund(m.group(3))
        if current_name in funds:
            old_name = m.group(2)
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


pipelines = {
    "investments": Pipeline(text_filter=text_filter),
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
        DeserializerManagmentCompanyStandard(),
    ),
    "renames": Pipeline(pdf_extract_rename, text_filter_rename, deserialize_rename),
    "merges": Pipeline(pdf_extract_merges, text_filter_merges, deserialize_merges),
}
