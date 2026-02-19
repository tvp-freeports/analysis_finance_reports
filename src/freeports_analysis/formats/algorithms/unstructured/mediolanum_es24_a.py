"""MEDIOLLANUM_ES24_A format submodule"""

from lxml import etree
from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSelection

h_font = PdfLineSelection.font_of(PdfLineSelection.text("n de la cartera"))
curr_set = PdfLineSelection(font_size=(8.9, 9.1), text="(expresado en") & h_font


@standard_pdf_filtering(
    header_set=[PdfLineSelection.text("Descripci") & h_font, curr_set],
    subfund_set=PdfLineSelection.area(0.0, 58.0, 1e6, 82.0) & h_font,
    body_set=PdfLineSelection.area(0.0, 0.0, 1e6, 795.0)
    & h_font / PdfLineSelection.text("^ "),
    currency_set=curr_set,
)
def pdf_filter(xml_root: etree._Element) -> dict:
    """Filter PDF content for Mediolanum ES24"""
    raise NotImplementedError
