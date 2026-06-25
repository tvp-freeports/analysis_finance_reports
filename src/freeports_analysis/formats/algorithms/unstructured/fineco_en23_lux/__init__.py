"""Custom pdf filter for FINECO-EN23@LUX format"""

import re

from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.utils.pdf_extract import PdfExtractSfdrArticleStandard
from freeports_analysis.formats.utils.text_filter import TextFilterSfdrArticleStandard
from freeports_analysis.formats.utils.deserialize import DeserializeSfdrArticleStandard
from freeports_analysis.formats.algorithms.commons import Pipeline

from . import inv_managers
from . import fund_assets

sfdr_classification_pdf_extract = PdfExtractSfdrArticleStandard(
    art9_selection=(
        PdfLineSelection.text("periodic disclosure for")
        & PdfLineSelection.text("products referred to in Article 9")
    ),
    art8_selection=(
        PdfLineSelection.text("periodic disclosure for")
        & PdfLineSelection.text("products referred to in Article 8")
    ),
    fund_selection=PdfLineSelection.area_from_bounds(
        x0=PdfLineSelection.text("Product name"),
        x1=1e6,
        y0=0,
        y1=PdfLineSelection.text("Did this financial product"),
    )
    & (PdfLineSelection(font="calibri") | PdfLineSelection(font="calibri-bold"))
    / (PdfLineSelection(text="Regulation (EU) 2020/852", font="calibri-bold"))
    / (PdfLineSelection(text="^ $"))
    / (
        PdfLineSelection.area_from_movewindow(
            PdfLineSelection(text="Legal entity identifier")
            | PdfLineSelection(text="Legal Entity Identifier"),
            (0.0, 0.0),
            30.0,
            3.0,
        )
    )
    | PdfLineSelection.text("Product name"),
)

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
    "sfdr_classification": Pipeline(
        pdf_extract=sfdr_classification_pdf_extract,
        text_filter=TextFilterSfdrArticleStandard(
            fund_prefix=re.compile(r"Product name.*:\s*")
        ),
        deserialize=DeserializeSfdrArticleStandard(),
    ),
}
