import pymupdf as pypdf
from lxml import etree
import copy
from freeports_analysis.formats.algorithms import get_pipelines
from freeports_analysis.formats.utils.pdf_filter import PdfLineSet, ExtractedPdfLine
from freeports_analysis.formats.utils.pdf_filter.xml.font import get_lines_with_txt


def get_page_xml(file_name: str, page: int, offset: int = -1):
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


def get_page_table(file_name: str, page: int, offset: int = 0):
    pdf_file = pypdf.Document(file_name)
    page_doc = pdf_file[page + offset]
    tabs = page_doc.find_tables()
    return tabs


def get_page_dict(file_name: str, page: int, offset: int = 0):
    pdf_file = pypdf.Document(file_name)
    page_doc = pdf_file[page + offset]
    page = page_doc.get_text("dict")
    return page


get_page = get_page_xml


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


def select_function(fmt, index_segment, pipeline_name="", index=0):
    pipeline = get_pipelines(fmt, allow_partial_pipelines=True)[pipeline_name]
    func = pipeline[index_segment][index]
    return func


def print_pdf_line_sets(page, strings, mode="structured"):
    if isinstance(strings, str):
        strings = [strings]
    first_string = True
    for txt in strings:
        exl = [
            ExtractedPdfLine(ln) for ln in get_lines_with_txt(page, txt, all_elem=True)
        ]
        ls = [
            PdfLineSet(
                font=el.font, font_size=el.font_size, text=el.text, area=el.geometry
            )
            for el in exl
        ]
        if not first_string:
            print("-----------------------------")
        first_string = False
        for ln in ls:
            x_min, x_max = ln.geometry.x_bounds
            y_min, y_max = ln.geometry.y_bounds
            if mode in "structured":
                area = f"(({x_min}:{x_max})({y_min}:{y_max}))"
                print(f'{ln.font}[{ln.font_size}]{area} "{ln.text}"')
            elif mode in "semistructured":
                print(f"font: {ln.font}")
                print(f"text: {ln.text}")
                print(f"font_size: {ln.font_size}")
                print("area:")
                print(f"\tx_min: {x_min}")
                print(f"\tx_max: {x_max}")
                print(f"\ty_min: {y_min}")
                print(f"\ty_max: {y_max}")
            else:
                print("PdfLineSet(")
                print(f'\tfont="{ln.font}"')
                print(f'\ttext="{ln.text}"')
                print(f'\tfont_size="{ln.font_size}"')
                print("\tarea=Area(")
                print(f"\t\tx_bound=XRange({x_min},{x_max}),")
                print(f"\t\ty_bound=YRange({y_min},{y_max})")
                print("\t)")
                print(")")
