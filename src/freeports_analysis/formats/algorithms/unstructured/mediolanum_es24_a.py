"""MEDIOLLANUM_ES24_A format submodule"""

from lxml import etree
from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractPageClassifyStandard,
    PdfExtractCurrencyStandard,
    PdfExtractFundStandard,
    PdfExtractManagmentCompanyStandard,
)
from freeports_analysis.formats.utils.text_filter import (
    TextFilterPageClassifyStandard,
    TextFilterManagmentCompanyStandard,
)
from freeports_analysis.formats.utils.deserialize import (
    DeserializerPageClassifyStandard,
    DeserializerManagmentCompanyStandard,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.algorithms.commons import Pipeline

h_font = PdfLineSelection.font_of(PdfLineSelection.text("n de la cartera"))
curr_set = PdfLineSelection(font_size=(8.9, 9.1), text="(expresado en") & h_font

pipelines = {
    "": Pipeline(
        pdf_extract=(
            PdfExtractPageClassifyStandard(
                header_sets=[PdfLineSelection.text("Descripci") & h_font, curr_set],
                page_type="investments",
            ),
            PdfExtractPageClassifyStandard(
                header_sets=[
                    PdfLineSelection.text(
                        "Gestor de Inversiones y Gestora de Tesorería"
                    ),
                    PdfLineSelection.text(
                        "Presidente del Consejo de Administración de la Sociedad"
                    ),
                    PdfLineSelection.text("Gestores Delegados de Inversiones"),
                ],
                page_type="manco",
            ),
        ),
        text_filter=TextFilterPageClassifyStandard(),
        deserialize=DeserializerPageClassifyStandard(),
    ),
    "investments": Pipeline(
        pdf_extract=(
            PdfExtractInvestmentsStandard(
                body_set=PdfLineSelection.area(0.0, 0.0, 1e6, 767.0)
                & h_font / PdfLineSelection.text("^ ")
            ),
            PdfExtractFundStandard(
                PdfLineSelection.area(0.0, 58.0, 1e6, 82.0) & h_font
            ),
            PdfExtractCurrencyStandard(curr_set),
        )
    ),
    "manco": Pipeline(
        pdf_extract=PdfExtractManagmentCompanyStandard(
            PdfLineSelection.area_from_movewindow(
                PdfLineSelection.text("Gestor de Inversiones y Gestora de Tesorería"),
                (-0.1, 0.8),
                2.0,
                1.4,
            )
        ),
        text_filter=TextFilterManagmentCompanyStandard(),
        deserialize=DeserializerManagmentCompanyStandard(),
    ),
}
