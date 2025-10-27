"""Low level utilities for handling typographic related aspects of the xml tree."""

from typing import List, Union
from lxml import etree


# FONT


def is_present_txt_font(blk: etree.Element, txt: str, font: str) -> bool:
    """Return if a certain pdf block with a specific text and font is present in the tree

    Parameters
    ----------
    blk : etree.Element
        tree to search in
    txt : str
        text to search
    font : str
        font to search

    Returns
    -------
    bool
        boolean describing if the block is present or not
    """
    relevant_part = get_lines_with_txt_font(blk, txt, font, all_elem=True)
    return len(relevant_part) > 0


def get_lines_with_txt_font(
    blk: etree.Element,
    txt: str,
    font: str,
    all_elem: bool = False,
    exact_match: bool = False,
) -> List[etree.Element] | etree.Element | None:
    """Get lines with a certain text and font.

    Parameters
    ----------
    blk : etree.Element
        XML tree structure to search
    txt : str
        Text to search for
    font : str
        Font to search for
    all_elem : bool, optional
        If True, return all matching elements as a list.
        If False, return only the first matching element.
    exact_match: bool, optional
        If True, perform exact string matching.
        If False, perform substring matching.

    Returns
    -------
    List[etree.Element] | etree.Element | None
        Matching line elements. Returns:
        - List if all_elem=True
        - Single element if all_elem=False and matches found
        - None if all_elem=False and no matches found
    """
    match_text = f"@text='{txt}'" if exact_match else f"contains(@text,'{txt}')"
    blks = blk.xpath(
        f"./descendant-or-self::line[{match_text} and font[@name='{font}']]"
    )
    return blks if all_elem else blks[0] if len(blks) > 0 else None


def get_lines_with_txt(
    blk: etree.Element,
    txt: str,
    all_elem: bool = False,
    exact_match: bool = False,
) -> List[etree.Element] | etree.Element | None:
    """Get lines containing specified text from XML tree.

    Parameters
    ----------
    blk : etree.Element
        XML tree structure to search
    txt : str
        Text to search for
    all_elem : bool, optional
        If True, return all matching elements as a list.
        If False, return only the first matching element.
    exact_match : bool, optional
        If True, perform exact string matching.
        If False, perform substring matching.

    Returns
    -------
    List[etree.Element] | etree.Element | None
        Matching line elements. Returns:
        - List if all_elem=True
        - Single element if all_elem=False and matches found
        - None if all_elem=False and no matches found
    """
    match_text = f"@text='{txt}'" if exact_match else f"contains(@text,'{txt}')"
    blks = blk.xpath(f"./descendant-or-self::line[{match_text}]")
    return blks if all_elem else blks[0] if len(blks) > 0 else None


def get_lines_with_font(
    blk: etree.Element, font: Union[str, List[str]]
) -> List[etree.Element]:
    """Return all the lines with certain font(s) in a tree

    Parameters
    ----------
    blk : etree.Element
        Tree from which to extract lines
    font : Union[str, List[str]]
        Font or list of fonts to extract

    Returns
    -------
    List[etree.Element]
        List of relevant lines
    """
    if isinstance(font, str):
        fonts = [font]
    else:
        fonts = font

    # Costruisci la condizione XPath per ogni font
    font_conditions = " or ".join([f"font[@name='{f}']" for f in fonts])
    xpath_query = f"./descendant-or-self::line[{font_conditions}]"

    return blk.xpath(xpath_query)


# SIZE


def get_lines_with_size(blk: etree.Element, size: str) -> List[etree.Element]:
    """
    Return all the lines with a specific font size in a tree.

    Parameters
    ----------
    blk : etree.Element
        Tree from which to extract lines.
    size : str
        Font size to match (as a string).

    Returns
    -------
    List[etree.Element]
        List of <line> elements that contain a <font> with the specified size.
    """

    return blk.xpath(f".//line[font[@size='{size}']]")


def get_lines_with_font_size(
    blk: etree.Element, txt: str, size: str, all_elem: bool = False
) -> List[etree.Element] | etree.Element | None:
    """Get lines with a certain text and font size.

    Parameters
    ----------
    blk : etree.Element
        XML tree structure to search
    txt : str
        Text to search for
    size : str
        Font size to search for
    all_elem : bool, optional
        If True, return all matching elements as a list.
        If False, return only the first matching element.

    Returns
    -------
    List[etree.Element] | etree.Element | None
        Matching line elements. Returns:
        - List if all_elem=True
        - Single element if all_elem=False and matches found
        - None if all_elem=False and no matches found
    """
    blks = blk.xpath(
        f"./descendant-or-self::line[contains(@text,'{txt}') and font[@size='{size}']]"
    )
    return blks if all_elem else blks[0] if len(blks) > 0 else None


def is_present_font_size(blk: etree.Element, txt: str, size: str) -> bool:
    """Return if a certain pdf block with a specific text and size is present in the tree

    Parameters
    ----------
    blk : etree.Element
        tree to search in
    txt : str
        text to search
    size : str
        size to search

    Returns
    -------
    bool
        boolean describing if the block is present or not
    """
    relevant_part = get_lines_with_font_size(blk, txt, size, all_elem=True)
    return len(relevant_part) > 0
