"""Custom pdf filter for FINECO-EN23@LUX format"""

from freeports_analysis.formats.algorithms.commons import Pipeline

from . import inv_managers
from . import fund_assets

pipelines = {
    "inv_managers": Pipeline(
        (inv_managers.pdf_extract, inv_managers.pdf_extract_manco),
        inv_managers.text_filter,
        (
            inv_managers.deserialize,
            inv_managers.deserialize_fund,
            inv_managers.deserialize_manco,
        ),
    ),
    "fund_assets": Pipeline(
        pdf_extract=fund_assets.pdf_extract,
        text_filter=fund_assets.text_filter,
        deserialize=fund_assets.deserialize,
    ),
}
