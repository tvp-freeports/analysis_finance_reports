"""PDF XML parts in a friendly format (custom Python classes).

This module provides a high-level interface for working with PDF document elements
by wrapping raw XML structures into Python objects with intuitive properties and
methods. The main classes include:

- `PdfLineBaseClass`: Base class representing a PDF line with font, size, and area properties
- `ExtractedPdfLine`: Concrete implementation that extracts data from XML elements
- `PdfLineSet`: Complex set operations for filtering PDF lines based on multiple criteria

These classes enable sophisticated PDF document analysis by providing:
- Geometric operations (area containment, position filtering)
- Typographic filtering (font, size matching)
- Text content matching with regex support
- Set operations (union, intersection, difference)
- Contextualization based on actual PDF content

Examples
--------
>>> # Create a PDF line set filtering for specific font and area
>>> line_set = PdfLineSet(font="Arial", area=((0, 100), (0, 200)))
>>> # Check if a line matches the criteria
>>> line in line_set
True

Notes
-----
The module uses Shapely for geometric operations and lxml for XML processing.
All coordinates are in PDF points (1/72 inch).
"""

from typing import Optional, Tuple, Annotated, Callable, Any, List
import re
import ast
import freeports_lib
from operator import or_, and_, sub, truediv
from functools import reduce
from lxml import etree
from pydantic import BaseModel, AfterValidator, PositiveFloat
from shapely import Polygon, box
import PIL
import io
import numpy as np
from portion.interval import Interval
from freeports.i18n import _
from .position import InputArea
import copy
import enum

PdfLineSelection = freeports_lib.pdf_extract.select.PdfLineSelection
PdfLine = freeports_lib.pdf_extract.select.PdfLine


class PyMuPDFBlockType(enum.Enum):
    TEXT = 0
    IMAGE_RASTER = 1
    IMAGE_VECTOR = 3


def collapsedspans_from_line(l, treshold=1e-1):
    res = []
    last_font = None
    last_size = None
    collapse = True
    sum_font_size = 0.0
    n_spans = 0
    text = ""
    for s in l["spans"]:
        font = s["font"]
        font_size = s["size"]
        sum_font_size += font_size
        text += s["text"]
        if last_font is not None and last_size is not None:
            if font != last_font or abs(font_size - last_size) > treshold:
                collapse = False
        last_font = font
        last_size = font_size
        n_spans += 1
        res.append(
            {"font_size": font_size, "bbox": s["bbox"], "font": font, "text": s["text"]}
        )
    if collapse:
        res = [
            {
                "font_size": sum_font_size / n_spans,
                "bbox": l["bbox"],
                "font": last_font,
                "text": text,
            }
        ]
    return res


def rotate_bbox(bbox, cs, sn, new_left, new_top):
    x0, y0, x1, y1 = bbox
    a = (x0, y0)
    b = (x0, y1)
    c = (x1, y1)
    d = (x1, y0)
    new_Xs1 = map(lambda p: cs * p[0] + sn * p[1], (a, b, c, d))
    new_Ys1 = map(lambda p: cs * p[1] - sn * p[0], (a, b, c, d))
    new_Xs2 = copy.deepcopy(new_Xs1)
    new_Ys2 = copy.deepcopy(new_Ys1)
    new_x0 = min(new_Xs1)
    new_x1 = max(new_Xs2)
    new_y0 = min(new_Ys1)
    new_y1 = max(new_Ys2)
    return (new_x0 - new_left, new_y0 - new_top, new_x1 - new_left, new_y1 - new_top)


def rotate_lines_inplace(lines, width, height):
    A0 = (0.0, 0.0)
    B0 = (0.0, height)
    C0 = (width, height)
    D0 = (width, 0.0)
    for line in lines:
        c, s = line["dir"]
        if c == 1.0 and s == 0.0:
            continue
        new_left = min(map(lambda p: c * p[0] + s * p[1], (A0, B0, C0, D0)))
        new_top = min(map(lambda p: c * p[1] - s * p[0], (A0, B0, C0, D0)))
        line["bbox"] = rotate_bbox(line["bbox"], c, s, new_left, new_top)
        for span in line["spans"]:
            span["bbox"] = rotate_bbox(span["bbox"], c, s, new_left, new_top)
        line["dir"] = (1.0, 0.0)


def pdfimages_from_pagedict(page):
    images = [
        i
        for blk in filter(
            lambda x: x["type"] == PyMuPDFBlockType.IMAGE_RASTER.value, page["blocks"]
        )
    ]
    I = []
    for img in imgs:
        i = PIL.Image.open(io.BytesIO(img["image"]), formats=[img["ext"]])
        i = i.convert("RGB")
        I.append(np.asarray(i))
    return I


def pdflines_from_pagedict(page, auto_rotate=True):

    lines = [
        l
        for blk in filter(
            lambda x: x["type"] == PyMuPDFBlockType.TEXT.value, page["blocks"]
        )
        if "lines" in blk
        for l in blk["lines"]
    ]
    if auto_rotate:
        rotate_lines_inplace(lines, page["width"], page["height"])
    args = [s for l in list(map(collapsedspans_from_line, lines)) for s in l]

    return list(
        map(
            lambda s: PdfLine(
                font=s["font"], font_size=s["font_size"], text=s["text"], bbox=s["bbox"]
            ),
            filter(
                lambda a: (
                    not (a["bbox"][0] == a["bbox"][2] or a["bbox"][1] == a["bbox"][3])
                ),
                args,
            ),
        )
    )


class InputPdfLineSet(BaseModel):
    text: Optional[str] = None
    font: Optional[str | List[str]] = None
    font_size: Optional[PositiveFloat] = None
    area: Optional[InputArea] = None


_LINE_SET_FONT_REGEXP = r"(?P<font>[\w\-, ]+)"
_NUMBER_REGEXP = r"[0-9]+(\.[0-9]+)?"
_LINE_SET_FONTSIZE_REGEXP = rf"\[(?P<font_size>{_NUMBER_REGEXP})\]"
_RANGE_REGEXP = rf"\(({_NUMBER_REGEXP})?:({_NUMBER_REGEXP})?\)"
_LINE_SET_AREA_REGEXP = (
    rf"(?P<y_range>{_RANGE_REGEXP})|\((?P<area>{_RANGE_REGEXP}{_RANGE_REGEXP})\)"
)
_LINE_SET_TEXT_REGEXP = '"(?P<text>.*)"'
LINE_SET_REGEXP = f"({_LINE_SET_FONT_REGEXP})? ?"
LINE_SET_REGEXP += f"({_LINE_SET_FONTSIZE_REGEXP})? ?"
LINE_SET_REGEXP += f"({_LINE_SET_AREA_REGEXP})? ?"
LINE_SET_REGEXP += f"({_LINE_SET_TEXT_REGEXP})?"
_LINE_SET_REGEXP = re.compile(LINE_SET_REGEXP)


def _op_over_none(op: Callable, v1: Any, v2: Any) -> Any:
    """Apply operation to values, handling None cases gracefully.

    Parameters
    ----------
    op : Callable
        Binary operation to apply
    v1 : Any
        First operand
    v2 : Any
        Second operand

    Returns
    -------
    Any
        Result of operation, or the non-None value if only one is provided,
        or None if both are None
    """
    if v1 is not None and v2 is not None:
        return op(v1, v2)
    if v1 is not None:
        return v1
    if v2 is not None:
        return v2
    return None


def pdfline_selection_from_dict(data):
    ls = InputPdfLineSet(**data)
    input_area = ls.area.model_dump() if ls.area is not None else None
    fonts = [ls.font] if isinstance(ls.font, str) else ls.font
    selection = PdfLineSelection(
        font_size=(max(ls.font_size - 1e-3, 0.0), ls.font_size + 1e-3)
        if ls.font_size is not None
        else None,
        area=(
            input_area["x_min"] if input_area["x_min"] is not None else 0.0,
            input_area["y_min"] if input_area["y_min"] is not None else 0.0,
            input_area["x_max"] if input_area["x_max"] is not None else +1e6,
            input_area["y_max"] if input_area["y_max"] is not None else +1e6,
        )
        if input_area is not None
        else None,
        text=ls.text,
    )
    if fonts is None:
        return selection
    else:
        return (
            reduce(
                lambda sa, sb: sa | sb, map(lambda f: PdfLineSelection.font(f), fonts)
            )
            & selection
        )


def pdfline_selection_from_str(string):
    matched = _LINE_SET_REGEXP.match(string).groupdict()
    area = None
    tmp_area = matched["area"]
    tmp_range = matched["y_range"]

    def _to_floats(x):
        return (
            (float(c) if c != "" else None)
            for c in x.replace("(", "").replace(")", "").split(":")
        )

    if tmp_area is not None:
        x_range, y_range = tmp_area.split(")(")
        x_min, x_max = _to_floats(x_range)
        y_min, y_max = _to_floats(y_range)
        area = (
            x_min if x_min is not None else 0.0,
            y_min if y_min is not None else 0.0,
            x_max if x_max is not None else +1e6,
            y_max if y_max is not None else +1e6,
        )
    elif tmp_range is not None:
        y_min, y_max = _to_floats(tmp_range)
        area = (
            0.0,
            y_min if y_min is not None else 0.0,
            1e6,
            y_max if y_max is not None else +1e6,
        )
    fs = matched["font_size"]
    fs = float(fs) if fs is not None else None
    return PdfLineSelection(
        font=matched["font"].strip() if matched["font"] is not None else None,
        font_size=(max(fs - 1e-3, 0.0), fs + 1e-3) if fs is not None else None,
        area=area,
        text=matched["text"] if matched["text"] is not None else None,
    )
