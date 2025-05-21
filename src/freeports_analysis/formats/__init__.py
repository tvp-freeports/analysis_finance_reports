from enum import Enum
from typing import Optional, List, Callable, Tuple
from lxml import etree
from pymupdf import Document


class PDF_Block:
    type_block: Enum
    metadata: Optional[dict]
    content: Optional[str]

    def __init__(self, type_block: Enum, metadata: dict, content: str):
        self.type_block = type_block
        self.metadata = metadata
        self.content = content


class Text_Block:
    type_block: Enum
    metadata: dict
    content: str
    pdf_block: PDF_Block

    def __init__(self, type_block: Enum, metadata: dict, pdf_block: PDF_Block):
        self.type_block = type_block
        self.metadata = metadata
        self.pdf_block = pdf_block
        self.content = pdf_block.content


def _text_form_element(ele: etree.Element) -> str:
    text = ""
    for line in ele.findall(".//line"):
        for e in line.findall(".//char"):
            c = e.get("c")
            if c is not None:
                text += c
        text += "\n"
    return text


def pdf_filter_exec(
    document: Document,
    pdf_filter_func: Callable[[etree.Element], List[Tuple[etree.Element, PDF_Block]]],
) -> List[PDF_Block]:
    parser = etree.XMLParser(recover=True)
    relevant_blocks = []
    for page in document:
        xml_str = page.get_text("xml")
        xml_tree = etree.fromstring(xml_str.encode(), parser=parser)
        relevant_blocks_raw = pdf_filter_func(xml_tree)
        for e, obj in relevant_blocks_raw:
            obj.content = _text_form_element(e)
            relevant_blocks.append(obj)
    return relevant_blocks


def text_extract_exec(
    pdf_blocks: List[PDF_Block],
    targets: List[str],
    text_extract_func: Callable[[List[PDF_Block], List[str]], List[Text_Block]],
) -> List[Text_Block]:
    return text_extract_func(pdf_blocks, targets)


def tabularize_exec(
    text_blocks: List[Text_Block], tabularize_func: Callable[[Text_Block], dict]
) -> List[dict]:
    return [tabularize_func(txtblk) for txtblk in text_blocks]
