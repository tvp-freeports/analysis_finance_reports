"""Unstructured module for FINECO-EN23@IR"""

import re

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
            PdfLineSelection.text(
                "disclosure for the financial products referred to in Article 9"
            ),
            PdfLineSelection.text(
                "disclosure for the financial products referred to in Article 8"
            ),
            (
                PdfLineSelection.area_from_bounds(
                    x0=PdfLineSelection.text('means'),
                    x1=372.94,
                    y0=PdfLineSelection.text('Regulation (EU) 2020/852'),
                    y1=PdfLineSelection.text('Did this financial product')
                ) / (
                    PdfLineSelection.text("^ $") | PdfLineSelection.text("^  $")
                ) / (
                    PdfLineSelection.area_from_movewindow(
                        PdfLineSelection(text='Legal entity identifier') | PdfLineSelection(text='Legal Entity Identifier'),
                        (0.0,0.0), 30.0, 3.0
                    )
                ) | PdfLineSelection.text('Product name')
            ),
        ),
        TextFilterSfdrArticleStandard(fund_prefix=re.compile(r'Product name.*:\s*')),
        DeserializeSfdrArticleStandard(),
    ),
}
