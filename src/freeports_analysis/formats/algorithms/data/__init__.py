"""Data management for PDF format definitions and URL mappings.

This module handles the loading and validation of format definitions and
URL-to-format mappings used in document processing.
"""

from pathlib import Path
from typing import Optional, List
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.i18n import _

from freeports_analysis.formats.data import VALID_FORMATS

from ..commons import (
    create_index_format_name_pipe,
    column_id_format_pipe,
    index_format_pipe,
    column_id_format_pipe,
    FKRelation,
    pipeline_name_regexp,
    add_format_name,
    add_pipeline_name,
)

data = Path(__file__).parent

# Schema for validating the list of formats
alghoritms_schedule_schema = pa.DataFrameSchema(
    columns={
        "ID": column_id_format_pipe(FKRelation.ONE_TO_ONE),
        "Page type": pa.Column(pd.StringDtype),
        "Filter next iteration": pa.Column(pd.BooleanDtype),
    },
    coerce=True,
    strict=True,
    index=pa.MultiIndex(
        [
            pa.Index(
                pd.StringDtype,
                [pa.Check(lambda x: x.isin(VALID_FORMATS))],
                name="Format name",
            ),
            pa.Index(
                pd.StringDtype,
                [pa.Check(lambda x: x.str.match(f"^{pipeline_name_regexp}$"))],
                name="Pipeline name",
                nullable=False,
            ),
        ]
    ),
)


def get_alghoritms_schedule() -> pd.DataFrame:
    df = pd.read_csv(data / "alghoritms_schedule.csv")
    df = add_format_name(df)
    df = add_pipeline_name(df)
    df = df.set_index(["Format name", "Pipeline name"])
    pd.set_option("future.no_silent_downcasting", True)
    df["Filter next iteration"] = df["Filter next iteration"].fillna(False)
    return alghoritms_schedule_schema.validate(df)


def get_schedule_of(format_name: str):
    df = get_alghoritms_schedule()
    df = df.drop(columns=["ID"])
    df_select = df.loc[format_name]
    schedule = [set()]
    d = dict()
    for i, r in df_select.iterrows():
        schedule[-1].add(r["Page type"])
        if r["Filter next iteration"]:
            schedule.append(set())
        d[r["Page type"]] = i

    return schedule, d


# Schema for validating the list of formats
pageclassify_overwrite_schema = pa.DataFrameSchema(
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
        [pa.Check(lambda x: x.isin(VALID_FORMATS))],
        name="Format name",
    ),
)


def get_pageclassify_overwrite() -> pd.DataFrame:
    df = pd.read_csv(data / "pageclassify_overwrite.csv")
    df = add_format_name(df)
    df = add_pipeline_name(df)
    df = df.set_index(["Format name"])
    return pageclassify_overwrite_schema.validate(df)


def get_pageclassify_pipeline(format_name: str):
    df = get_pageclassify_overwrite()
    df_agg = df.groupby(by="Format name").agg({"Pipeline name": set})
    return df_agg.loc[format_name]["Pipeline name"]
