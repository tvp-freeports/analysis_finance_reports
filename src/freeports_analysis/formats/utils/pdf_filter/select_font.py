"""Utilities for selecting or deselecting lines based of font and text information"""

from typing import List, Tuple
from .pdf_parts.font import Font
from .pdf_parts import ExtractedPdfLine


def deselect_txt_font(
    lines: List[ExtractedPdfLine], deselection_list: List[Tuple[str, Font]]
) -> List[ExtractedPdfLine]:
    """Deselect text with a certain font and text combination

    Parameters
    ----------
    lines : List[ExtractedPdfLine]
        list to filter
    deselection_list : List[Tuple[str, Font]]
        list of text and font to search for

    Returns
    -------
    List[ExtractedPdfLine]
        filtered list
    """
    return [
        line
        for line in lines
        if (line.xml_blk.xpath(".//@text")[0], line.font) not in deselection_list
    ]
