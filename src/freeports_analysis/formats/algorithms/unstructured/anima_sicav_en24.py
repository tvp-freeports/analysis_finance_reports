"""ANIMA_SICAV-EN24 format submodule"""

import re
from freeports_analysis.formats.utils.text_filter import TextFilterInvestmentsStandard
from freeports_analysis.formats.utils.text_filter import PdfBlocksTable
from freeports_analysis.formats.algorithms.commons import Pipeline

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
        c = txt_blk.content
        m = market_value_regex.match(c)
        txt_blk.metadata |= {"quantity": m[0]}
    return txt_blks


pipelines = {"investments": Pipeline(text_filter=text_filter)}
