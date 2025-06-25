"""Module common to each format, it contains the definitions used by all the formats"""

from enum import Enum
from typing import Optional, List, Callable, Tuple
from lxml import etree
from pymupdf import Document
import logging as log

logger = log.getLogger(__name__)


def _str_blocks(blk) -> str:
    text = f"{blk.__class__.__name__}:  ({blk.type_block.name} type)\n"
    text += f"\tmetadata {blk.metadata}\n"
    text_no_last_nl = blk.content
    if len(blk.content) > 0:
        if blk.content[-1] == "\n":
            text_no_last_nl = text_no_last_nl[:-1]
    text += f'\t"{text_no_last_nl}"'
    return text


class PdfBlock:
    """Rappresent a pdf content block with data to be extracted or relevant for the
    subsequents filtering stages
    """

    type_block: Enum
    metadata: Optional[dict]
    content: Optional[str]

    def _text_form_element(self, ele: etree.Element) -> str:
        text = ""
        if ele.tag == "line":
            lines = [ele]
        else:
            lines = ele.findall("line")
        for line in lines:
            for e in line.findall(".//char"):
                c = e.get("c")
                if c is not None:
                    text += c
            text += "\n"
        return text

    def __init__(self, type_block: Enum, metadata: dict, xml_ele: etree.Element):
        self.type_block = type_block
        self.metadata = metadata
        self.content = self._text_form_element(xml_ele)

    def __str__(self) -> str:
        return _str_blocks(self)


class TextBlock:
    type_block: Enum
    metadata: dict
    content: str
    pdf_block: PdfBlock

    def __init__(self, type_block: Enum, metadata: dict, pdf_block: PdfBlock):
        self.type_block = type_block
        self.metadata = metadata
        self.pdf_block = pdf_block
        self.content = pdf_block.content

    def __str__(self) -> str:
        return _str_blocks(self)


def pdf_filter_exec(
    document: Document,
    pdf_filter_func: Callable[[etree.Element], List[Tuple[etree.Element, PdfBlock]]],
) -> List[PdfBlock]:
    parser = etree.XMLParser(recover=True)
    relevant_blocks = []
    page_number = 1
    pages = len(document)
    for page in document:
        if page_number % (pages // min(10, pages)) == 0:
            logger.debug("Filtering page %i", page_number)
        xml_str = page.get_text("xml")
        xml_tree = etree.fromstring(xml_str.encode(), parser=parser)
        relevant_blocks += pdf_filter_func(xml_tree)
        page_number += 1
    return relevant_blocks


def text_extract_exec(
    pdf_blocks: List[PdfBlock],
    targets: List[str],
    text_extract_func: Callable[[List[PdfBlock], List[str]], List[TextBlock]],
) -> List[TextBlock]:
    return text_extract_func(pdf_blocks, targets)


def tabularize_exec(
    TextBlocks: List[TextBlock], tabularize_func: Callable[[TextBlock], dict]
) -> List[dict]:
    return [tabularize_func(txtblk) for txtblk in TextBlocks]
