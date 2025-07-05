import pymupdf as pypdf
from lxml import etree
import copy


def get_page(file_name: str, page: int, offset: int = 0):
    pdf_file = pypdf.Document(file_name)
    parser = etree.XMLParser(recover=True)
    page_doc = pdf_file[page + offset]
    xml_str = page_doc.get_text("xml")
    xml_tree = etree.fromstring(xml_str.encode(), parser=parser)
    return xml_tree


def get_page_html(file_name: str, page: int, offset: int = 0):
    pdf_file = pypdf.Document(file_name)
    page_doc = pdf_file[page + offset]
    html_str = page_doc.get_text("html")
    return html_str


def print_blocks(xml_tree: etree.Element, max_deeph: int = 0) -> None:
    etree_to_print = copy.deepcopy(xml_tree)

    def _remove_tree_to_depth(elem: etree.Element, depth: int = 0, max_depth: int = 0):
        for e in list(elem):
            if depth >= max_depth:
                elem.remove(e)
            else:
                _remove_tree_to_depth(e, depth + 1, max_depth)

    _remove_tree_to_depth(etree_to_print, depth=0, max_depth=max_deeph)
    print(etree.tostring(etree_to_print, pretty_print=True).decode(), end="")
    del etree_to_print
