from typing import List

from .pdf_parts import ExtractedPdfLine
from .pdf_parts.position import XRange, YRange


def select_inside(
    lines: List[ExtractedPdfLine], bounds: XRange | YRange
) -> List[ExtractedPdfLine]:
    coord = 0 if isinstance(bounds, XRange) else 1
    return [line for line in lines if line.c[coord] in bounds]


def select_outside(
    lines: List[ExtractedPdfLine], bounds: XRange | YRange
) -> List[ExtractedPdfLine]:
    coord = 0 if isinstance(bounds, XRange) else 1
    return [l for l in lines if l.c[coord] not in bounds]


def get_table_positions(
    lines: List[ExtractedPdfLine],
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
    indexes = [None for _ in lines]
    areas = [l.geometry for l in lines]
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
                test_bounds = area.x_bounds if return_columns else area.y_bounds
                if ruler_pos in test_bounds:
                    indexes[i] = curr_idx
            else:
                test_pos = area.c[0] if return_columns else area.c[1]
                if test_pos in ruler_bounds:
                    indexes[i] = curr_idx

    # Sort rulers and create mapping
    sorted_rulers = sorted(rulers, key=lambda x: x[1])
    mapping = {old: new for new, (old, _) in enumerate(sorted_rulers)}

    # Apply mapping
    return [mapping[idx] for idx in indexes]
