import pymupdf as pypdf
from lxml import etree
import textwrap
from pathlib import Path
from typing import List
import dill
from freeports import _native

Algorithm = _native.core.Algorithm
PdfBlock = _native.core.PdfBlock
from freeports._internals.formats.repo.metadata import get_formats

extraction_flags = pypdf.TEXT_PRESERVE_IMAGES | pypdf.TEXT_COLLECT_VECTORS


def get_page_xml(file_name: str, page: int):
    pdf_file = pypdf.Document(file_name)
    parser = etree.XMLParser(recover=True)
    page_doc = pdf_file[page - 1]
    xml_str = page_doc.get_text("xml")
    xml_tree = etree.fromstring(xml_str.encode(), parser=parser)
    return xml_tree


def get_page_html(file_name: str, page: int):
    pdf_file = pypdf.Document(file_name)
    page_doc = pdf_file[page - 1]
    html_str = page_doc.get_text("html")
    return html_str


def get_page_table(file_name: str, page: int):
    pdf_file = pypdf.Document(file_name)
    page_doc = pdf_file[page - 1]
    tabs = page_doc.find_tables()
    return tabs


def get_page_dict(file_name: str, page: int):
    pdf_file = pypdf.Document(file_name)
    page_doc = pdf_file[page - 1]
    page = page_doc.get_text("dict", flags=extraction_flags)
    return page


get_page = get_page_dict


def get_doc(file_path):
    pdf_file = pypdf.Document(file_path)
    return [page.get_text("dict", flags=extraction_flags) for page in pdf_file]


def get_pdf_from_tests(fmt, document=None, base_path=None):
    _file = base_path / fmt / ("" if document is None else str(document)) / "report.pdf"
    pdf_file = pypdf.Document(_file)
    return pdf_file


def get_doc_from_tests(fmt, document=None, base_path=None):
    pdf = get_pdf_from_tests(fmt, document, base_path=base_path)
    return [page.get_text("dict") for page in pdf]


def print_pdf_line_sets(page, strings, mode="structured"):
    if isinstance(strings, str):
        strings = [strings]
    first_string = True
    from freeports._internals.formats.utils.pdf_extract.pdf_blks_acquire import (
        pdflines_from_pagedict,
    )
    from freeports import _native

    PdfLineSelection = _native.core.PdfLineSelection
    lines = pdflines_from_pagedict(page)
    for txt in strings:
        exl = PdfLineSelection.text(txt).select(lines)
        if not first_string:
            print("-----------------------------")
        first_string = False
        for el in exl:
            x_min, y_min, x_max, y_max = el.bbox
            txt = el.text
            fs = el.font_size
            font = el.font_name
            if mode == "structured":
                area = f"(({x_min}:{x_max})({y_min}:{y_max}))"
                print(f'{font}[{fs}]{area} "{txt}"')
            elif mode == "semistructured":
                print(f"font: {font}")
                print(f"text: {txt}")
                print(f"font_size: {fs}")
                print("area:")
                print(f"\tx_min: {x_min}")
                print(f"\tx_max: {x_max}")
                print(f"\ty_min: {y_min}")
                print(f"\ty_max: {y_max}")
            else:
                print("PdfLineSelection(")
                print(f'\tfont="{font}",')
                print(f'\ttext="{txt}",')
                print(f"\tfont_size={fs},")
                print("\tarea=(")
                print(f"\t\t({x_min},{x_max}),")
                print(f"\t\t({y_min},{y_max})")
                print("\t)")
                print(")")


class _PdfBlocksTable:
    """Read-only grid view of PDF blocks keyed by their table-row/table-col metadata.

    Local, display-only reimplementation: the original `PdfBlocksTable` was ported to Rust as an
    internal-only struct (never exposed to Python) once `freeports._native`'s `TextFilterInvestmentsStandard`
    took over the production text-filter loop, so it's no longer importable from Python.
    """

    def __init__(self, pdf_blocks: List[PdfBlock]):
        dict_table = {}
        col_max = 0
        for blk in pdf_blocks:
            row = blk.metadata["table-row"]
            col = blk.metadata["table-col"]
            dict_table.setdefault(row, {}).setdefault(col, []).append(blk)
            col_max = max(col, col_max)
        self._table = [
            [dict_table[row].get(col, []) for col in range(col_max + 1)]
            for row in sorted(dict_table.keys())
        ]

    @property
    def shape(self):
        rows = len(self._table)
        cols = max((len(row) for row in self._table), default=0)
        return (rows, cols)

    def __getitem__(self, coords):
        row, col = coords
        cell = self._table[row][col]
        if len(cell) == 1:
            return cell[0]
        if len(cell) == 0:
            return None
        return cell


def print_pdf_blks_table_MD(
    pdf_blocks: List[PdfBlock], max_cell_width: int = 30, show_grid: bool = True
) -> None:
    pdf_blocks = [
        b for b in pdf_blocks if "table-row" in b.metadata and "table-col" in b.metadata
    ]

    if not pdf_blocks:
        print("No PDF blocks to display")
        return

    table = _PdfBlocksTable(pdf_blocks)
    rows, cols = table.shape

    if rows == 0 or cols == 0:
        print("Empty table")
        return

    cell_contents = []
    max_lines_per_row = []

    for r in range(rows):
        row_contents = []
        max_lines = 1

        for c in range(cols):
            cell = table[r, c]
            if cell is None:
                row_contents.append([""])
            elif isinstance(cell, PdfBlock):
                content = cell.content.strip()
                if len(content) > max_cell_width:
                    content = textwrap.shorten(
                        content, width=max_cell_width, placeholder="..."
                    )
                row_contents.append([content])
            else:
                block_contents = []
                for block in cell:
                    content = block.content.strip()
                    if len(content) > max_cell_width:
                        content = textwrap.shorten(
                            content, width=max_cell_width, placeholder="..."
                        )
                    block_contents.append(content)
                row_contents.append(block_contents)
                max_lines = max(max_lines, len(block_contents))

        cell_contents.append(row_contents)
        max_lines_per_row.append(max_lines)

    if show_grid:
        header_sep = "|" + "|".join(["---" for _ in range(cols)]) + "|"
        for r in range(rows):
            lines_in_row = max_lines_per_row[r]
            for line_idx in range(lines_in_row):
                row_line = "|"
                for c in range(cols):
                    cell_lines = cell_contents[r][c]
                    if line_idx < len(cell_lines):
                        row_line += f" {cell_lines[line_idx]} |"
                    else:
                        row_line += " |"
                print(row_line)
                if r == 0 and line_idx == lines_in_row - 1:
                    print(header_sep)
    else:
        for r in range(rows):
            lines_in_row = max_lines_per_row[r]
            for line_idx in range(lines_in_row):
                row_line = ""
                for c in range(cols):
                    cell_lines = cell_contents[r][c]
                    if line_idx < len(cell_lines):
                        content = cell_lines[line_idx]
                        padded_content = content.ljust(max_cell_width)
                        row_line += f" {padded_content} "
                    else:
                        row_line += " " * (max_cell_width + 2)
                    if c < cols - 1:
                        row_line += " "
                print(row_line)
            if r < rows - 1:
                print()


def print_pdf_blks_table_ASCII(
    pdf_blocks: List[PdfBlock], max_cell_width: int = 30
) -> None:
    pdf_blocks = [
        b for b in pdf_blocks if "table-row" in b.metadata and "table-col" in b.metadata
    ]
    if not pdf_blocks:
        print("No PDF blocks to display")
        return

    table = _PdfBlocksTable(pdf_blocks)
    rows, cols = table.shape

    if rows == 0 or cols == 0:
        print("Empty table")
        return

    cell_contents = []
    max_lines_per_row = []
    col_widths = [0] * cols

    for r in range(rows):
        row_contents = []
        max_lines = 1

        for c in range(cols):
            cell = table[r, c]
            if cell is None:
                row_contents.append([""])
            elif isinstance(cell, PdfBlock):
                content = cell.content.strip()
                if len(content) > max_cell_width:
                    content = textwrap.shorten(
                        content, width=max_cell_width, placeholder="..."
                    )
                row_contents.append([content])
                col_widths[c] = max(col_widths[c], len(content))
            else:
                block_contents = []
                for block in cell:
                    content = block.content.strip()
                    if len(content) > max_cell_width:
                        content = textwrap.shorten(
                            content, width=max_cell_width, placeholder="..."
                        )
                    block_contents.append(content)
                    col_widths[c] = max(col_widths[c], len(content))
                row_contents.append(block_contents)
                max_lines = max(max_lines, len(block_contents))

        cell_contents.append(row_contents)
        max_lines_per_row.append(max_lines)

    col_widths = [max(3, w) for w in col_widths]

    def horizontal_border() -> str:
        border = "+"
        for w in col_widths:
            border += "-" * (w + 2) + "+"
        return border

    col_header = " "
    for c in range(cols):
        col_num = str(c).center(col_widths[c] + 2)
        col_header += f"{col_num} "
    print(col_header)

    top_border = horizontal_border()
    print(top_border)

    for r in range(rows):
        lines_in_row = max_lines_per_row[r]
        for line_idx in range(lines_in_row):
            row_line = "|"
            for c in range(cols):
                cell_lines = cell_contents[r][c]
                if line_idx < len(cell_lines):
                    content = cell_lines[line_idx]
                    padded_content = content.ljust(col_widths[c])
                    row_line += f" {padded_content} |"
                else:
                    row_line += " " * (col_widths[c] + 2) + "|"
            if line_idx == 0:
                row_line = row_line[:-1] + f"| {str(r).rjust(2)}"
            else:
                row_line = row_line[:-1] + "|"
            print(row_line)
        row_border = horizontal_border()
        print(row_border)


def get_pdf_blocks(
    fmt,
    document,
    page_type,
    n_page,
    base_path=None,
    page=None,
    algorithm=None,
    only_computed=False,
):
    if algorithm is None:
        algorithm = Algorithm.load(
            base_path.parent, fmt, list(get_formats(base_path.parent).index)
        )
    if page is None:
        page = get_doc_from_tests(fmt, document, base_path=base_path)[n_page - 1]
    pdf_blks = algorithm.apply_pdf_extract(page, page_type)
    if only_computed:
        return pdf_blks
    reference_pdf_blks = None
    with (
        base_path
        / fmt
        / ("" if document is None else str(document))
        / "pages"
        / page_type
        / f"{n_page}-pdf_blks.pkl"
    ).open("rb") as f:
        reference_pdf_blks = dill.load(f)
    return pdf_blks, reference_pdf_blks


def get_text_blocks(
    fmt,
    document,
    page_type,
    n_page,
    filter_data,
    base_path=None,
    page=None,
    algorithm=None,
    only_computed=False,
):
    if algorithm is None:
        algorithm = Algorithm.load(
            base_path.parent, fmt, list(get_formats(base_path.parent).index)
        )
    if page is None:
        page = get_doc_from_tests(fmt, document, base_path=base_path)[n_page - 1]

    txt_blks = algorithm.apply_text_filter(page, filter_data, page_type)
    if only_computed:
        return txt_blks
    with (
        base_path
        / fmt
        / ("" if document is None else str(document))
        / "pages"
        / page_type
        / f"{n_page}-txt_blks.pkl"
    ).open("rb") as f:
        reference_txt_blks = dill.load(f)
    return txt_blks, reference_txt_blks


def get_results(
    fmt,
    document,
    page_type,
    n_page,
    filter_data,
    base_path=None,
    page=None,
    algorithm=None,
    only_computed=False,
):
    if algorithm is None:
        algorithm = Algorithm.load(
            base_path.parent, fmt, list(get_formats(base_path.parent).index)
        )
    if page is None:
        page = get_doc_from_tests(fmt, document, base_path=base_path)[n_page - 1]
    results = algorithm.apply_deserialize(page, filter_data, page_type)
    if only_computed:
        return results
    with (
        base_path
        / fmt
        / ("" if document is None else str(document))
        / "pages"
        / page_type
        / f"{n_page}-results.pkl"
    ).open("rb") as f:
        reference_results = dill.load(f)
    return results, reference_results


def relative_movewindow_area(vec, width_mult, height_mult):
    def context(x0, y0, x1, y1):
        w = x1 - x0
        h = y1 - y0
        x, y = vec
        return (
            x0 + x * w,
            y0 + y * h,
            x0 + (width_mult + x) * w,
            y0 + (height_mult + y) * h,
        )

    return context
