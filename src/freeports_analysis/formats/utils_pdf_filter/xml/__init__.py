from lxml import etree
from typing import List
from abc import ABC


class Range:
    def __init__(self, start, end):
        self._start = start
        self._end = end

    @property
    def start(self):
        return self._start

    @property
    def end(self):
        return self._end

    @property
    def size(self):
        return self.end - self.start

    def __contains__(self, value):
        return self.start <= value <= self.end

    def __str__(self):
        return f"[{self.start},{self.end}]"


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
    blk: etree.Element, txt: str, font: str, all_elem: bool = False
) -> List[etree.Element]:
    blks = blk.xpath(
        f"./descendant-or-self::line[contains(@text,'{txt}') and font[@name='{font}']]"
    )
    return blks if all_elem else blks[0] if len(blks) > 0 else None


def get_lines_with_font(blk: etree.Element, font: str) -> List[etree.Element]:
    """Return all the line with a certain font in a tree

    Parameters
    ----------
    blk : etree.Element
        tree from which extract lines
    font : str
        font to extract

    Returns
    -------
    List[etree.Element]
        list of relevant blocks
    """
    return blk.xpath(f"./descendant-or-self::line[font[@name='{font}']]")
