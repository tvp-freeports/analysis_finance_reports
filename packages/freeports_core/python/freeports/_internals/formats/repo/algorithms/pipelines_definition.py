"""Common utilities and data structures for algorithm pipeline management.

This module provides shared functionality for handling format and pipeline
identifiers, including validation schemas and index manipulation utilities.

``PipelineSegement`` and its 3 subclasses (``PdfExtractSegment``/``TextFilterSegment``/
``DeserializeSegment``), and ``Pipeline`` itself, are now implemented in Rust — see
``packages/freeports_engine/src/pipeline.rs`` and
``analysis_finance_reports/agent-memory/rust-native-binary-plan.md`` (Phase B). ``Pipeline`` is
part of the public format-authoring API (``freeports.core.Pipeline``, constructed directly by
every format definition in ``analysis_finance_reports_formats``) — the constructor signature and
calling convention are unchanged. The ID-format/schema validation utilities in this file
(``check_id_column``, ``add_format_name``, etc.) stay Python — pandas/pandera-dependent, same
category as the other input/output pieces still pending the pandas-vs-polars decision. The
original (pre-Rust-port) ``_LegacyPipelineSegement``/``_LegacyPdfExtractSegment``/
``_LegacyTextFilterSegment``/``_LegacyDeserializeSegment``/``_LegacyPipeline`` dead-code class
bodies this module used to keep for reference were removed during the freeports_core ->
freeports_engine consolidation (see
``analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md``) — a workspace-wide
grep confirmed nothing outside this module itself ever referenced them.
"""

import pandera.pandas as pa
import pandas as pd
from enum import Enum
from typing import Optional, Callable, Any, Set, Iterator, List

from freeports import _native
from freeports._internals.formats.repo.metadata import FORMAT_NAME_REGEXP

PdfExtractSegment = _native.core.PdfExtractSegment
TextFilterSegment = _native.core.TextFilterSegment
DeserializeSegment = _native.core.DeserializeSegment
Pipeline = _native.core.Pipeline


class PipeIndexMode(Enum):
    """Strategy for determining pipe indices within a pipeline group."""

    INFER = "infer"
    """Infer index from row order within group."""
    EXPLICIT = "explicit"
    """Read index explicitly from the ID string."""


class MissingIndexPolicy(Enum):
    """Policy for handling missing pipe indices in explicit mode."""

    ZERO = "zero"
    """Fill missing indices with 0."""
    INFER = "infer"
    """Infer missing indices from row order within group."""


class FKRelation(Enum):
    """Type of foreign key relationship between tables."""

    ONE_TO_MAYBE = "one to maybe one"
    """One-to-zero-or-one relationship."""
    ONE_TO_ONE = "one to one"
    """One-to-one relationship."""
    ONE_TO_MANY = "one to many"
    """One-to-many relationship."""


# ============================================================
# Regular expressions
# ============================================================

pipeline_name_regexp: str = r"[0-9a-z_]*"
pipeline_regexp: str = rf"\(({pipeline_name_regexp})\)"
index_regexp: str = r"/([0-9]+)"


class IDFormat(Enum):
    """Expected format of ID column for validation."""

    EXPANDIBLE_NO_INDEX = 0
    """Format name with optional pipeline, no index suffix."""
    EXPANDIBLE = 1
    """Format name with optional pipeline and optional index suffix."""
    COMPLETE = 2
    """Format name with required pipeline and required index suffix."""


def check_id_column(id_format: IDFormat) -> pa.Check:
    """Build a pandera Check that validates ID column format.

    Parameters
    ----------
    id_format : IDFormat
        The expected ID format level.

    Returns
    -------
    pa.Check
        Pandera check for ID column validation.
    """
    reg = rf"{FORMAT_NAME_REGEXP}{pipeline_regexp}{index_regexp}"
    if id_format == IDFormat.EXPANDIBLE:
        reg = rf"{FORMAT_NAME_REGEXP}({pipeline_regexp})?({index_regexp})?"
    elif id_format == IDFormat.EXPANDIBLE_NO_INDEX:
        reg = rf"{FORMAT_NAME_REGEXP}({pipeline_regexp})?"

    return pa.Check(lambda x: x.str.match(f"^{reg}$"))


def add_format_name(df: pd.DataFrame) -> pd.DataFrame:
    """Extract format name from ID (removes pipeline and index suffix)."""

    df = df.assign(
        format_name=lambda x: x["ID"].str.replace(
            rf"({pipeline_regexp})?({index_regexp})?$", "", regex=True
        )
    )

    return df.rename(columns={"format_name": "Format name"})


def add_pipeline_name(
    df: pd.DataFrame,
    default: Optional[str] = None,
) -> pd.DataFrame:
    """
    Extract pipeline name from ID.

    If missing, fill with `default`.
    """

    df = df.assign(
        pipeline_name=lambda x: x["ID"].str.extract(rf"\(({pipeline_name_regexp})\)")[0]
    )

    if default is not None:
        df["pipeline_name"] = df["pipeline_name"].fillna(default)

    return df.rename(columns={"pipeline_name": "Pipeline name"})


def add_pipe_index(
    df: pd.DataFrame, relation_to_principal: FKRelation = FKRelation.ONE_TO_ONE
) -> pd.DataFrame:
    """
    Add 'Pipe index' column.

    Parameters
    ----------
    mode:
        PipeIndexMode.INFER
        PipeIndexMode.EXPLICIT

    missing_index_policy (only for EXPLICIT mode):
        MissingIndexPolicy.ZERO
        MissingIndexPolicy.INFER
    """
    mode = (
        PipeIndexMode.INFER
        if relation_to_principal == FKRelation.ONE_TO_ONE
        else PipeIndexMode.EXPLICIT
    )
    missing_index_policy = (
        MissingIndexPolicy.ZERO
        if relation_to_principal == FKRelation.ONE_TO_MANY
        else MissingIndexPolicy.INFER
    )
    df = df.copy()

    if mode is PipeIndexMode.EXPLICIT:
        extracted = df["ID"].str.extract(rf"{index_regexp}$")[0]
        df["Pipe index"] = extracted.astype("Int32")

        if missing_index_policy is MissingIndexPolicy.ZERO:
            df["Pipe index"] = df["Pipe index"].fillna(0)

        elif missing_index_policy is MissingIndexPolicy.INFER:
            mask_missing = df["Pipe index"].isna()

            df.loc[mask_missing, "Pipe index"] = (
                df[mask_missing].groupby(["Format name", "Pipeline name"]).cumcount()
            )

        df["Pipe index"] = df["Pipe index"].astype("Int32")

    elif mode is PipeIndexMode.INFER:
        df["Pipe index"] = (
            df.groupby(["Format name", "Pipeline name"]).cumcount().astype("Int32")
        )

    else:
        raise ValueError(f"Unsupported PipeIndexMode: {mode}")

    return df


def create_index_format_name_pipe(
    df: pd.DataFrame,
    pipeline_default: Optional[str] = None,
    relation_to_principal: FKRelation = FKRelation.ONE_TO_ONE,
) -> pd.DataFrame:
    """
    Full pipeline:
        1. Add Format name
        2. Add Pipeline name
        3. Add Pipe index
        4. Set MultiIndex
    """

    df = add_format_name(df)
    df = add_pipeline_name(df, default=pipeline_default)
    df = add_pipe_index(df, relation_to_principal=relation_to_principal)
    df["Computed ID"] = (
        df["Format name"]
        + "("
        + df["Pipeline name"]
        + ")/"
        + df["Pipe index"].astype(str)
    )
    return df.set_index(["Format name", "Pipeline name", "Pipe index", "Computed ID"])


def column_id_format_pipe(
    relation_to_principal: FKRelation = FKRelation.ONE_TO_ONE,
) -> pa.Column:
    """Build a pandera Column schema for ID with format-pipe validation.

    Parameters
    ----------
    relation_to_principal : FKRelation
        Relationship type determining ID format strictness.

    Returns
    -------
    pa.Column
        Pandera column schema for ID validation.
    """
    return pa.Column(
        pd.StringDtype,
        checks=[
            check_id_column(
                IDFormat.EXPANDIBLE_NO_INDEX
                if relation_to_principal == FKRelation.ONE_TO_ONE
                else IDFormat.EXPANDIBLE
            )
        ],
        nullable=True,
    )


def index_format_pipe(id_principal_table: Optional[Set[str]] = None) -> pa.MultiIndex:
    """Build a pandera MultiIndex schema for the format-pipeline index.

    Parameters
    ----------
    id_principal_table : Optional[Set[str]]
        Optional set of valid principal table IDs to validate against.

    Returns
    -------
    pa.MultiIndex
        Pandera MultiIndex schema for format-pipeline index validation.
    """
    # Pandera schema for validating format-pipeline index structure
    checks_id_idx = [check_id_column(IDFormat.COMPLETE)]
    if id_principal_table is not None:
        checks_id_idx.append(pa.Check(lambda x: x.isin(id_principal_table)))
    index_format_pipe_multindex: pa.MultiIndex = pa.MultiIndex(
        [
            pa.Index(
                pd.StringDtype,
                # [pa.Check(lambda x: x.isin(VALID_FORMATS))],
                name="Format name",
            ),
            pa.Index(
                pd.StringDtype,
                [pa.Check(lambda x: x.str.match(f"^{pipeline_name_regexp}$"))],
                name="Pipeline name",
                nullable=False,
            ),
            pa.Index(
                pd.UInt16Dtype,
                name="Pipe index",
            ),
            pa.Index(
                pd.StringDtype,
                checks_id_idx,
                name="Computed ID",
            ),
        ]
    )
    return index_format_pipe_multindex
