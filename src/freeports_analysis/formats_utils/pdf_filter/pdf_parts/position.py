"""Definition of types for identify characteristic related with geometrical aspects of the line."""

from typing import Tuple, TypeAlias
from .generic import Range


class XRange(Range):
    """A class representing a range along the X-axis.

    Attributes
    ----------
    x0 : float
        Alias for start of the range.
    x1 : float
        Alias for end of the range.
    """

    @property
    def x0(self) -> float:
        """Get the start value of the X-range.
        Returns
        -------
        float
            The start value of the range.
        """
        return self.start

    @property
    def x1(self) -> float:
        """Get the end value of the X-range.
        Returns
        -------
        float
            The end value of the range.
        """
        return self.end


class YRange(Range):
    """A class representing a range along the Y-axis.

    Attributes
    ----------
    y0 : float
        Alias for start of the range.
    y1 : float
        Alias for end of the range.
    """

    @property
    def y0(self) -> float:
        """Get the start value of the Y-range.

        Returns
        -------
        float
            The start value of the range.
        """
        return self.start

    @property
    def y1(self) -> float:
        """Get the end value of the Y-range.

        Returns
        -------
        float
            The end value of the range.
        """
        return self.end


Coord: TypeAlias = Tuple[float, float]


class Area:
    """A class representing a 2D area defined by X and Y ranges.

    Attributes
    ----------
    x_bounds : XRange
        The range along the X-axis.
    y_bounds : YRange
        The range along the Y-axis.
    c : Coord
        The center coordinate of the area.
    corners : tuple
        The corner coordinates of the area.
    width : float
        The width of the area (x_bounds.size).
    height : float
        The height of the area (y_bounds.size).
    """

    def __init__(self, x_range: XRange, y_range: YRange):
        """Initialize the Area with X and Y ranges.

        Parameters
        ----------
        x_range : XRange
            The range along the X-axis.
        y_range : YRange
            The range along the Y-axis.
        """
        self._x_range = x_range
        self._y_range = y_range

    @property
    def x_bounds(self) -> XRange:
        """Get the X-range bounds.

        Returns
        -------
        XRange
            The range along the X-axis.
        """
        return self._x_range

    @property
    def y_bounds(self) -> YRange:
        """Get the Y-range bounds.

        Returns
        -------
        YRange
            The range along the Y-axis.
        """
        return self._y_range

    @property
    def c(self) -> Coord:
        """Calculate the center coordinate of the area.

        Returns
        -------
        Coord
            The (x, y) center coordinate.
        """
        x = (self.x_bounds.x1 + self.x_bounds.x0) / 2.0
        y = (self.y_bounds.y1 + self.y_bounds.y0) / 2.0
        return (x, y)

    @property
    def corners(self) -> Tuple[Tuple[Coord, Coord], Tuple[Coord, Coord]]:
        """Get the corner coordinates of the area.

        Returns
        -------
        tuple
            The corner coordinates in the format (((x0,y0), (x1,y0)), ((x0,y1), (x1,y1))).
        """
        x0 = self.x_bounds.x0
        x1 = self.x_bounds.x1
        y0 = self.y_bounds.y0
        y1 = self.y_bounds.y1
        return (((x0, y0), (x1, y0)), ((x0, y1), (x1, y1)))

    @property
    def width(self) -> float:
        """Get the width of the area.

        Returns
        -------
        float
            The width (x_bounds.size).
        """
        return self.x_bounds.size

    @property
    def height(self) -> float:
        """Get the height of the area.

        Returns
        -------
        float
            The height (y_bounds.size).
        """
        return self.y_bounds.size

    def __str__(self) -> str:
        """Return a string representation of the area.

        Returns
        -------
        str
            Formatted string showing corner coordinates and center.
        """
        string = ""
        (((x_tl, y_tl), (x_tr, y_tr)), ((x_bl, y_bl), (x_br, y_br))) = self.corners
        x, y = self.c
        string += f"|({x_tl:.3f}, {y_tl:.3f})\t({x_tr:.3f}, {y_tr:.3f})\n"
        string += f"|\t({x:.3f}, {y:.3f})\n"
        string += f"|({x_bl:.3f}, {y_bl:.3f})\t({x_br:.3f}, {y_br:.3f})\n"
        return string
