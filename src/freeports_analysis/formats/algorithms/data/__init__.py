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
from ..pipelines import get_pipelines
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
        "Page type": pa.Column(pd.StringDtype),
        "Filter next iteration": pa.Column(pd.BooleanDtype),
    },
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        [pa.Check(lambda x: x.isin(VALID_FORMATS))],
        name="Format name",
    ),
)


def get_alghoritms_schedule() -> pd.DataFrame:
    df = pd.read_csv(data / "alghoritms_schedule.csv")
    df = df.set_index(["Format name"])
    pd.set_option("future.no_silent_downcasting", True)
    df["Filter next iteration"] = df["Filter next iteration"].fillna(False)
    return alghoritms_schedule_schema.validate(df)


def get_schedule(format_name: str):
    df = get_alghoritms_schedule()
    try:
        df_select = df.loc[[format_name]]
        schedule = [set()]
        for i, r in df_select.iterrows():
            schedule[-1].add(r["Page type"])
            if r["Filter next iteration"]:
                schedule.append(set())

        return schedule
    except KeyError:
        mapping = get_mapping(format_name)
        return [set([pt for pt in mapping])]


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


def get_pageclassify_pipelines(format_name: str):
    df = get_pageclassify_overwrite()
    df_agg = df.groupby(by="Format name").agg({"Pipeline name": set})
    try:
        return df_agg.loc[format_name]["Pipeline name"]
    except KeyError:
        return set([""])


mapping_schema = pa.DataFrameSchema(
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
                checks=[pa.Check(lambda x: x.isin(VALID_FORMATS))],
                name="Format name",
            ),
            pa.Index(pd.StringDtype, name="Page type"),
        ]
    ),
)


def get_mapping_table():
    df = pd.read_csv(data / "mapping.csv")
    df = add_format_name(df)
    df = add_pipeline_name(df)
    pd.set_option("future.no_silent_downcasting", True)
    df["Pipeline name"] = df["Pipeline name"].fillna("")
    df = df.set_index(["Format name", "Page type"])
    return mapping_schema.validate(df)


def get_mapping(format_name):
    df = get_mapping_table()
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
        pp = get_pipelines(format_name)
        pcpp = get_pageclassify_pipelines(format_name)
        return {pn: set([pn]) for pn in pp if pn not in pcpp}
