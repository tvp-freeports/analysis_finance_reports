import pymupdf as pypdf
from lxml import etree
import copy
import textwrap
from typing import List, Optional
from freeports_analysis.formats.algorithms import get_pipelines
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import (
    PdfLineSelection,
    pdfline_from_xml,
)
from freeports_analysis.formats.utils.pdf_filter.xml.font import get_lines_with_txt
from freeports_analysis.formats import PdfBlock
from freeports_analysis.formats.utils.text_extract import PdfBlocksTable
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
    xml_string = etree.tostring(etree_to_print, pretty_print=True).decode()
    lines = xml_string.split("\n")
    indented_lines = []
    indent_level = 0
    for line in lines:
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.startswith("</"):
            indent_level -= 1
        indented_lines.append("|  " * indent_level + stripped)
        if (
            stripped.startswith("<")
            and not stripped.startswith("</")
            and not stripped.endswith("/>")
        ):
            indent_level += 1
    print("\n".join(indented_lines))
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
            pdfline_from_xml(ln) for ln in get_lines_with_txt(page, txt, all_elem=True)
        ]

        if not first_string:
            print("-----------------------------")
        first_string = False
        for el in exl:
            x_min, y_min, x_max, y_max = el.bbox
            txt = el.text
            fs = el.font_size
            font = el.font_name
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
                print(f'\tfont="{font}",')
                print(f'\ttext="{txt}",')
                print(f"\tfont_size={fs},")
                print("\tarea=(")
                print(f"\t\t({x_min},{x_max}),")
                print(f"\t\t({y_min},{y_max})")
                print("\t)")
                print(")")


def print_pdf_blks_table_MD(
    pdf_blocks: List[PdfBlock], max_cell_width: int = 30, show_grid: bool = True
) -> None:
    """Print a table from PdfBlocks with row/col metadata in markdown format.

    Parameters
    ----------
    pdf_blocks : List[PdfBlock]
        List of PdfBlocks with 'table-row' and 'table-col' metadata
    max_cell_width : int, optional
        Maximum width for cell content before truncation with ellipsis, by default 30
    show_grid : bool, optional
        Whether to draw grid lines in the markdown table, by default True

    Notes
    -----
    - Cells can contain multiple PdfBlocks (displayed one under another)
    - Row height adjusts based on number of blocks in each cell
    - Long text is truncated with ellipsis using textwrap
    - Empty cells are shown as empty
    """
    if not pdf_blocks:
        print("No PDF blocks to display")
        return

    # Create table structure
    table = PdfBlocksTable(pdf_blocks)
    rows, cols = table.shape

    if rows == 0 or cols == 0:
        print("Empty table")
        return

    # Collect all cell contents
    cell_contents = []
    max_lines_per_row = []

    for r in range(rows):
        row_contents = []
        max_lines = 1  # At least 1 line per row

        for c in range(cols):
            cell = table[r, c]
            if cell is None:
                row_contents.append([""])
            elif isinstance(cell, PdfBlock):
                # Single block in cell
                content = cell.content.strip()
                if len(content) > max_cell_width:
                    content = textwrap.shorten(
                        content, width=max_cell_width, placeholder="..."
                    )
                row_contents.append([content])
            else:
                # Multiple blocks in cell
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

    # Build markdown table
    if show_grid:
        # Header separator
        header_sep = "|" + "|".join(["---" for _ in range(cols)]) + "|"
        # Print table
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

                # Add separator after first row
                if r == 0 and line_idx == lines_in_row - 1:
                    print(header_sep)
    else:
        # Simple table without grid lines
        for r in range(rows):
            lines_in_row = max_lines_per_row[r]

            for line_idx in range(lines_in_row):
                row_line = ""
                for c in range(cols):
                    cell_lines = cell_contents[r][c]
                    if line_idx < len(cell_lines):
                        content = cell_lines[line_idx]
                        # Pad content for alignment
                        padded_content = content.ljust(max_cell_width)
                        row_line += f" {padded_content} "
                    else:
                        row_line += " " * (max_cell_width + 2)

                    if c < cols - 1:
                        row_line += " "
                print(row_line)

            if r < rows - 1:
                print()  # Empty line between rows


def print_pdf_blks_table_ASCII(
    pdf_blocks: List[PdfBlock], max_cell_width: int = 30
) -> None:
    """Print a table from PdfBlocks with ASCII borders for better visualization.

    Parameters
    ----------
    pdf_blocks : List[PdfBlock]
        List of PdfBlocks with 'table-row' and 'table-col' metadata
    max_cell_width : int, optional
        Maximum width for cell content before truncation with ellipsis, by default 30
    """
    if not pdf_blocks:
        print("No PDF blocks to display")
        return

    # Create table structure
    table = PdfBlocksTable(pdf_blocks)
    rows, cols = table.shape

    if rows == 0 or cols == 0:
        print("Empty table")
        return

    # Collect all cell contents and calculate column widths
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

    # Ensure minimum column width
    col_widths = [max(3, w) for w in col_widths]

    # Build ASCII table with borders
    def horizontal_border() -> str:
        border = "+"
        for w in col_widths:
            border += "-" * (w + 2) + "+"
        return border

    # Print column numbers header (centered with columns)
    col_header = " "
    for c in range(cols):
        col_num = str(c).center(col_widths[c] + 2)
        col_header += f"{col_num} "
    print(col_header)

    # Print top border
    top_border = horizontal_border()
    print(top_border)

    # Print table rows with row numbers at right
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

            # Add row number for first line of each row at right
            if line_idx == 0:
                row_line = row_line[:-1] + f"| {str(r).rjust(2)}"
            else:
                row_line = row_line[:-1] + "   |"
            print(row_line)

        # Print border after each row
        row_border = horizontal_border()
        print(row_border)
