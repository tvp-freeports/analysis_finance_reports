"""Utilities for selecting or deselecting lines or getting infos based of geometrical information

``InputArea``, ``CellGeometry``, ``RowConfig``, ``ColumnConfig``, ``TableConfig``, and
``get_groups`` are now implemented in Rust — see
``packages/freeports_engine/src/pdf_extract/position.rs`` and
``analysis_finance_reports/agent-memory/rust-rewrite-plan.md``. ``SplittingState``,
``CollapseAlgorithm``, ``TablePosAlgorithm``, ``TablePosMeasureUnit``, and
``get_table_coordinates`` stay in Python (user confirmed, 2026-08-19): the Enums are read by
Rust via generic `.name`/iteration duck-typing that a from-scratch Rust pyclass wouldn't get for
free, ``TablePosAlgorithm`` carries real Flag-parsing machinery with no Rust counterpart, and
``get_table_coordinates`` itself is mostly thin glue already calling straight into Rust (formerly
the separate ``freeports_lib`` crate; merged into ``freeports_engine`` in Fase E — see
``analysis_finance_reports/agent-memory/rust-native-binary-plan.md``). Three real (previously dormant)
bugs in ``get_table_coordinates`` were found and fixed at the root here, before porting the rest
of this module.

The original (pre-Rust-port) ``_LegacyInputArea``/``_LegacyCellGeometry``/``_LegacyRowConfig``/
``_LegacyColumnConfig``/``_LegacyTableConfig``/``_legacy_get_groups`` dead-code bodies this module
used to keep for reference were removed during the freeports_core -> freeports_engine
consolidation (see
``analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md``). ``Limits``/
``NullableState`` (the only two of this module's pre-existing type aliases those dead bodies used)
are kept: they are re-exported as part of the public ``freeports.utils.pdf_extract`` surface
(``freeports/utils/pdf_extract.py``), which stays stable across this consolidation.
"""

from typing import List, Tuple, Optional, TypeAlias
from enum import Flag, Enum, auto

from freeports import _native
from freeports._internals.commons.enum_utils import flag_from_string, input_flags

Limits: TypeAlias = Tuple[float, float]
NullableState: TypeAlias = bool

PdfLine = _native.core.PdfLine

InputArea = _native.core.InputArea
CellGeometry = _native.core.CellGeometry
RowConfig = _native.core.RowConfig
ColumnConfig = _native.core.ColumnConfig
TableConfig = _native.core.TableConfig
get_groups = _native.core.get_groups


"""Definition of types for identifying characteristics related to geometrical aspects of lines."""


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
    table_cfg: Optional[TableConfig] = None,
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
    table_cfg : TableConfig or None, optional
        Table configuration with column/row definitions. When omitted, a fresh default is
        used for this call — never reused or mutated across calls.
    algorithm_flags : TablePosAlgorithm, optional
        Algorithm flags controlling detection behavior.
    collapse_alg : CollapseAlgorithm, optional
        Strategy for collapsing multi-line cells (default GEOMETRY).
    tolerance : float, optional
        Tolerance value for position comparisons (default 0).
    tolerance_mu : TablePosMeasureUnit, optional
        Unit of the tolerance value (default EM).
    company_col : int or None, optional
        If set (and ``table_cfg.cols`` wasn't already given explicitly), builds a default
        column config where only this column allows splitting across multiple lines (company
        names can be long enough to wrap) — every other column disallows it.
    collapse : bool, optional
        Whether to collapse multi-line cells into single rows (default False).

    Returns
    -------
    list of tuple of (int, int)
        List of ``(row_index, col_index)`` pairs for each input line.
    """
    if table_cfg is None:
        table_cfg = TableConfig()
    cells = [
        CellGeometry(
            l.bbox,
            tolerance
            if (tolerance_mu == TablePosMeasureUnit.PT)
            else tolerance * (l.bbox[2] - l.bbox[0])
            if (tolerance_mu == TablePosMeasureUnit.PERC)
            else tolerance * l.font_size
            if (tolerance_mu == TablePosMeasureUnit.EM)
            else 0,
        )
        for l in lines
    ]
    coords = _native.core.get_table_coordinates(cells, algorithm_flags, table_cfg)
    if table_cfg.cols is None and company_col is not None:
        _, cols = zip(*coords)
        n_cols = max(*cols)
        # Each column needs its own ColumnConfig instance — `[ColumnConfig()] * n_cols` would
        # alias every element to the same object, so setting `.splitting` on `company_col` below
        # would silently apply to every column instead of just that one.
        table_cfg.cols = [ColumnConfig() for _ in range(n_cols)]
        table_cfg.cols[company_col].splitting = None

    if collapse:
        coords = _native.core.collapse_table_rows(coords, table_cfg, collapse_alg)
    return coords
