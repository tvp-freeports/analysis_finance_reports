from typing import Tuple, TypeAlias
from .generic import Range


class XRange(Range):
    @property
    def x0(self):
        return self.start

    @property
    def x1(self):
        return self.end


class YRange(Range):
    @property
    def y0(self):
        return self.start

    @property
    def y1(self):
        return self.end


Coord: TypeAlias = Tuple[float, float]


class Area:
    def __init__(self, x_range: XRange, y_range: YRange):
        self._x_range = x_range
        self._y_range = y_range

    @property
    def x_bounds(self):
        return self._x_range

    @property
    def y_bounds(self):
        return self._y_range

    @property
    def c(self) -> Coord:
        x = (self.x_bounds.x1 + self.x_bounds.x0) / 2.0
        y = (self.y_bounds.y1 + self.y_bounds.y0) / 2.0
        return (x, y)

    @property
    def corners(self):
        x0 = self.x_bounds.x0
        x1 = self.x_bounds.x1
        y0 = self.y_bounds.y0
        y1 = self.y_bounds.y1
        return (((x0, y0), (x1, y0)), ((x0, y1), (x1, y1)))

    @property
    def width(self):
        return self.x_bounds.size

    @property
    def height(self):
        return self.y_bounds.size

    def __str__(self):
        string = ""
        (((x_tl, y_tl), (x_tr, y_tr)), ((x_bl, y_bl), (x_br, y_br))) = self.corners
        x, y = self.c
        string += f"\t({x_tl:.3f}, {y_tl:.3f})\t({x_tr:.3f}, {y_tr:.3f})\n"
        string += f"\t\t({x:.3f}, {y:.3f})\n"
        string += f"\t({x_bl:.3f}, {y_bl:.3f})\t({x_br:.3f}, {y_br:.3f})\n"
        return string
