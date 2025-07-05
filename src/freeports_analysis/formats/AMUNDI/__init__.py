"""AMUNDI format submodule"""

from .. import PdfBlock, TextBlock
from ..utils_pdf_filter import one_pdf_blk, standard_pdf_filtering
from ..utils_pdf_filter.xml.font import get_lines_with_font
from ..utils_pdf_filter.select_position import select_inside
from ..utils_text_extract import standard_text_extraction, equity_bond_blks
from ..utils_deserialize import standard_deserialization
from ..utils_pdf_filter.pdf_parts.position import YRange
from ..utils_pdf_filter.pdf_parts import ExtractedPdfLine
from freeports_analysis.consts import Currency


@one_pdf_blk
class PdfBlock:
    pass


@equity_bond_blks
class TextBlock:
    pass


@standard_pdf_filtering(
    header_txt="Securities Portfolio as at",
    header_font="ArialNarrow-BoldItalic",
    subfund_height=(None, 27),
    subfund_font="ArialMT",
    body_font="ArialNarrow",
    y_range=(None, 768),
)
def pdf_filter(xml_root, page_number) -> dict:
    lines = get_lines_with_font(xml_root, "ArialNarrow")
    lines = [ExtractedPdfLine(line) for line in lines]
    y_range = YRange(None, 208)
    currency = select_inside(lines, y_range)[0].xml_blk.xpath(".//@text")[0]
    return {"currency": currency, "page": page_number}


@standard_text_extraction(
    nominal_quantity_pos=+1,
    market_value_pos=-1,
    perc_net_assets_pos=-2,
    currency=Currency.EUR,
)
def text_extract(pdf_blocks, i):
    return {"currency": pdf_blocks[i].metadata["currency"]}


@standard_deserialization(True)
def deserialize(pdf_block, targets):
    pass
