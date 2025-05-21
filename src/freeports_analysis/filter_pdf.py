from .consts import PDF_Formats as fmt
import xml.etree.ElementTree as ET
from lxml import etree
from typing import List
import pymupdf as pypdf

parser = etree.XMLParser(recover=True)

def EURIZON_filter(root: ET.Element) -> List[ET.Element]:
    return root.findall('.//block')


def _DEFAULT_filter(root: ET.Element) -> List[ET.Element]:
    return [root]



def relevant_text(format_pdf: fmt, document: pypdf.Document) -> str:
    filter_func=None
    match format_pdf:
        case fmt.EURIZON:
            filter_func=EURIZON_filter
        case _:
            filter_func=_DEFAULT_filter
    text=''
    for page in document:
        xml_str=page.get_text("xml")
        xml_tree = etree.fromstring(xml_str.encode(), parser=parser)
        relevant_parts=filter_func(xml_tree)
        for part in relevant_parts:
            for line in part.findall('.//line'):
                for e in line.findall('.//char'):
                    c=e.get('c')
                    if c is not None:
                        text+=c
                text+='\n'
    return text
