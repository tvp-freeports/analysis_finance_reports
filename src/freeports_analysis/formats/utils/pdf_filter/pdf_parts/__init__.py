"""Pdf xml parts in a friendly format (custom python classes)."""

from typing import Optional, Tuple, Annotated
import re
from lxml import etree
from pydantic import BaseModel, AfterValidator
from freeports_analysis.i18n import _
from .font import Font, FontSize, FontSizeRange
from ..xml.position import get_bounds
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
        area: Optional[Area | XRange | YRange | Tuple[float, float]] = None,
        text: Optional[str] = None,
    ):
        """Initialize the ExtractedPdfLine from an XML element.

        Parameters
        ----------
        blk : etree.Element
            The XML element containing the line data.
        """
        if area is None:
            area = Area(XRange(None, None), YRange(None, None))
        elif isinstance(area, XRange):
            area = Area(area, YRange(None, None))
        elif isinstance(area, YRange):
            area = Area(XRange(None, None), area)
        if isinstance(area, tuple):
            area = Area(XRange(None, None), YRange(*area))
        self._text = text
        self._font = font
        self._font_size = font_size
        self._geometry = area

    @property
    def geometry(self) -> Area:
        """Get the geometric properties of the line.

        Returns
        -------
        Area
            The area representing the line's bounds.
        """
        return self._geometry

    @property
    def c(self) -> Coord:
        """Get the center coordinate of the line.

        Returns
        -------
        Coord
            The center coordinate (x, y) of the line.
        """
        return self._geometry.c

    @property
    def corners(self) -> tuple:
        """Get the corner coordinates of the line.

        Returns
        -------
        tuple
            A tuple of tuples representing the line's corners in the format
            (((x_tl, y_tl), (x_tr, y_tr)), ((x_bl, y_bl), (x_br, y_br))).
        """
        return self._geometry.corners

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
        (((x_tl, y_tl), (x_tr, y_tr)), ((x_bl, y_bl), (x_br, y_br))) = self.corners
        x, y = self.c

        x_tl = self._fmt_point(x_tl)
        y_tl = self._fmt_point(y_tl)
        x_tr = self._fmt_point(x_tr)
        y_tr = self._fmt_point(y_tr)

        x_bl = self._fmt_point(x_bl)
        y_bl = self._fmt_point(y_bl)
        x_br = self._fmt_point(x_br)
        y_br = self._fmt_point(y_br)

        x = self._fmt_point(x)
        y = self._fmt_point(y)

        string += f"\t({x_tl}, {y_tl})\t({x_tr}, {y_tr})\n"
        string += f"\t\t({x}, {y})\n"
        string += f"\t({x_bl}, {y_bl})\t({x_br}, {y_br})\n"
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
            area=Area(
                XRange(bounds[0][0], bounds[0][1]), YRange(bounds[1][0], bounds[1][1])
            ),
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
    font: Optional[Font] = None
    font_size: Optional[FontSize] = None
    area: Optional[InputArea] = None


_line_set_font_regexp = r"(?P<font>[\w\-,]+)"
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


class PdfLineSet(PdfLine):
    """Set of lines with some carateristic"""

    @classmethod
    def from_dict(cls, data):
        ls = InputPdfLineSet(**data)
        return cls(
            font=ls.font,
            font_size=ls.font_size,
            area=Area.from_dict(ls.area.model_dump()),
            text=ls.text,
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
            area = Area(
                x_range=XRange(*_to_floats(x_range)),
                y_range=YRange(*_to_floats(y_range)),
            )
        elif tmp_range is not None:
            area = Area(
                x_range=XRange(None, None), y_range=YRange(*_to_floats(tmp_range))
            )

        fs = matched["font_size"]
        return cls(
            font=matched["font"],
            font_size=float(fs) if fs is not None else None,
            area=area,
            text=matched["text"],
        )

    def __contains__(self, other: ExtractedPdfLine):
        eq = True
        if self.text is not None:
            effective_text = self.text
            begin = False
            end = False
            if len(effective_text) >= 2 and effective_text.startswith(r"\^"):
                effective_text = effective_text[1:]  # Remove the backslash
            elif effective_text.startswith("^"):
                effective_text = effective_text[1:]
                begin = True

            # Check for escaped $ at the end
            if len(effective_text) >= 2 and effective_text.endswith(r"\$"):
                effective_text = (
                    effective_text[:-2] + effective_text[-1]
                )  # Remove the backslash
            elif effective_text.endswith("$"):
                effective_text = effective_text[:-1]
                end = True
            # Perform the checks
            if begin:
                eq = eq and other.text.startswith(effective_text)
            if end:
                eq = eq and other.text.endswith(effective_text)
            if not begin and not end:
                eq = eq and (effective_text in other.text)
        eq = eq and (self.font is None or self.font == other.font)
        if self.font_size is not None:
            if isinstance(self.font_size, FontSizeRange):
                eq = eq and other.font_size in self.font_size
            else:
                eq = eq and abs(self.font_size - other.font_size) <= 1e-4
        eq = eq and (self.geometry is None or self.geometry in other.geometry)
        return eq
