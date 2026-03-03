"""MEDIOLLANUM_ES24_A format submodule"""

from lxml import etree
from freeports_analysis.formats.utils.pdf_filter import (
    PdfExtractInvestmentsStandard,
    PdfExtractPageClassifyStandard,
)
from freeports_analysis.formats.utils.text_extract import TextFilterPageClassifyStandard
from freeports_analysis.formats.utils.deserialize import (
    DeserializerPageClassifyStandard,
)
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSelection
from freeports_analysis.formats.algorithms.commons import Pipeline

h_font = PdfLineSelection.font_of(PdfLineSelection.text("n de la cartera"))
curr_set = PdfLineSelection(font_size=(8.9, 9.1), text="(expresado en") & h_font

pipelines = {
    "": Pipeline(
        pdf_extract=PdfExtractPageClassifyStandard(
            header_sets=[PdfLineSelection.text("Descripci") & h_font, curr_set],
            page_type="investments",
        ),
        text_filter=TextFilterPageClassifyStandard(),
        deserialize=DeserializerPageClassifyStandard(),
    ),
    "investments": Pipeline(
        pdf_extract=PdfExtractInvestmentsStandard(
            subfund_set=PdfLineSelection.area(0.0, 58.0, 1e6, 82.0) & h_font,
            body_set=PdfLineSelection.area(0.0, 0.0, 1e6, 795.0)
            & h_font / PdfLineSelection.text("^ "),
            currency_set=curr_set,
        )
    ),
}
