"""Data management for PDF format definitions and URL mappings.

This module handles the loading and validation of format definitions and
URL-to-format mappings used in document processing.
"""

from pathlib import Path
from typing import Optional, List
import pandera.pandas as pa
import pandas as pd
from freeports.i18n import _
from freeports._internals.formats.repo.metadata import get_formats
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
ALGORITHMS_DIR = CONTENT_DIR / "alghoritms"
ORCHESTRATION_DIR = CONTENT_DIR / "orchestration"
TEMPLATES_DIR = CONTENT_DIR / "templates"


# Schema for validating the list of formats
_alghoritms_schedule_schema = pa.DataFrameSchema(
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


def get_alghoritms_schedule_schema(formats_repo_validation_data):
    _alghoritms_schedule_schema.index.checks.append(
        pa.Check.isin(formats_repo_validation_data.formats)
    )
    return _alghoritms_schedule_schema


def get_alghoritms_schedule(
    formats_repo_dir, formats_repo_validation_data
) -> pd.DataFrame:
    pd.set_option("future.no_silent_downcasting", True)
    df = pd.read_csv(formats_repo_dir / ORCHESTRATION_DIR / "alghoritms_schedule.csv")
    df = df.set_index(["Format name"])
    df["Filter next iteration"] = df["Filter next iteration"].fillna(False)
    return get_alghoritms_schedule_schema(formats_repo_validation_data).validate(df)


def get_schedule(formats_repo_dir, format_name: str, formats_repo_validation_data):
    get_formats(formats_repo_dir, formats_repo_validation_data)
    df = get_alghoritms_schedule(formats_repo_dir, formats_repo_validation_data)
    try:
        df_select = df.loc[[format_name]]
        schedule = [set()]
        for i, r in df_select.iterrows():
            schedule[-1].add(r["Page type"])
            if r["Filter next iteration"]:
                schedule.append(set())

        return schedule
    except KeyError:
        mapping = get_mapping(
            formats_repo_dir, format_name, formats_repo_validation_data
        )
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


def get_pageclassify_overwrite_schema(format_repo_validation_data):
    _pageclassify_overwrite_schema.index.checks.append(
        pa.Check.isin(format_repo_validation_data.formats)
    )
    return _pageclassify_overwrite_schema


def get_pageclassify_overwrite(
    formats_repo_dir, format_repo_validation_data
) -> pd.DataFrame:
    df = pd.read_csv(
        formats_repo_dir / ORCHESTRATION_DIR / "pageclassify_overwrite.csv"
    )
    df = add_format_name(df)
    df = add_pipeline_name(df)
    df = df.set_index(["Format name"])
    return get_pageclassify_overwrite_schema(format_repo_validation_data).validate(df)


def get_pageclassify_pipelines(
    formats_repo_dir, format_name: str, format_repo_validation_data
):
    df = get_pageclassify_overwrite(formats_repo_dir, format_repo_validation_data)
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


def get_mapping_schema(format_repo_validation_data):
    _mapping_schema.index.indexes[0].checks.append(
        pa.Check.isin(format_repo_validation_data.formats)
    )
    return _mapping_schema


def get_mapping_table(formats_repo_dir, formats_repo_validation_data):
    df = pd.read_csv(formats_repo_dir / ORCHESTRATION_DIR / "mapping.csv")
    df = add_format_name(df)
    df = add_pipeline_name(df)
    df["Pipeline name"] = df["Pipeline name"].fillna("")
    df = df.set_index(["Format name", "Page type"])
    return get_mapping_schema(formats_repo_validation_data).validate(df)


def get_mapping(formats_repo_dir, format_name, formats_repo_validation_data):
    df = get_mapping_table(formats_repo_dir, formats_repo_validation_data)
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
        pcpp = get_pageclassify_pipelines(
            formats_repo_dir, format_name, formats_repo_validation_data
        )
        return {pn: set([pn]) for pn in pp if pn not in pcpp}
