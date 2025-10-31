"""ANIMA_SICAV-EN24 format submodule"""

import re
from freeports_analysis.formats.utils.text_extract import standard_text_extraction
from freeports_analysis.formats.utils.text_extract import PdfBlocksTable

market_value_regex = re.compile(r"(([0-9]+,)?[0-9]+,?[0-9]+\.[0-9]{2}) ")
# non sono sicuro di come ho riscritto questa regex e a cosa servivano le parentesi


@standard_text_extraction(
    nominal_quantity_pos=0,
    perc_net_assets_pos=3,
    acquisition_currency_pos=1,
    market_value_pos=2,
)
def text_extract(pdf_blks: PdfBlocksTable, i: int):
    """
    Text extract that extract quantity from the name of the company (is conained in the same cell)
    """
    c = pdf_blks[i].content
    m = market_value_regex.match(c)
    return {"quantity": m[0]}
