"""MEDIOLLANUM_ES24_A format submodule"""

from lxml import etree
from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet


@standard_pdf_filtering(
    header_set=[
        # PdfLineSet(text="n de la cartera"),
        PdfLineSet(PdfLineSet(text="n de la cartera"), text="Descripci"),
        PdfLineSet(PdfLineSet(text="n de la cartera"), 9, text="(expresado en"),
    ],
    subfund_set=PdfLineSet(PdfLineSet(text="n de la cartera"), area=(58, 82)),
    body_set=PdfLineSet(PdfLineSet(text="n de la cartera"), area=(None, 795))
    / PdfLineSet(text="^ "),
    currency_set=PdfLineSet(
        PdfLineSet(text="n de la cartera"), 9, text="(expresado en"
    ),
)
def pdf_filter(xml_root: etree._Element) -> dict:
    """Filter PDF content for Mediolanum ES24"""
    raise NotImplementedError
