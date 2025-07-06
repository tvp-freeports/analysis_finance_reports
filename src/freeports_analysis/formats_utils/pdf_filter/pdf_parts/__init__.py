"""Pdf xml parts in a friendly format (custom python classes)."""

from lxml import etree
from .font import Font, TextSize
from ..xml.position import get_bounds
from .position import Area, XRange, YRange, Coord


class ExtractedPdfLine:
    """A class representing a line extracted from a PDF XML structure.

    This class provides a friendly interface to access geometric properties,
    font information, and text size of a line in a PDF document.

    Parameters
    ----------
    blk : etree.Element
        The XML element containing the line data.
    """

    def __init__(self, blk: etree.Element):
        """Initialize the ExtractedPdfLine from an XML element.

        Parameters
        ----------
        blk : etree.Element
            The XML element containing the line data.
        """
        self._blk = blk
        bounds = get_bounds(blk)
        self._geometry = Area(
            XRange(bounds[0][0], bounds[0][1]), YRange(bounds[1][0], bounds[1][1])
        )
        self._font = Font(blk.xpath(".//font/@name")[0])
        self._txt_size = TextSize(blk.xpath(".//font/@size")[0])

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
    def text_size(self) -> TextSize:
        """Get the text size used in the line.

        Returns
        -------
        TextSize
            The text size used in the line.
        """
        return self._txt_size

    @property
    def xml_blk(self) -> etree.Element:
        """Get the original XML element containing the line data.

        Returns
        -------
        etree.Element
            The original XML element containing the line data.
        """
        return self._blk

    def __str__(self) -> str:
        """Return a formatted string representation of the line.

        Returns
        -------
        str
            Formatted string showing font, text size, and coordinates.
        """
        string = f"Line PDF - Font '{self.font}' [{self.text_size}]\n"
        (((x_tl, y_tl), (x_tr, y_tr)), ((x_bl, y_bl), (x_br, y_br))) = self.corners
        x, y = self.c
        string += f"\t({x_tl:.3f}, {y_tl:.3f})\t({x_tr:.3f}, {y_tr:.3f})\n"
        string += f"\t\t({x:.3f}, {y:.3f})\n"
        string += f"\t({x_bl:.3f}, {y_bl:.3f})\t({x_br:.3f}, {y_br:.3f})\n"
        return string
