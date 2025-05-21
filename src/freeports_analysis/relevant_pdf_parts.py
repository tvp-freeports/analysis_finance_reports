from .consts import PDF_Formats as fmt
from lxml import etree
from typing import List, Tuple
import pymupdf as pypdf
import pdf_parts
from pdf_parts import PDF_part

parser = etree.XMLParser(recover=True)


def EURIZON(root: ET.Element) -> Tuple[etree.Element, List[PDF_part]]:
    parts = []
    for blk in root.findall(".//block"):
        parts.append((blk, PDF_part(pdf_parts.EURIZON.TABLE, {})))
    return parts


def _DEFAULT(root: ET.Element) -> Tuple[etree.Element, List[PDF_part]]:
    pass


def pdf_relevant_parts(document: pypdf.Document, filter_pdf) -> List[PDF_part]:
    relevant_parts = []
    for page in document:
        xml_str = page.get_text("xml")
        xml_tree = etree.fromstring(xml_str.encode(), parser=parser)
        relevant_parts_raw = filter_pdf(xml_tree)
        for sub_part in relevant_parts_raw:
            text = ""
            xml_part, part_obj = sub_part
            for line in xml_part.findall(".//line"):
                for e in line.findall(".//char"):
                    c = e.get("c")
                    if c is not None:
                        text += c
                text += "\n"
            part_obj.content = text
            relevant_parts.append(part_obj)
