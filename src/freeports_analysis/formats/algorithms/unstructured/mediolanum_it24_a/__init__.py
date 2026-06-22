from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.utils.pdf_extract import PdfExtractSfdrArticleStandard
from freeports_analysis.formats.utils.text_filter import TextFilterSfdrArticleStandard
from freeports_analysis.formats.utils.deserialize import DeserializeSfdrArticleStandard
from freeports_analysis.formats.templates import mediolanum_24
import re

from . import fund_assets


compute_page_class = mediolanum_24.compute_page_class

pipelines = {
    "fund_assets": Pipeline(
        pdf_extract=fund_assets.pdf_extract,
        text_filter=fund_assets.text_filter,
        deserialize=fund_assets.deserialize,
    ),
    "inv_managers_begin": Pipeline(
        pdf_extract=mediolanum_24.inv_managers.PdfExtractBeginPage("^INVESTMENT "),
        text_filter=mediolanum_24.inv_managers.text_filter_begin_page,
        deserialize=(
            mediolanum_24.inv_managers.deserialize,
            mediolanum_24.inv_managers.deserialize_fund,
            mediolanum_24.inv_managers.deserialize_manco,
        ),
    ),
    "inv_managers": Pipeline(
        pdf_extract=mediolanum_24.inv_managers.pdf_extract,
        text_filter=mediolanum_24.inv_managers.text_filter,
        deserialize=(
            mediolanum_24.inv_managers.deserialize,
            mediolanum_24.inv_managers.deserialize_fund,
        ),
    ),
    "inv_managers_end": Pipeline(
        pdf_extract=mediolanum_24.inv_managers.PdfExtractEndPage("^BANCA "),
        text_filter=mediolanum_24.inv_managers.text_filter,
        deserialize=(
            mediolanum_24.inv_managers.deserialize,
            mediolanum_24.inv_managers.deserialize_fund,
        ),
    ),
    "sfdr_classification": Pipeline(
        PdfExtractSfdrArticleStandard(
            PdfLineSelection.text("obiettivo di investimento sostenibile di questo"),
            PdfLineSelection.text(
                "sono state soddisfatte le caratteristiche ambientali e/o sociali promosse"
            ),
            PdfLineSelection.text("Nome del prodotto: "),
        ),
        TextFilterSfdrArticleStandard(
            (
                "Nome del prodotto: ",
                re.compile(" \(.*\)"),
                re.compile(", un comparto .*"),
            )
        ),
        DeserializeSfdrArticleStandard(),
    ),
}
