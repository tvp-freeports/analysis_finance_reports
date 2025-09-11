"""MEDIOLLANUM_ES24_B format submodule"""

from typing import List, TypeAlias
from enum import auto, Enum
from lxml import etree
from freeports_analysis.formats.utils.pdf_filter import (
    standard_pdf_filtering,
)
from freeports_analysis.formats.utils.pdf_filter.xml.position import get_lines_contained
from freeports_analysis.formats.utils.pdf_filter.xml.font import is_present_txt_font
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.formats.utils.text_extract import (
    standard_text_extraction,
)
from freeports_analysis.formats.utils.deserialize import standard_deserialization
from .. import PdfBlock, TextBlock
from freeports_analysis.consts import (
    Promise,
    Currency,
    PromisesResolutionContext,
)


class PdfBlockType(Enum):
    RELEVANT_BLOCK = auto()
    SUBFUND = auto()


class TextBlockType(Enum):
    BOND_TARGET = auto()
    EQUITY_TARGET = auto()
    SUBFUND = auto()


def pdf_filter(xml_root: etree.Element) -> dict:
    if is_present_txt_font(
        xml_root, "Registro CNMV:", "Helvetica-Bold"
    ) and is_present_txt_font(xml_root, "Grupo Gestora:", "Helvetica-Bold"):
        blk = get_lines_contained(xml_root, y_range=(88, 102))[0]
        subfund = blk.xpath("./@text")[0].strip().upper()
        return [PdfBlock(PdfBlockType.SUBFUND, {"subfund": subfund}, blk)]

    @standard_pdf_filtering(
        header_set=[
            PdfLineSet("Helvetica-Bold", text="Descripc"),
            PdfLineSet("Helvetica-Bold", text="Divisa"),
            PdfLineSet("Helvetica-Bold", text="Periodo actual"),
        ],
        subfund_set=Promise("title document"),
        body_set=PdfLineSet("Helvetica"),
        currency_set=Currency.EUR,
    )
    def standard_pdf_filter(xml_root):
        raise NotImplementedError

    return standard_pdf_filter(xml_root)


def text_extract(pdf_blocks: List[PdfBlock], targets: List[str]) -> List[TextBlock]:
    if len(pdf_blocks) == 1 and pdf_blocks[0].type_block == PdfBlockType.SUBFUND:
        return [
            TextBlock(
                TextBlockType.SUBFUND,
                {"subfund": pdf_blocks[0].metadata["subfund"]},
                pdf_blocks[0],
            )
        ]

    @standard_text_extraction(
        market_value_pos=2,
        perc_net_assets_pos=3,
        acquisition_currency_pos=1,
    )
    def standard_text_extract(pdf_blocks, targets):
        raise NotImplementedError

    return standard_text_extract(pdf_blocks, targets)


def deserialize(txt_blk: TextBlock):
    if txt_blk is None:
        return None
    if txt_blk.type_block == TextBlockType.SUBFUND:
        return PromisesResolutionContext(
            {"title document": txt_blk.metadata["subfund"]}
        )

    @standard_deserialization()
    def std_deserialize(txt_blk: TextBlock):
        raise NotImplementedError

    blk = std_deserialize(txt_blk)
    if blk is not None:
        blk.market_value = blk.market_value * 1000
    return blk
