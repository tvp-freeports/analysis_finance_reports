"""Pdf xml parts in a friendly format (custom python classes)."""

from typing import Optional, Tuple, Annotated
import re
from functools import reduce
from lxml import etree
from pydantic import BaseModel, AfterValidator, PositiveFloat
from freeports_analysis.i18n import _
from .font import Font, FontSize, FontSizeSet, FontSet, TextSet
from ..xml.position import get_bounds
from shapely import Polygon, box
from portion.interval import Interval
from .position import Area, XRange, YRange, Coord, Area, InputArea


class PdfLine:
    """A class representing a PDF line.

    This class provides a friendly interface to access geometric properties,
    font information, and text size of a line in a PDF document.

    Parameters
    ----------
    text : Optional[str]
        The text content of the line
    font : Optional[Font]
        The font of the line
    font_size : Optional[FontSize]
        The text size of the line
    area : Optional[Area]
        The area in which the line is contained

    """

    def __init__(
        self,
        font: Optional[Font] = None,
        font_size: Optional[FontSize] = None,
        area: Optional[Polygon] = None,
        text: Optional[str] = None,
    ):
        """Initialize the ExtractedPdfLine from an XML element.

        Parameters
        ----------
        blk : etree.Element
            The XML element containing the line data.
        """
        self._text = text
        self._font = font
        self._font_size = font_size
        self._area = area

    @property
    def area(self) -> Area:
        """Get the geometric properties of the line.

        Returns
        -------
        Area
            The area representing the line's bounds.
        """
        return self._area

    @property
    def font(self) -> Font:
        """Get the font used in the line.

        Returns
        -------
        Font
            The font used in the line.
        """
        return self._font

    @property
    def font_size(self) -> FontSize:
        """Get the text size used in the line.

        Returns
        -------
        FontSize
            The text size used in the line.
        """
        return self._font_size

    @property
    def text(self) -> str:
        """Get the text used in the line.

        Returns
        -------
        str
            The text used in the line.
        """
        return self._text

    def _fmt_point(self, coor):
        return "" if coor is None else f"{coor:.3f}"

    def __str__(self) -> str:
        """Return a formatted string representation of the line.

        Returns
        -------
        str
            Formatted string showing font, text size, and coordinates.
        """
        string = f"{self.__class__.__name__}:\n"
        string += f"\t'{self.font}' [{self.font_size}]\n"
        if self.text is not None:
            string += f'\t"{self.text}"\n'
        if self.area is not None:
            string += f"\t{self.area}\n"

        return string


class ExtractedPdfLine(PdfLine):
    def __init__(self, blk: etree.Element):
        """Initialize the ExtractedPdfLine from an XML element.

        Parameters
        ----------
        blk : etree.Element
            The XML element containing the line data.
        """
        bounds = get_bounds(blk)
        super().__init__(
            text=blk.xpath("./@text")[0],
            font=Font(blk.xpath(".//font/@name")[0]),
            font_size=FontSize(blk.xpath(".//font/@size")[0]),
            area=box(bounds[0][0], bounds[1][0], bounds[0][1], bounds[1][1]),
        )
        self._blk = blk

    @property
    def xml_blk(self) -> etree.Element:
        """Get the original XML element containing the line data.

        Returns
        -------
        etree.Element
            The original XML element containing the line data.
        """
        return self._blk


class InputPdfLineSet(BaseModel):
    text: Optional[str] = None
    font: Optional[str] = None
    font_size: Optional[PositiveFloat] = None
    area: Optional[InputArea] = None


_line_set_font_regexp = r"(?P<font>[\w\-, ]+)"
_number_regexp = r"[0-9]+(\.[0-9]+)?"
_line_set_fontsize_regexp = rf"\[(?P<font_size>{_number_regexp})\]"
_range_regexp = rf"\(({_number_regexp})?:({_number_regexp})?\)"
_line_set_area_regexp = (
    rf"(?P<y_range>{_range_regexp})|\((?P<area>{_range_regexp}{_range_regexp})\)"
)
_line_set_text_regexp = '"(?P<text>.*)"'
line_set_regexp = f"({_line_set_font_regexp})? ?"
line_set_regexp += f"({_line_set_fontsize_regexp})? ?"
line_set_regexp += f"({_line_set_area_regexp})? ?"
line_set_regexp += f"({_line_set_text_regexp})?"
_line_set_regexp = re.compile(line_set_regexp)


class PdfLineSet:
    def __init__(
        self,
        font: Optional[FontSet] = None,
        font_size: Optional[FontSizeSet] = None,
        area: Optional[Polygon | Tuple[float, float]] = None,
        text: Optional[TextSet] = None,
    ):
        self._font = font
        self._font_size = font_size
        self._area = area
        self._text = text

    @classmethod
    def from_dict(cls, data):
        ls = InputPdfLineSet(**data)
        input_area = ls.area.model_dump() if ls.area is not None else None
        return cls(
            font=FontSet(ls.font) if ls.font is not None else None,
            font_size=FontSizeSet.from_range(ls.font_size - 1e-3, ls.font_size + 1e-3)
            if ls.font_size is not None
            else None,
            area=box(
                input_area["x_min"],
                input_area["y_min"],
                input_area["x_max"],
                input_area["y_max"],
            )
            if input_area is not None
            else None,
            text=TextSet(ls.text) if ls.text is not None else None,
        )

    @classmethod
    def from_str(cls, string):
        matched = _line_set_regexp.match(string).groupdict()
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
            area = box(x_min, y_min, x_max, y_max)
        elif tmp_range is not None:
            y_min, y_max = _to_floats(tmp_range)
            area = box(-1e6, y_min, 1e6, y_max)
        fs = matched["font_size"]
        fs = float(fs) if fs is not None else None
        return cls(
            font=FontSet(matched["font"]) if matched["font"] is not None else None,
            font_size=FontSizeSet.from_range(fs - 1e-3, fs + 1e-3)
            if fs is not None
            else None,
            area=area,
            text=TextSet(matched["text"]) if matched["text"] is not None else None,
        )

    def __contains__(self, other: ExtractedPdfLine):
        if self.font is not None:
            if other.font not in self.font:
                return False
        if self.font_size is not None:
            if other.font_size not in self.font_size:
                return False
        if self.area is not None:
            if not self.area.contains(other.area):
                return False
        if self.text is not None:
            if other.text not in self.text:
                return False
        return True

    def __or__(self, other):
        return PdfLineSet(
            font=self.font | other.font,
            font_size=self.font_size | other.font_size,
            area=self.area | other.area,
            text=self.text | other.text,
        )

    def __and__(self, other):
        return PdfLineSet(
            font=self.font & other.font,
            font_size=self.font_size & other.font_size,
            area=self.area & other.area,
            text=self.text & other.text,
        )

    def __truediv__(self, other):
        return PdfLineSet(
            font=self.font - other.font,
            font_size=self.font_size - other.font_size,
            area=self.area - other.area,
            text=self.text / other.text,
        )

    def __sum__(self, other):
        return self | other

    def __sub__(self, other):
        return self / other

    @property
    def font(self) -> FontSet:
        """Get the font used in the line.

        Returns
        -------
        Font
            The font used in the line.
        """
        return self._font

    @property
    def font_size(self) -> FontSizeSet:
        """Get the text size used in the line.

        Returns
        -------
        FontSize
            The text size used in the line.
        """
        return self._font_size

    @property
    def text(self) -> TextSet:
        """Get the text used in the line.

        Returns
        -------
        str
            The text used in the line.
        """
        return self._text

    @property
    def area(self) -> Polygon:
        return self._area

    def __repr__(self):
        string = f"{self.__class__.__name__}:\n"
        font = "{}"
        if self.font is not None:
            font = (
                f"{set(self.font)}"
                if not isinstance(self.font, PdfLineSet)
                else "<font_ref>"
            )
        fs = "[]"
        if self.font_size is not None:
            fs = (
                f"{Interval(self.font_size)}"
                if not isinstance(self.font_size, PdfLineSet)
                else "<font_size_ref>"
            )
        area = f"{self.area}" if not isinstance(self.area, PdfLineSet) else "<area_ref>"
        text = f"{self.text}" if not isinstance(self.text, PdfLineSet) else "<text_ref>"
        string += f"\t{font} {fs}\n"
        if self.text is not None:
            string += f"\t{text}\n"
        if self.area is not None:
            string += f"\t{area}\n"
        return string

    def contextualize(self, xml_root):
        concrete = PdfLineSet(
            font=self.font, font_size=self.font_size, area=self.area, text=self.text
        )
        lines = [ExtractedPdfLine(el) for el in xml_root.findall(".//line")]

        def _contextualize(t, value, aggregators, lines, xml_root):
            handled = isinstance(value, t)
            if handled:
                return value
            for condition, agg_func in aggregators:
                if isinstance(condition, type):
                    handled = isinstance(value, condition)
                else:
                    handled = condition(value)
                if handled:
                    return agg_func(value, lines, xml_root)
            raise ValueError(
                _("Not possible aggregate to {} from {}:\n{}").format(
                    t, type(value), value
                )
            )

        font_aggregators = _font_aggregators
        font_size_aggregators = _font_size_aggregators
        text_aggregators = _text_aggregators
        area_aggregators = _area_aggregators
        concrete._font = _contextualize(
            Font, concrete.font, font_aggregators, lines, xml_root
        )
        concrete._font_size = _contextualize(
            FontSizeSet, concrete.font_size, font_size_aggregators, lines, xml_root
        )
        concrete._text = _contextualize(
            TextSet, concrete.text, text_aggregators, lines, xml_root
        )
        concrete._area = _contextualize(
            Polygon, concrete.area, area_aggregators, lines, xml_root
        )
        return concrete


def _pdf_line_set_aggregator(attribute):
    def wrapper(agg_func):
        def new_agg(value, lines, xml_root):
            value = value.contextualize(xml_root)
            inputs = [getattr(l, attribute) for l in lines if l in value]
            return agg_func(inputs)

        return new_agg

    return wrapper


@_pdf_line_set_aggregator("font")
def _default_font_agg(fonts):
    return FontSet(*fonts)


@_pdf_line_set_aggregator("font_size")
def _default_font_size_agg(font_sizes):
    if len(font_sizes) == 1:
        fs = font_sizes[0]
        font_sizes = [fs - 1e-4, fs + 1e-4]
    return FontSizeSet.from_range(min(*font_sizes), max(*font_sizes))


@_pdf_line_set_aggregator("text")
def _default_text_agg(texts):
    return TextSet(*[f"^{t}$" for t in texts])


@_pdf_line_set_aggregator("area")
def _pdflineset_area_agg(areas):
    if len(areas) == 1:
        y_max = areas[0].bounds[1]
        return box(-1e6, -1e6, 1e6, y_max)
    if len(areas) == 2:
        x_min0, y_min0, x_max0, y_max0 = areas[0].bounds
        x_min1, y_min1, x_max1, y_max1 = areas[1].bounds
        h_a = y_max1 - y_min2
        h_b = y_max2 - y_min1
        h = min(abs(h_a), abs(h_b))
        w_a = x_max1 - x_min2
        w_b = x_max2 - x_min1
        w = min(abs(w_a), abs(w_b))
        if h > w:
            return box(-1e6, min(y_max1, y_max2), 1e6, max(y_min1, y_min2))
        return box(min(x_max1, x_max2), -1e6, min(x_min1, x_min2), 1e6)
    if len(areas) == 3:
        x_min0, y_min0, x_max0, y_max0 = areas[0].bounds
        x_min1, y_min1, x_max1, y_max1 = areas[1].bounds
        x_min2, y_min2, x_max2, y_max2 = areas[2].bounds
        h_a12 = y_max1 - y_min2
        h_b12 = y_max2 - y_min1
        h_a23 = y_max2 - y_min3
        h_b23 = y_max3 - y_min2
        h_a13 = y_max1 - y_min3
        h_b13 = y_max3 - y_min1
        h_12 = min(abs(h_a12), abs(h_b12))
        h_23 = min(abs(h_a23), abs(h_b23))
        h_13 = min(abs(h_a13), abs(h_b13))
        i_h, h = max((0, h_12), (1, h_23), (2, h_13), key=lambda x: x[1])

        w_a12 = x_max1 - x_min2
        w_b12 = x_max2 - x_min1
        w_a23 = x_max2 - x_min3
        w_b23 = x_max3 - x_min2
        w_a13 = x_max1 - x_min3
        w_b13 = x_max3 - x_min1
        w_12 = min(abs(w_a12), abs(w_b12))
        w_23 = min(abs(w_a23), abs(w_b23))
        w_13 = min(abs(w_a13), abs(w_b13))
        i_w, w = max((0, w_12), (1, w_23), (2, w_13), key=lambda x: x[1])
        if h > w:
            y_maxs = [(y_max1, y_max2), (y_max2, y_max3), (y_max1, y_max3)]
            y_mins = [(y_min1, y_min2), (y_min2, y_min3), (y_min1, y_min3)]
            other_x = [(x_min3, x_max3), (x_min1, x_max1), (x_min2, x_max2)]
            x_mins, x_maxs = tuple(zip(*other_x))
            return box(
                other_x[i_h][1] if other_x[i_h][1] == min(*x_maxs) else -1e6,
                min(*y_maxs[i_h]),
                other_x[i_h][0] if other_x[i_h][0] == max(*x_mins) else 1e6,
                max(*y_mins[i_h]),
            )
        x_maxs = [(x_max1, x_max2), (x_max2, x_max3), (x_max1, x_max3)]
        x_mins = [(x_min1, x_min2), (x_min2, x_min3), (x_min1, x_min3)]
        other_y = [(y_min3, y_max3), (y_min1, y_max1), (y_min2, y_max2)]
        y_mins, y_maxs = tuple(zip(*other_y))
        return box(
            min(*x_maxs[i_w]),
            other_y[i_w][1] if other_y[i_w][1] == min(*y_maxs) else -1e6,
            max(*x_mins[i_w]),
            other_y[i_w][0] if other_y[i_w][0] == max(*y_mins) else 1e6,
        )

    bounds = [a.bounds for a in areas]
    x_mins, y_mins, x_maxs, y_maxs = tuple(zip(*bounds))
    return box(min(*x_maxs), min(*y_maxs), max(*x_mins), max(*y_mins))


def _default_area_agg(value, lines, xml_root):
    concrete_values = {"x_min": None, "x_max": None, "y_min": None, "y_max": None}
    for k in concrete_values.keys():
        vl = value[k]
        if isinstace(vl, PdfLineSet):
            vl = vl.contextualize(xml_root)
            bounds = [l.area.bounds for l in lines if l in vl]
            bounds_t = tuple(zip(*bounds))
            vl = {
                "x_min": min(*bounds_t[2]),
                "y_min": min(*bounds_t[3]),
                "x_max": max(*bounds_t[0]),
                "y_max": max(*bounds_t[1]),
            }[k]
        concrete_values[k] = vl
    return box(
        concrete_values["x_min"],
        concrete_values["y_min"],
        concrete_values["x_max"],
        concrete_values["y_max"],
    )


_font_aggregators = [(PdfLineSet, _default_font_agg)]
_font_size_aggregators = [(PdfLineSet, _default_font_size_agg)]
_text_aggregators = [(PdfLineSet, _default_text_agg)]
_area_aggregators = [(dict, _default_area_agg), (PdfLineSet, _pdflineset_area_agg)]
