from enum import Enum
from lxml import etree
from typing import List, TypeAlias
from .. import PdfBlock, TextBlock
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.formats_utils.pdf_filter import (
    standard_pdf_filtering,
    OnePdfBlockType,
)
from freeports_analysis.formats_utils.text_extract import (
    standard_text_extraction,
    EquityBondTextBlockType,
)
from freeports_analysis.formats_utils.deserialize import standard_deserialization


soi = PdfLineSet(font="Calibri", font_size=13, text="Statement of Investments")
investment_fund = PdfLineSet(font="Calibri", font_size=14.5, area=(60, 80))
expressed_in = PdfLineSet(font="Calibri", font_size=8.5, text="expressed in")
table_company = PdfLineSet(font="Calibri", font_size=9)


@standard_pdf_filtering(
    header_set=soi,
    subfund_set=investment_fund,
    body_set=table_company,
    currency_set=expressed_in,
)
def pdf_filter(xml_root: etree.Element) -> List[PdfBlock]:
    raise NotImplementedError


@standard_text_extraction(
    nominal_quantity_pos=+1,
    market_value_pos=+4,
    perc_net_assets_pos=+5,
    acquisition_currency_pos=+2,
    acquisition_cost_pos=+3,
)
def text_extract(pdf_blocks: List[PdfBlock], targets: List[str]) -> List[TextBlock]:
    raise NotImplementedError


@standard_deserialization()
def deserialize(txt_block: TextBlock, targets: List[str]) -> dict:
    raise NotImplementedError


PdfBlockType: TypeAlias = OnePdfBlockType

TextBlockType: TypeAlias = EquityBondTextBlockType
