"""Utilities for selecting or deselecting lines or getting infos based of geometrical information"""

from typing import List, Tuple, TypeAlias, Optional
from enum import Flag, Enum, auto

from pydantic import BaseModel, PositiveFloat, model_validator

import freeports_lib
from freeports._internals.commons.enum_utils import flag_from_string, input_flags

Limits: TypeAlias = Tuple[float, float]
NullableState: TypeAlias = bool

PdfLine = freeports_lib.pdf_extract.select.PdfLine


"""Definition of types for identifying characteristics related to geometrical aspects of lines."""


class InputArea(BaseModel):
    """Represents a validated rectangular area with optional boundaries.

    This class defines a rectangular area with optional minimum and maximum coordinates
    for both x and y axes. It includes validation to ensure proper coordinate ranges.

    Attributes
    ----------
    x_min : Optional[PositiveFloat]
        Minimum x-coordinate of the area. Must be positive if provided.
    x_max : Optional[PositiveFloat]
        Maximum x-coordinate of the area. Must be positive if provided.
    y_min : Optional[PositiveFloat]
        Minimum y-coordinate of the area. Must be positive if provided.
    y_max : Optional[PositiveFloat]
        Maximum y-coordinate of the area. Must be positive if provided.

    Raises
    ------
    ValueError
        If x_max is not greater than x_min when both are provided.
    ValueError
        If y_max is not greater than y_min when both are provided.

    Examples
    --------
    >>> area = InputArea(x_min=0.0, x_max=10.0, y_min=0.0, y_max=5.0)
    >>> area.x_min
    0.0
    """

    x_min: Optional[PositiveFloat] = None
    x_max: Optional[PositiveFloat] = None
    y_min: Optional[PositiveFloat] = None
    y_max: Optional[PositiveFloat] = None

    @model_validator(mode="after")
    def validate_bounds(self) -> "InputArea":
        """Validate that maximum bounds are greater than minimum bounds.

        This validator ensures that when both minimum and maximum values are provided
        for either axis, the maximum is strictly greater than the minimum.

        Returns
        -------
        InputArea
            The validated instance.

        Raises
        ------
        ValueError
            If x_max is not greater than x_min when both are provided.
        ValueError
            If y_max is not greater than y_min when both are provided.
        """
        # Validate x-axis bounds
        if self.x_max is not None and self.x_min is not None:
            if self.x_max <= self.x_min:
                raise ValueError("x_max must be greater than x_min")

        # Validate y-axis bounds
        if self.y_max is not None and self.y_min is not None:
            if self.y_max <= self.y_min:
                raise ValueError("y_max must be greater than y_min")

        return self


class CellGeometry:
    """Represents a cell's bounding box with a tolerance for geometric comparisons.

    Parameters
    ----------
    bounds : tuple of (float, float, float, float)
        Bounding box as ``(x0, y0, x1, y1)``.
    tolerance : float
        Allowed deviation for geometric comparisons.
    """

    bounds: Tuple[float, float, float, float]
    tolerance: float

    def __init__(
        self, bounds: Tuple[float, float, float, float], tolerance: float
    ) -> None:
        """Initialize cell geometry.

        Parameters
        ----------
        bounds : tuple of (float, float, float, float)
            Bounding box coordinates.
        tolerance : float
            Tolerance value for geometric operations.
        """
        self.bounds = bounds
        self.tolerance = tolerance


class SplittingState(Enum):
    """Enum controlling how table cells may be split across multiple lines.

    Attributes
    ----------
    DISALLOW : SplittingState
        Cell splitting is not allowed.
    ALLOW_UP : SplittingState
        Upper cell can be split.
    ALLOW_DOWN : SplittingState
        Lower cell can be split.
    """

    DISALLOW = auto()
    ALLOW_UP = auto()
    ALLOW_DOWN = auto()


class RowConfig:
    """Configuration for a table row, specifying its vertical position limits.

    Parameters
    ----------
    limits : tuple of (float, float) or None, optional
        Minimum and maximum y-coordinate bounds for the row.
    """

    limits: Optional[Limits]

    def __init__(self, limits: Optional[Limits] = None) -> None:
        """Initialize row configuration.

        Parameters
        ----------
        limits : tuple of (float, float) or None, optional
            Vertical bounds for the row.
        """
        self.limits = limits


class ColumnConfig:
    """Configuration for a table column, specifying limits, nullability, and splitting.

    Parameters
    ----------
    limits : tuple of (float, float) or None, optional
        Minimum and maximum x-coordinate bounds for the column.
    nullable : bool or None, optional
        Whether the column may contain empty cells.
    splitting : SplittingState, optional
        Whether cell splitting is allowed for this column (default DISALLOW).
    """

    limits: Optional[Limits]
    nullable: Optional[NullableState]
    splitting: Optional[SplittingState]

    def __init__(
        self,
        limits: Optional[Limits] = None,
        nullable: Optional[NullableState] = None,
        splitting: Optional[NullableState] = SplittingState.DISALLOW,
    ) -> None:
        """Initialize column configuration.

        Parameters
        ----------
        limits : tuple of (float, float) or None, optional
            Horizontal bounds for the column.
        nullable : bool or None, optional
            Whether empty cells are allowed.
        splitting : SplittingState or None, optional
            Cell splitting behavior (default DISALLOW).
        """
        self.limits = limits
        self.nullable = nullable
        self.splitting = splitting


class TableConfig:
    """Configuration for a table with column and row definitions.

    Parameters
    ----------
    cols : list of ColumnConfig or ColumnConfig or None, optional
        Column configurations. A single ColumnConfig is wrapped in a list.
    rows : list of RowConfig or RowConfig or None, optional
        Row configurations. A single RowConfig is wrapped in a list.
    """

    cols: Optional[List[ColumnConfig]] = None
    rows: Optional[List[RowConfig]] = None

    def __init__(
        self,
        cols: Optional[ColumnConfig | List[ColumnConfig]] = None,
        rows: Optional[RowConfig | List[RowConfig]] = None,
    ) -> None:
        """Initialize table configuration.

        Parameters
        ----------
        cols : ColumnConfig, list of ColumnConfig, or None, optional
            Column configuration(s).
        rows : RowConfig, list of RowConfig, or None, optional
            Row configuration(s).
        """
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
    """Enum defining strategies for collapsing table rows.

    Attributes
    ----------
    GEOMETRY : CollapseAlgorithm
        Collapse based on geometric proximity only.
    PATTERN : CollapseAlgorithm
        Collapse based on pattern matching.
    GEOMETRY_PATTERN : CollapseAlgorithm
        Try geometry first, then pattern.
    PATTERN_GEOMETRY : CollapseAlgorithm
        Try pattern first, then geometry.
    """

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
    table_cfg: TableConfig = TableConfig(),
    algorithm_flags: TablePosAlgorithm = TablePosAlgorithm(0),
    collapse_alg: CollapseAlgorithm = CollapseAlgorithm.GEOMETRY,
    tolerance: float = 0,
    tolerance_mu: TablePosMeasureUnit = TablePosMeasureUnit.EM,
    company_col: Optional[int] = None,
    collapse: bool = False,
) -> List[Tuple[int, int]]:
    """Compute row and column indices for a list of PDF lines in a table.

    Parameters
    ----------
    lines : list of PdfLine
        PDF lines to assign table coordinates to.
    table_cfg : TableConfig, optional
        Table configuration with column/row definitions.
    algorithm_flags : TablePosAlgorithm, optional
        Algorithm flags controlling detection behavior.
    collapse_alg : CollapseAlgorithm, optional
        Strategy for collapsing multi-line cells (default GEOMETRY).
    tolerance : float, optional
        Tolerance value for position comparisons (default 0).
    tolerance_mu : TablePosMeasureUnit, optional
        Unit of the tolerance value (default EM).
    company_col : int or None, optional
        If set, disables splitting for the company-name column.
    collapse : bool, optional
        Whether to collapse multi-line cells into single rows (default False).

    Returns
    -------
    list of tuple of (int, int)
        List of ``(row_index, col_index)`` pairs for each input line.
    """
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


def get_groups(
    lines: List[PdfLine], treshold: float, vertical: bool = True
) -> List[int]:
    """Group lines by position proximity along the vertical or horizontal axis.

    Parameters
    ----------
    lines : list of PdfLine
        PDF lines to group.
    treshold : float
        Maximum distance between consecutive positions to stay in the same group.
    vertical : bool, optional
        If True, group by y-coordinate; otherwise by x-coordinate (default True).

    Returns
    -------
    list of int
        Group ID (0-based) for each input line.
    """
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
