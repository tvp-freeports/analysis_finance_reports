from typing import List, Tuple
from .pdf_parts.font import Font
from .pdf_parts import ExtractedPdfLine


def deselect_txt_font(
    lines: List[ExtractedPdfLine], deselection_list: List[Tuple[str, Font]]
):
    return [
        line
        for line in lines
        if (line.xml_blk.xpath(".//@text")[0], line.font) not in deselection_list
    ]
