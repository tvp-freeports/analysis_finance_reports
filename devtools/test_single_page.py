import pymupdf as pypdf
from lxml import etree
import copy
from freeports_analysis.formats.algorithms import get_pipelines
from freeports_analysis.formats.utils.pdf_filter import PdfLineSet, ExtractedPdfLine
from freeports_analysis.formats.utils.pdf_filter.xml.font import get_lines_with_txt
from collections.abc import Callable


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
    """Print the content of a page until a certain depth

    Parameters
    ----------
    xml_tree : etree.Element
        page to be analized
    max_deeph : int, optional
        depth of the printed xml_tree:
        0: page margins and parameters
        1: blocks boxes coordinates
        2: line boxes coordinates and text
        3: text parameters (font, size)
        4: characters coordinates and parameters
        by default 0
    """
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


# what is the pipeline index?
def select_function(
    fmt: str, index_segment: int, pipeline_name: str = "", index: int = 0
) -> Callable:
    """select an already written function to filter/extract/deserialize a specific format document

    Parameters
    ----------
    fmt : str
        the pdf format needed
    index_segment : int
        the function needed:
        1: pdf_filter
        2: text extract
        3: deserialize
    pipeline_name : str, optional
        the pipeline needed (if present), by default ""
    index : int, optional
        pipeline index, by default 0

    Returns
    -------
    function
        the selected function
    """
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
            PdfLineSet(font=el.font, font_size=el.font_size, text=el.text, area=el.area)
            for el in exl
        ]
        if not first_string:
            print("-----------------------------")
        first_string = False
        for ln in ls:
            ln = ln._left
            x_min, y_min, x_max, y_max = ln.area.bounds
            txt = list(ln.text._left)[0]
            fs = (ln.font_size.upper + ln.font_size.lower) / 2
            font = list(ln.font)[0]
            if mode in "structured":
                area = f"(({x_min}:{x_max})({y_min}:{y_max}))"
                print(f'{font}[{fs}]{area} "{txt}"')
            elif mode in "semistructured":
                print(f"font: {font}")
                print(f"text: {txt}")
                print(f"font_size: {fs}")
                print("area:")
                print(f"\tx_min: {x_min}")
                print(f"\tx_max: {x_max}")
                print(f"\ty_min: {y_min}")
                print(f"\ty_max: {y_max}")
            else:
                print("PdfLineSet(")
                print(f'\tfont="{font}"')
                print(f'\ttext="{txt}"')
                print(f"\tfont_size={fs}")
                print("\tarea=(")
                print(f"\t\t({x_min},{x_max}),")
                print(f"\t\t({y_min},{y_max})")
                print("\t)")
                print(")")
