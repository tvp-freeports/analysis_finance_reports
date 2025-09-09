"""Definition of types for identify characteristic related with geometrical aspects of the line."""

from typing import Tuple, TypeAlias, Optional, Annotated
from pydantic import BaseModel, PositiveFloat, model_validator, AfterValidator
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


class InputArea(BaseModel):
    x_min: Optional[PositiveFloat] = None
    x_max: Optional[PositiveFloat] = None
    y_min: Optional[PositiveFloat] = None
    y_max: Optional[PositiveFloat] = None

    @model_validator(mode="after")  # Dopo la validazione dei singoli campi
    def validate_bounds(self):
        if self.x_max is not None and self.x_min is not None:
            if self.x_max <= self.x_min:
                raise ValueError("x_max must be greater than x_min")
        if self.y_max is not None and self.y_min is not None:
            if self.y_max <= self.y_min:
                raise ValueError("y_max must be greater than y_min")
        return self


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

    @classmethod
    def from_dict(cls, data):
        area = InputArea(**data)
        return cls(
            x_range=XRange(area.x_min, area.x_max),
            y_range=YRange(area.y_min, area.y_max),
        )

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
        if self.x_bounds.x1 is None or self.x_bounds.x0 is None:
            x = None
        else:
            x = (self.x_bounds.x1 + self.x_bounds.x0) / 2.0
        if self.y_bounds.y1 is None or self.y_bounds.y0 is None:
            y = None
        else:
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

    def __contains__(self, other) -> bool:
        """Check if the area is contained in another area

        Parameters
        ----------
        other : Area
            The area to check containment

        Returns
        -------
        bool
            The area is contained
        """
        return self.x_bounds in other.x_bounds and self.y_bounds in other.y_bounds

    def _fmt_point(self, coor):
        return "" if coor is None else f"{coor:.3f}"

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
        string += f"|({x_tl}, {y_tl})\t({x_tr}, {y_tr})\n"
        string += f"|\t({x}, {y})\n"
        string += f"|({x_bl}, {y_bl})\t({x_br}, {y_br})\n"
        return string


AreaDict = Annotated[
    InputArea, AfterValidator(lambda x: Area.from_dict(x.model_dump()))
]
