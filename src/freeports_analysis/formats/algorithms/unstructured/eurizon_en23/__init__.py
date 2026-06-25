from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.utils.pdf_extract import PdfExtractSfdrArticleStandard
from freeports_analysis.formats.utils.text_filter import TextFilterSfdrArticleStandard
from freeports_analysis.formats.utils.deserialize import DeserializeSfdrArticleStandard
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from . import fund_assets
from . import investment_managers
from . import merging
from . import esg_indicators as esg


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
    "merging": Pipeline(
        merging.pdf_extract,
        (merging.text_filter, merging.text_filter_last_date),
        (merging.deserialize, merging.deserialize_last_date),
    ),
    "sfdr": Pipeline(
        PdfExtractSfdrArticleStandard(
            PdfLineSelection.text("Template periodic disclosure")
            & PdfLineSelection.text("Article 9"),
            PdfLineSelection.text("Template periodic disclosure")
            & PdfLineSelection.text("Article 8"),
            PdfLineSelection.text("Product name")
            | PdfLineSelection.area_from_bounds(
                0.0,
                PdfLineSelection.text("Product name"),
                1e6,
                PdfLineSelection.text("Legal entity identifier"),
            ),
        ),
        TextFilterSfdrArticleStandard("Product name: "),
        DeserializeSfdrArticleStandard(),
    ),
    "esg_fund": Pipeline(
        esg.pdf_extract_fund, esg.text_filter_fund, esg.deserialize_fund
    ),
    "esg_indicators": Pipeline(esg.pdf_extract, esg.text_filter, esg.deserialize),
    # "merging":Pipeline(merging.pdf_extract,merging.text_filter,merging.deserialize),
    # "merging_end":Pipeline(merging.pdf_extract_end,merging.text_filter,merging.deserialize),
    "renaming": Pipeline(
        merging.pdf_extract_renaming, merging.text_filter_renaming, merging.deserialize
    ),
}
