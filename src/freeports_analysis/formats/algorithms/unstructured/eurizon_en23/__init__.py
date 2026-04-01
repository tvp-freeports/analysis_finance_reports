from freeports_analysis.formats.algorithms.commons import Pipeline
from . import fund_assets
from . import investment_managers


def compute_page_class(classification):
    inv_managers = False
    for i, val in enumerate(classification):
        if inv_managers and val is None:
            classification[i] = "inv_managers"
        elif val == "inv_managers_begin":
            inv_managers = True
        elif val == "inv_managers_end":
            inv_managers = False
    return classification


pipelines = {
    "fund_assets": Pipeline(
        pdf_extract=(fund_assets.pdf_extract,),
        text_filter=fund_assets.text_filter,
        deserialize=fund_assets.deserialize,
    ),
    "manco": Pipeline(
        pdf_extract=investment_managers.pdf_extract_manco,
        text_filter=investment_managers.text_filter_manco,
        deserialize=investment_managers.deserialize_manco,
    ),
    "inv_managers_begin": Pipeline(
        pdf_extract=investment_managers.pdf_extract_inv_managers_begin,
        text_filter=investment_managers.text_filter_inv_managers_begin,
        deserialize=(
            investment_managers.deserialize_inv_managers,
            investment_managers.deserialize_fund,
        ),
    ),
    "inv_managers": Pipeline(
        pdf_extract=investment_managers.pdf_extract_inv_managers,
        text_filter=investment_managers.text_filter_inv_managers,
        deserialize=(
            investment_managers.deserialize_inv_managers,
            investment_managers.deserialize_fund,
        ),
    ),
    "inv_managers_end": Pipeline(
        pdf_extract=investment_managers.pdf_extract_inv_managers_end,
        text_filter=investment_managers.text_filter_inv_managers,
        deserialize=(
            investment_managers.deserialize_inv_managers,
            investment_managers.deserialize_fund,
        ),
    ),
}
