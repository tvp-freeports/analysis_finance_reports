"""Utilities for selecting or deselecting lines or getting infos based of geometrical information"""

import freeports_lib
from typing import List, Tuple, TypeAlias, Optional
from enum import Flag, Enum, auto

from freeports_analysis.consts import flag_from_string, input_flags

Limits: TypeAlias = Tuple[float, float]
NullableState: TypeAlias = bool


class CellGeometry:
    bounds: Tuple[float, float, float, float]
    tolerance: float

    def __init__(self, bounds, tolerance):
        self.bounds = bounds
        self.tolerance = tolerance


class SplittingState(Enum):
    DISALLOW = auto()
    ALLOW_UP = auto()
    ALLOW_DOWN = auto()


class RowConfig:
    limits: Optional[Limits]


class ColumnConfig:
    limits: Optional[Limits] = None
    nullable: Optional[NullableState] = None
    splitting: Optional[SplittingState] = None

    def __init__(self):
        self.splitting = SplittingState.DISALLOW


class TableConfig:
    cols: Optional[List] = None
    rows: Optional[List] = None


class CollapseAlgorithm(Enum):
    GEOMETRY = auto()
    PATTERN = auto()
    GEOMETRY_PATTERN = auto()
    PATTERN_GEOMETRY = auto()


class TablePosAlgorithm(Flag):
    """Algorithm flags for table position detection.

    Attributes
    ----------
    RETURN_ROWS : TablePosAlgorithm
        Calculate row positions (vertical axis)
    BIG_CELL_RULE : TablePosAlgorithm
        Use largest areas as rulers instead of smallest
    USE_RULER_AREA : TablePosAlgorithm
        Match based on ruler area intersection
    USE_TES_POS : TablePosAlgorithm
        Match based on test element position
    """

    RETURN_ROWS = auto()
    BIG_CELL_RULE = auto()
    USE_RULER_AREA = auto()
    USE_TES_POS = auto()

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
        return flag_from_string(v, cls)


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


def get_table_coordinates(
    lines: List[ExtractedPdfLine],
    table_cfg=TableConfig(),
    algorithm_flags: TablePosAlgorithm = TablePosAlgorithm(0),
    collapse_alg=CollapseAlgorithm.GEOMETRY,
    tolerance: float = 0,
    tolerance_mu: TablePosMeasureUnit = TablePosMeasureUnit.EM,
    company_col: Optional[int] = None,
    collapse: bool = False,
) -> List[Tuple[int, int]]:
    cells = [
        CellGeometry(
            l.bbox,
            tolerance
            if (tolerance_mu == TablePosMeasureUnit.PT)
            else tolerance * (l.bounds[2] - l.bounds[0])
            if (tolerance_mu == TablePosMeasureUnit.PERC)
            else tolerance * l.font_size
            if (tolerance_mu == TablePosMeasureUnit.EM)
            else 0,
        )
        for l in lines
    ]
    coords = freeports_lib.pdf_extract.tabularizer.get_table_coordinates(
        cells, algorithm_flags, table_cfg
    )
    if table_cfg.cols is None and company_col is not None:
        _, cols = zip(*coords)
        n_cols = max(*cols)
        table_cfg.cols = [ColumnConfig()] * n_cols
        table_cfg.cols[company_col].splitting = None

    if collapse:
        coords = freeports_lib.pdf_extract.tabularizer.collapse_table_rows(
            coords, table_cfg, collapse_alg
        )
    return coords
