"""Custom pdf filter for FINECO-EN23[IR] format"""

from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.utils.pdf_extract import PdfExtractSfdrArticleStandard
from freeports_analysis.formats.utils.text_filter import TextFilterSfdrArticleStandard
from freeports_analysis.formats.utils.deserialize import DeserializeSfdrArticleStandard

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
        (managment_company.deserialize, inv_managers.deserialize),
    ),
    "fund_assets": Pipeline(
        fund_assets.pdf_extract, fund_assets.text_filter, fund_assets.deserialize
    ),
    "sfdr_classification": Pipeline(
        PdfExtractSfdrArticleStandard(
            PdfLineSelection.text('periodic disclosure for the financial products referred to in Article 8'),
            PdfLineSelection.text('periodic disclosure for the financial products referred to in Article 9'),
            PdfLineSelection.area_from_bounds(
                x0 = PdfLineSelection.text('Product name'),
                x1 = PdfLineSelection.text('Legal entity identifier'),
                y0 = 0,
                y1 = PdfLineSelection.text('Did this financial product'),  
            ) & PdfLineSelection.font('calibri')            
        ),
        TextFilterSfdrArticleStandard(),
        DeserializeSfdrArticleStandard()
    )
}
