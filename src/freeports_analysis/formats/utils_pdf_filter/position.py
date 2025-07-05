from typing import Optional, Tuple, List, TypeAlias
from lxml import etree

from .xml import Range


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
    def c(self):
        x = (self.x_bounds.x1 - self.x_bounds.x0) / 2.0
        y = (self.y_bounds.y1 - self.y_bounds.y0) / 2.0
        return Coord(x, y)

    @property
    def corners(self):
        x0 = self.x_bounds.x0
        x1 = self.x_bounds.x1
        y0 = self.y_bounds.y0
        y1 = self.y_bounds.y1
        return ((Coord(x0, y0), Coord(x1, y0)), (Coord(x0, y1), Coord(x1, y1)))

    @property
    def width(self):
        return self.x_bounds.size

    @property
    def height(self):
        return self.y_bounds.size

    def __str__(self):
        ((tl, tr), (bl, br)) = self.corners
        string += f"{tl}\t{tr}\n"
        string += f"\t{self.c}\n"
        string += f"{bl}\t{br}\n"
        return string


def select_inside(areas: List[Area], bounds: XRange | YRange):
    coord = 0 if isinstance(bounds, XRange) else 1
    return [a for a in areas if a.c[coord] in bounds]


def select_outside(areas: List[Area], bounds: XRange | YRange):
    coord = 0 if isinstance(bounds, XRange) else 1
    return [a for a in areas if a.c[coord] not in bounds]


def get_table_indexes(
    areas: List[Area],
    return_columns: bool = True,
    small_rule: bool = True,
    use_ruler_pos: bool = True,
) -> List[int]:
    """Compute either row or column indexes for areas in a tabular layout.

    Parameters
    ----------
    return_columns : bool
        Whether to return column indexes (True) or row indexes (False)
    areas : list of Area
        List of areas representing table cells
    small_rule : bool
        Whether to use smallest (True) or largest (False) dimension for rulers
    use_ruler_pos : bool
        Whether to use ruler position (True) or bounds (False) for classification

    Returns
    -------
    list of int
        A list of indexes corresponding to each area
    """
    # Initialize indexes
    indexes = [None for _ in areas]
    rulers = []

    # Choose min/max function based on small_rule
    choose = min if small_rule else max

    while None in indexes:
        curr_idx = len(rulers)
        # Get unindexed areas
        unindexed = [
            (i, area.width if return_columns else area.height)
            for i, area in enumerate(areas)
            if indexes[i] is None
        ]

        # Select ruler for this axis
        ruler_idx, _ = choose(unindexed, key=lambda x: x[1])
        ruler_area = areas[ruler_idx]

        # Get ruler bounds and position
        ruler_bounds = ruler_area.x_bounds if return_columns else ruler_area.y_bounds
        ruler_pos = ruler_area.c[0] if return_columns else ruler_area.c[1]
        rulers.append((curr_idx, ruler_pos))

        # Classify areas
        for i, area in enumerate(areas):
            if indexes[i] is not None:
                continue

            if use_ruler_pos:
                # Use ruler position classification
                test_pos = area.c[0] if return_columns else area.c[1]
                if test_pos in ruler_bounds:
                    indexes[i] = curr_idx
            else:
                # Use bounds classification
                test_bounds = area.x_bounds if return_columns else area.y_bounds
                if ruler_pos in test_bounds:
                    indexes[i] = curr_idx

    # Sort rulers and create mapping
    sorted_rulers = sorted(rulers, key=lambda x: x[1])
    mapping = {old: new for new, (old, _) in enumerate(sorted_rulers)}

    # Apply mapping
    return [mapping[idx] for idx in indexes]
