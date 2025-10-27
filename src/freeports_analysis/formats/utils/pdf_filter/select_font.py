"""Font-based selection utilities for PDF text filtering.

This module provides functions for filtering PDF text elements based on
font properties and text content combinations.
"""

from typing import List, Tuple
from .pdf_parts.font import Font
from .pdf_parts import ExtractedPdfLine


def deselect_txt_font(
    lines: List[ExtractedPdfLine], deselection_list: List[Tuple[str, Font]]
) -> List[ExtractedPdfLine]:
    """Filter out lines matching specific text-font combinations.

    Removes PDF lines that match any of the specified text and font pairs
    from the deselection list.

    Parameters
    ----------
    lines : List[ExtractedPdfLine]
        List of PDF text lines to filter
    deselection_list : List[Tuple[str, Font]]
        List of (text, font) pairs to exclude from results

    Returns
    -------
    List[ExtractedPdfLine]
        Filtered list of PDF lines excluding deselected combinations
    """
    return [
        line
        for line in lines
        if (line.xml_blk.xpath(".//@text")[0], line.font) not in deselection_list
    ]
