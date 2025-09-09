"""MEDIOLLANUM_ES24_A format submodule"""

from typing import List, TypeAlias
from lxml import etree
from freeports_analysis.formats_utils.pdf_filter import (
    standard_pdf_filtering,
    OnePdfBlockType,
)
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.formats_utils.text_extract import (
    standard_text_extraction,
    EquityBondTextBlockType,
)
from freeports_analysis.formats_utils.deserialize import standard_deserialization
from freeports_analysis.formats_utils.pdf_filter.xml.font import is_present_txt_font
from .. import PdfBlock, TextBlock


@standard_pdf_filtering(
    header_set=PdfLineSet("TimesNewRomanPSMT", text="n de la cartera"),
    subfund_set=PdfLineSet("TimesNewRomanPSMT", area=(60, 77)),
    body_set=PdfLineSet("TimesNewRomanPSMT", area=(None, 795)),
    currency_set=PdfLineSet("TimesNewRomanPSMT", 9, text="(expresado en"),
    deselection_list=[PdfLineSet("TimesNewRomanPSMT", text="^ ")],
)
def _pdf_filter_one(xml_root: etree.Element) -> dict:
    raise NotImplementedError


@standard_pdf_filtering(
    header_set=PdfLineSet("TT91E2o00", text="n de la cartera"),
    subfund_set=PdfLineSet("TT91E2o00", area=(60, 77)),
    body_set=PdfLineSet("TT9162o00", area=(None, 795)),
    currency_set=PdfLineSet("TT9182o00", 8.997417, text="(expresado en"),
    deselection_list=[
        PdfLineSet("TT9462o00", text="^ "),
        PdfLineSet("TT9162o00", text="^ "),
    ],
)
def _pdf_filter_two(xml_root: etree.Element) -> dict:
    raise NotImplementedError


def pdf_filter(xml_root: etree.Element) -> dict:
    """This pdf filter use two different implementation for two different page types"""
    if is_present_txt_font(xml_root, "n de la cartera", "TimesNewRomanPSMT"):
        return _pdf_filter_one(xml_root)
    return _pdf_filter_two(xml_root)


@standard_text_extraction(
    nominal_quantity_pos=1,
    market_value_pos=4,
    perc_net_assets_pos=5,
    acquisition_cost_pos=3,
    acquisition_currency_pos=2,
)
def text_extract(pdf_blocks: List[PdfBlock], targets: List[str]) -> List[TextBlock]:
    raise NotImplementedError


@standard_deserialization()
def deserialize(txt_blk: TextBlock, targets: List[str]):
    raise NotImplementedError


PdfBlockType: TypeAlias = OnePdfBlockType
TextBlockType: TypeAlias = EquityBondTextBlockType
