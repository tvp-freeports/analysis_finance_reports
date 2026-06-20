from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.pdf_extract import OnePdfBlockType
from freeports_analysis.formats.utils.text_filter import OneTextBlockType
from freeports_analysis.formats.utils.text_filter.match import MatchFund
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    get_table_coordinates,
)
from freeports_analysis.formats import PdfBlock, TextBlock
from freeports_analysis.output import FundSfdrClassification, SfdrArticle, Investment
import datetime
import re
from enum import Enum, auto


def pdf_extract(page):
    lines = pdflines_from_pagedict(page)
    a = PdfLineSelection.text("Template periodic disclosure").select(lines)
    fl1 = PdfLineSelection.text("Product name").select(lines)
    fl2 = PdfLineSelection.area_from_bounds(
        0.0,
        PdfLineSelection.text("Product name"),
        1e6,
        PdfLineSelection.text("Legal entity identifier"),
    ).select(lines)
    fund = fl1[0].text if len(fl2) == 0 else fl1[0].text + fl2[0].text

    return [PdfBlock(OnePdfBlockType.RELEVANT_BLOCK, {"article": a[0].text}, fund)]


def text_extract(pdf_blks, filter_data):
    if len(pdf_blks) == 0:
        return []
    blk = next(iter(pdf_blks))
    fund_name = blk.content.removeprefix("Product name: ")
    filter_funds = set(
        MatchFund(name=n.fund)
        for n in filter(lambda x: isinstance(x, Investment), filter_data)
    )
    fund = MatchFund(name=fund_name)
    a = blk.metadata["article"]
    art = SfdrArticle.ART_6
    if "Article 8" in a:
        art = SfdrArticle.ART_8
    elif "Article 9" in a:
        art = SfdrArticle.ART_9
    if fund in filter_funds:
        return [
            TextBlock.from_content(
                OneTextBlockType.RELEVANT_BLOCK, {"article": art}, fund_name
            )
        ]
    else:
        return []


def deserialize(txt_blk):
    return FundSfdrClassification(
        fund=txt_blk.content, article=txt_blk.metadata["article"]
    )
