"""Management of the part of the format repository responsible of
page classification to pipeline mapping and of the order with
which pages are processed.
"""

from pathlib import Path
from copy import deepcopy
from typing import Optional, List, Set, Dict, Any
import pandera.pandas as pa
import pandas as pd
from freeports.i18n import _
from freeports._internals.formats.repo.algorithms.pipelines_definition import (
    create_index_format_name_pipe,
    column_id_format_pipe,
    index_format_pipe,
    column_id_format_pipe,
    FKRelation,
    pipeline_name_regexp,
    add_format_name,
    add_pipeline_name,
)
from freeports._internals.formats.repo.algorithms.pipelines_acquisition import (
    get_pipelines,
)


CONTENT_DIR = Path("content")
ALGORITHMS_DIR = CONTENT_DIR / "algorithms"
ORCHESTRATION_DIR = CONTENT_DIR / "orchestration"
TEMPLATES_DIR = CONTENT_DIR / "templates"


# Schema for validating the list of formats
_algorithms_schedule_schema = pa.DataFrameSchema(
    columns={
        "Page type": pa.Column(pd.StringDtype),
        "Filter next iteration": pa.Column(pd.BooleanDtype),
    },
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        name="Format name",
    ),
)


def get_algorithms_schedule_schema(format_names: List[str]) -> pa.DataFrameSchema:
    """Build a pandera schema for the algorithms schedule CSV.

    Parameters
    ----------
    format_names : List[str]
        List of valid format names.

    Returns
    -------
    pa.DataFrameSchema
        Pandera schema for algorithm schedule validation.
    """
    schema = deepcopy(_algorithms_schedule_schema)
    schema.index.checks.append(pa.Check.isin(format_names))
    return schema


def get_algorithms_schedule(
    formats_repo_dir: Path, format_names: List[str]
) -> pd.DataFrame:
    """Load and validate the algorithms schedule CSV.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.
    format_names : List[str]
        List of valid format names.

    Returns
    -------
    pd.DataFrame
        Validated algorithms schedule DataFrame.
    """
    pd.set_option("future.no_silent_downcasting", True)
    df = pd.read_csv(formats_repo_dir / ORCHESTRATION_DIR / "algorithms_schedule.csv")
    df = df.set_index(["Format name"])
    df["Filter next iteration"] = df["Filter next iteration"].fillna(False)
    return get_algorithms_schedule_schema(format_names).validate(df)


def get_schedule(
    formats_repo_dir: Path, format_name: str, format_names: List[str]
) -> List[Set[str]]:
    """Build the processing schedule for a format.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.
    format_name : str
        Name of the format to build schedule for.
    format_names : List[str]
        List of valid format names.

    Returns
    -------
    List[Set[str]]
        Ordered list of page type groups defining processing steps.
    """
    df = get_algorithms_schedule(formats_repo_dir, format_names)
    try:
        df_select = df.loc[[format_name]]
        schedule = [set()]
        for i, r in df_select.iterrows():
            schedule[-1].add(r["Page type"])
            if r["Filter next iteration"]:
                schedule.append(set())

        return schedule
    except KeyError:
        mapping = get_mapping(formats_repo_dir, format_name, format_names)
        return [set([pt for pt in mapping])]


# Schema for validating the list of formats
_pageclassify_overwrite_schema = pa.DataFrameSchema(
    columns={
        "ID": column_id_format_pipe(FKRelation.ONE_TO_ONE),
        "Pipeline name": pa.Column(
            pd.StringDtype,
            [pa.Check(lambda x: x.str.match(f"^{pipeline_name_regexp}$"))],
            nullable=False,
        ),
    },
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        name="Format name",
    ),
)


def get_pageclassify_overwrite_schema(format_names: List[str]) -> pa.DataFrameSchema:
    """Build a pandera schema for page classification overwrite CSV.

    Parameters
    ----------
    format_names : List[str]
        List of valid format names.

    Returns
    -------
    pa.DataFrameSchema
        Pandera schema for page classification overwrite validation.
    """
    schema = deepcopy(_pageclassify_overwrite_schema)
    schema.index.checks.append(pa.Check.isin(format_names))
    return schema


def get_pageclassify_overwrite(
    formats_repo_dir: Path, format_names: List[str]
) -> pd.DataFrame:
    """Load and validate the page classification overwrite CSV.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.
    format_names : List[str]
        List of valid format names.

    Returns
    -------
    pd.DataFrame
        Validated page classification overwrite DataFrame.
    """
    df = pd.read_csv(
        formats_repo_dir / ORCHESTRATION_DIR / "pageclassify_overwrite.csv"
    )
    df = add_format_name(df)
    df = add_pipeline_name(df)
    df = df.set_index(["Format name"])
    return get_pageclassify_overwrite_schema(format_names).validate(df)


def get_pageclassify_pipelines(
    formats_repo_dir: Path, format_name: str, format_names: List[str]
) -> Set[str]:
    """Get the set of pipeline names used for page classification.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.
    format_name : str
        Name of the format.
    format_names : List[str]
        List of valid format names.

    Returns
    -------
    Set[str]
        Set of pipeline names for page classification.
    """
    df = get_pageclassify_overwrite(formats_repo_dir, format_names)
    df_agg = df.groupby(by="Format name").agg({"Pipeline name": set})
    try:
        return df_agg.loc[format_name]["Pipeline name"]
    except KeyError:
        return set([""])


_mapping_schema = pa.DataFrameSchema(
    columns={
        "ID": column_id_format_pipe(FKRelation.ONE_TO_ONE),
        "Pipeline name": pa.Column(pd.StringDtype),
    },
    coerce=True,
    strict=True,
    index=pa.MultiIndex(
        [
            pa.Index(
                pd.StringDtype,
                name="Format name",
            ),
            pa.Index(pd.StringDtype, name="Page type"),
        ]
    ),
)


def get_mapping_schema(format_names: List[str]) -> pa.DataFrameSchema:
    """Build a pandera schema for the page-type-to-pipeline mapping CSV.

    Parameters
    ----------
    format_names : List[str]
        List of valid format names.

    Returns
    -------
    pa.DataFrameSchema
        Pandera schema for mapping validation.
    """
    schema = deepcopy(_mapping_schema)
    schema.index.indexes[0].checks.append(pa.Check.isin(format_names))
    return schema


def get_mapping_table(formats_repo_dir: Path, format_names: List[str]) -> pd.DataFrame:
    """Load and validate the page-type-to-pipeline mapping CSV.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.
    format_names : List[str]
        List of valid format names.

    Returns
    -------
    pd.DataFrame
        Validated mapping DataFrame.
    """
    df = pd.read_csv(formats_repo_dir / ORCHESTRATION_DIR / "mapping.csv")
    df = add_format_name(df)
    df = add_pipeline_name(df)
    df["Pipeline name"] = df["Pipeline name"].fillna("")
    df = df.set_index(["Format name", "Page type"])
    return get_mapping_schema(format_names).validate(df)


def get_mapping(
    formats_repo_dir: Path, format_name: str, format_names: List[str]
) -> Dict[str, Set[str]]:
    """Get the mapping from page types to pipeline names for a format.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.
    format_name : str
        Name of the format.
    format_names : List[str]
        List of valid format names.

    Returns
    -------
    Dict[str, Set[str]]
        Mapping from page type to set of pipeline names.
    """
    df = get_mapping_table(formats_repo_dir, format_names)
    df = df.drop(columns="ID")
    df = df.groupby(["Format name", "Page type"]).agg({"Pipeline name": set})
    res_df = None
    try:
        res_df = df.loc[format_name]
        mapping = {}
        for page_type, pipeline_names in res_df.iterrows():
            mapping[page_type] = pipeline_names["Pipeline name"]
        return mapping
    except KeyError:
        pp = get_pipelines(formats_repo_dir, format_name)
        pcpp = get_pageclassify_pipelines(formats_repo_dir, format_name, format_names)
        return {pn: set([pn]) for pn in pp if pn not in pcpp}
