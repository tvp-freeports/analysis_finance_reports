"""Custom pdf filter for FINECO-EN23[IR] format"""

from freeports_analysis.formats.algorithms.commons import Pipeline


from . import investments
from . import inv_managers
from . import managment_company
from . import fund_assets

pipelines = {
    "investments": Pipeline(
        pdf_extract=(
            investments.pdf_extract,
            investments.pdf_extract_fund,
            investments.pdf_extract_currency,
        )
    ),
    "inv_managers_table": Pipeline(
        inv_managers.pdf_filter,
        inv_managers.text_extract,
        (inv_managers.deserialize, inv_managers.deserialize_fund),
    ),
    "manco": Pipeline(
        managment_company.pdf_filter,
        managment_company.text_extract,
        managment_company.deserialize,
    ),
    "fund_assets": Pipeline(
        fund_assets.pdf_extract, fund_assets.text_filter, fund_assets.deserialize
    ),
}
