from freeports_analysis.formats.algorithms.commons import Pipeline
from . import investment_managers
from . import fund_assets


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
        pdf_extract=(fund_assets.pdf_extract, fund_assets.pdf_extract_currency),
        text_filter=fund_assets.text_filter,
        deserialize=fund_assets.deserialize,
    ),
    "inv_managers_begin": Pipeline(
        pdf_extract=investment_managers.pdf_extract_begin_page,
        text_filter=investment_managers.text_filter_begin_page,
        deserialize=(
            investment_managers.deserialize,
            investment_managers.deserialize_fund,
        ),
    ),
    "inv_managers": Pipeline(
        pdf_extract=investment_managers.pdf_extract,
        text_filter=investment_managers.text_filter,
        deserialize=(
            investment_managers.deserialize,
            investment_managers.deserialize_fund,
        ),
    ),
    "inv_managers_end": Pipeline(
        pdf_extract=investment_managers.pdf_extract_end_page,
        text_filter=investment_managers.text_filter,
        deserialize=(
            investment_managers.deserialize,
            investment_managers.deserialize_fund,
        ),
    ),
}
