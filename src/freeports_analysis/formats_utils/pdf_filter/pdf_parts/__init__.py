"""Pdf xml parts in a friendly format (custom python classes)."""

from typing import Optional, Tuple
from lxml import etree
from freeports_analysis.i18n import _
from .font import Font, FontSize
from ..xml.position import get_bounds
from .position import Area, XRange, YRange, Coord


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
    def text(self) -> FontSize:
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


class PdfLineSet(PdfLine):
    """Set of lines with some carateristic"""

    def __contains__(self, other: ExtractedPdfLine):
        eq = True
        eq = eq and self.text is None or self.text in other.text
        eq = eq and self.font is None or self.font == other.font
        eq = (
            eq
            and self.font_size is None
            or abs(self.font_size - other.font_size) <= 1e-4
        )
        eq = eq and self.geometry is None or self.geometry in other.geometry
        return eq
