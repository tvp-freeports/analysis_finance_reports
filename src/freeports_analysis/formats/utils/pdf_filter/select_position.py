"""Utilities for selecting or deselecting lines or getting infos based of geometrical information"""

from typing import List, Annotated
from enum import Flag, Enum, auto
from pydantic import BeforeValidator
import pandas as pd

from freeports_analysis.consts import flag_from_string, InputFlags
from .pdf_parts import ExtractedPdfLine


class TablePosAlgorithm(Flag):
    ROW = auto()
    BIG_RULE = auto()
    RULER_AREA = auto()
    TEST_POS = auto()

    @classmethod
    def from_dict(cls, v: str | list):
        flag_from_string(v, cls)


InputTablePosAlgorithm = InputFlags(TablePosAlgorithm)


class TablePosMeasureUnit(Enum):
    EM = auto()
    PERC = auto()
    PT = auto()


def _area_position_algorithm(
    ruler_geometry, test_geometry, algorithm_flags, abs_tolerance
):
    test_pos, test_bounds = test_geometry
    ruler_pos, ruler_bounds = ruler_geometry
    if TablePosAlgorithm.RULER_AREA in algorithm_flags:
        match_pos = test_pos
        min_bound, max_bound = ruler_bounds
    else:
        match_pos = ruler_pos
        min_bound, max_bound = test_bounds

    return (min_bound - abs_tolerance) <= match_pos <= (max_bound + abs_tolerance)


def _area_intersection_algorithm(
    ruler_geometry, test_geometry, algorithm_flags, abs_tolerance
):
    test_bounds = test_geometry[1]
    ruler_bounds = ruler_geometry[1]
    min_bound_t, max_bound_t = test_bounds
    min_bound_r, max_bound_r = ruler_bounds
    return (min_bound_r - abs_tolerance <= max_bound_t) and (
        min_bound_t - abs_tolerance <= max_bound_r
    )


def _algorithm_table_pos(ruler_geometry, test_geometry, algorithm_flags, abs_tolerance):
    if (TablePosAlgorithm.RULER_AREA in algorithm_flags) and (
        TablePosAlgorithm.TEST_POS not in algorithm_flags
    ):
        return _area_intersection_algorithm(
            ruler_geometry, test_geometry, algorithm_flags, abs_tolerance
        )
    else:
        return _area_position_algorithm(
            ruler_geometry, test_geometry, algorithm_flags, abs_tolerance
        )


def get_table_positions(
    lines: List[ExtractedPdfLine],
    algorithm_flags: TablePosAlgorithm = TablePosAlgorithm(0),
    tolerance: float = 0,
    tolerance_mu: TablePosMeasureUnit = TablePosMeasureUnit.EM,
) -> List[int]:
    """Compute either row or column indexes for areas in a tabular layout.

    Parameters
    ----------
    return_columns : bool
        Whether to return column indexes (True) or row indexes (False)
    areas : list of Poligons
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
    areas = [line.area for line in lines]
    font_sizes = [line.font_size for line in lines]
    rulers = []
    # Choose min/max function based on small_rule
    choose = max if TablePosAlgorithm.BIG_RULE in algorithm_flags else min
    return_col = TablePosAlgorithm.ROW not in algorithm_flags

    def _get_geometrical_horizontal_infos(a):
        xmin, ymin, xmax, ymax = a.bounds
        width = xmax - xmin
        x_center = (xmin + xmax) / 2
        return (xmin, xmax), width, x_center

    def _get_geometrical_vertical_infos(a):
        xmin, ymin, xmax, ymax = a.bounds
        height = ymax - ymin
        y_center = (ymin + ymax) / 2
        return (ymin, ymax), height, y_center

    geometrical_infos = list(
        map(
            _get_geometrical_horizontal_infos
            if return_col
            else _get_geometrical_vertical_infos,
            areas,
        )
    )

    while None in indexes:
        curr_idx = len(rulers)
        # Get unindexed areas
        unindexed = [
            (i, geometrical_infos[i][1])
            for i, area in enumerate(areas)
            if indexes[i] is None
        ]

        # Select ruler for this axis
        ruler_idx, _ = choose(unindexed, key=lambda x: x[1])
        # Get ruler bounds and position
        ruler_bounds, _, ruler_pos = geometrical_infos[ruler_idx]
        rulers.append((curr_idx, ruler_pos))

        # Classify areas
        ruler_geometry = (ruler_pos, ruler_bounds)
        for i, table_pos in enumerate(indexes):
            if table_pos is not None:
                continue
            (test_bounds, test_area, test_pos) = geometrical_infos[i]
            test_geometry = (test_pos, test_bounds)
            effective_tolerance = 0
            if tolerance_mu == TablePosMeasureUnit.PT:
                effective_tolerance = tolerance
            elif tolerance_mu == TablePosMeasureUnit.PERC:
                effective_tolerance = tolerance * test_area
            elif tolerance_mu == TablePosMeasureUnit.EM:
                effective_tolerance = tolerance * font_sizes[i]

            if _algorithm_table_pos(
                ruler_geometry=ruler_geometry,
                test_geometry=test_geometry,
                algorithm_flags=algorithm_flags,
                abs_tolerance=effective_tolerance,
            ):
                indexes[i] = curr_idx

    # Sort rulers and create mapping
    mapping = {
        old: new for new, (old, _) in enumerate(sorted(rulers, key=lambda x: x[1]))
    }

    # Apply mapping
    return [mapping[idx] for idx in indexes]
