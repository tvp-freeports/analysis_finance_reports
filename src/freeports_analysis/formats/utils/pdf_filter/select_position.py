"""Utilities for selecting or deselecting lines or getting infos based of geometrical information"""

from typing import List, Tuple
from enum import Flag, Enum, auto

from freeports_analysis.consts import flag_from_string, input_flags
from .pdf_parts import ExtractedPdfLine


class TablePosAlgorithm(Flag):
    """Algorithm flags for table position detection.

    Attributes
    ----------
    ROW : TablePosAlgorithm
        Calculate row positions (vertical axis)
    BIG_RULE : TablePosAlgorithm
        Use largest areas as rulers instead of smallest
    RULER_AREA : TablePosAlgorithm
        Match based on ruler area intersection
    TEST_POS : TablePosAlgorithm
        Match based on test element position
    """

    ROW = auto()
    BIG_RULE = auto()
    RULER_AREA = auto()
    TEST_POS = auto()

    @classmethod
    def from_dict(cls, v: str | list):
        """Create TablePosAlgorithm from string or list representation.

        Parameters
        ----------
        v : str | list
            String flag name or list of flag names

        Returns
        -------
        TablePosAlgorithm
            Combined flags object
        """
        flag_from_string(v, cls)


InputTablePosAlgorithm = input_flags(TablePosAlgorithm)


class TablePosMeasureUnit(Enum):
    """Measurement units for position tolerance.

    Attributes
    ----------
    EM : TablePosMeasureUnit
        Relative to font size (em units)
    PERC : TablePosMeasureUnit
        Percentage of element size
    PT : TablePosMeasureUnit
        Absolute points
    """

    EM = auto()
    PERC = auto()
    PT = auto()


def _area_position_algorithm(
    ruler_geometry: Tuple[float, Tuple[float, float]],
    test_geometry: Tuple[float, Tuple[float, float]],
    algorithm_flags: TablePosAlgorithm,
    abs_tolerance: float,
) -> bool:
    """Determine if test geometry matches ruler geometry using position-based algorithm.

    Parameters
    ----------
    ruler_geometry : Tuple[float, Tuple[float, float]]
        (position, (min_bound, max_bound)) of the ruler element
    test_geometry : Tuple[float, Tuple[float, float]]
        (position, (min_bound, max_bound)) of the test element
    algorithm_flags : TablePosAlgorithm
        Flags controlling the matching algorithm
    abs_tolerance : float
        Absolute tolerance for position matching

    Returns
    -------
    bool
        True if test geometry matches ruler geometry within tolerance
    """
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
    ruler_geometry: Tuple[float, Tuple[float, float]],
    test_geometry: Tuple[float, Tuple[float, float]],
    abs_tolerance: float,
) -> bool:
    """Determine if test geometry intersects ruler geometry using area-based algorithm.

    Parameters
    ----------
    ruler_geometry : Tuple[float, Tuple[float, float]]
        (position, (min_bound, max_bound)) of the ruler element
    test_geometry : Tuple[float, Tuple[float, float]]
        (position, (min_bound, max_bound)) of the test element
    abs_tolerance : float
        Absolute tolerance for boundary matching

    Returns
    -------
    bool
        True if test geometry intersects ruler geometry within tolerance
    """
    test_bounds = test_geometry[1]
    ruler_bounds = ruler_geometry[1]
    min_bound_t, max_bound_t = test_bounds
    min_bound_r, max_bound_r = ruler_bounds
    return (min_bound_r - abs_tolerance <= max_bound_t) and (
        min_bound_t - abs_tolerance <= max_bound_r
    )


def _algorithm_table_pos(
    ruler_geometry: Tuple[float, Tuple[float, float]],
    test_geometry: Tuple[float, Tuple[float, float]],
    algorithm_flags: TablePosAlgorithm,
    abs_tolerance: float,
) -> bool:
    """Main algorithm selector for table position matching.

    Parameters
    ----------
    ruler_geometry : Tuple[float, Tuple[float, float]]
        (position, (min_bound, max_bound)) of the ruler element
    test_geometry : Tuple[float, Tuple[float, float]]
        (position, (min_bound, max_bound)) of the test element
    algorithm_flags : TablePosAlgorithm
        Flags controlling which algorithm to use
    abs_tolerance : float
        Absolute tolerance for matching

    Returns
    -------
    bool
        True if test geometry matches ruler geometry according to selected algorithm
    """
    if (TablePosAlgorithm.RULER_AREA in algorithm_flags) and (
        TablePosAlgorithm.TEST_POS not in algorithm_flags
    ):
        return _area_intersection_algorithm(
            ruler_geometry, test_geometry, abs_tolerance
        )
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
    lines : List[ExtractedPdfLine]
        List of PDF text lines to analyze
    algorithm_flags : TablePosAlgorithm, optional
        Algorithm flags controlling position calculation behavior, by default TablePosAlgorithm(0)
    tolerance : float, optional
        Tolerance value for position matching, by default 0
    tolerance_mu : TablePosMeasureUnit, optional
        Tolerance measurement unit, by default TablePosMeasureUnit.EM

    Returns
    -------
    List[int]
        A list of indexes corresponding to each line's position in the table

    Notes
    -----
    The algorithm:
    - Determines whether to calculate row or column indexes based on algorithm flags
    - Uses rulers (largest or smallest areas) as reference points
    - Applies tolerance-based matching for position classification
    - Sorts and maps positions to create consistent indexes
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
        bounds = a.bounds
        xmin = bounds[0]
        xmax = bounds[2]
        width = xmax - xmin
        x_center = (xmin + xmax) / 2
        return (xmin, xmax), width, x_center

    def _get_geometrical_vertical_infos(a):
        bounds = a.bounds
        ymin = bounds[1]
        ymax = bounds[3]
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
