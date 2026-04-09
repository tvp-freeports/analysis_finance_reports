"""Utilities for selecting or deselecting lines or getting infos based of geometrical information"""

import freeports_lib
from typing import List, Tuple, TypeAlias, Optional
from enum import Flag, Enum, auto

from freeports_analysis.consts import flag_from_string, input_flags

Limits: TypeAlias = Tuple[float, float]
NullableState: TypeAlias = bool

PdfLine = freeports_lib.pdf_extract.select.PdfLine


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

    def __init__(self, limits: Optional[Limits] = None):
        self.limits = limits


class ColumnConfig:
    limits: Optional[Limits]
    nullable: Optional[NullableState]
    splitting: Optional[SplittingState]

    def __init__(
        self,
        limits: Optional[Limits] = None,
        nullable: Optional[NullableState] = None,
        splitting: Optional[NullableState] = SplittingState.DISALLOW,
    ):
        self.limits = limits
        self.nullable = nullable
        self.splitting = splitting


class TableConfig:
    cols: Optional[List] = None
    rows: Optional[List] = None

    def __init__(self, cols=None, rows=None):
        if cols is None:
            self.cols = cols
        elif isinstance(cols, ColumnConfig):
            self.cols = [cols]
        else:
            self.cols = []
            for c in cols:
                self.cols.append(c)
        if rows is None:
            self.rows = rows
        elif isinstance(rows, RowConfig):
            self.rows = [rows]
        else:
            self.rows = []
            for c in rows:
                self.rows.append(c)


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
    USE_TEST_POS : TablePosAlgorithm
        Match based on test element position
    """

    RETURN_ROWS = auto()
    BIG_CELL_RULE = auto()
    USE_RULER_AREA = auto()
    USE_TEST_POS = auto()

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
    lines: List[PdfLine],
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


def get_groups(lines, treshold, vertical=True):
    geoindex = 1 if vertical else 0
    sorted_lines = sorted(lines, key=lambda x: x.bbox[geoindex])
    group_id = 0
    groups = []
    a = sorted_lines[0].bbox[geoindex]
    for l in sorted_lines:
        b = l.bbox[geoindex]
        if abs(b - a) >= treshold:
            group_id += 1
        a = b
        groups.append(group_id)
    return groups
