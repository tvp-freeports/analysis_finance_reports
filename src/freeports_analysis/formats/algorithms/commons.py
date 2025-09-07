from typing import Optional, List
from enum import Enum
from lxml import etree
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.formats.data import format_name_regexp, VALID_FORMATS
from freeports_analysis.i18n import _

pipe_name_regexp = "[0-9a-z_]*"
pipe_regexp = rf"\({pipe_name_regexp}\)"
format_algorithm_id_regexp = f"{format_name_regexp}({pipe_regexp})?"


index_format_pipe = pa.MultiIndex(
    [
        pa.Index(
            pd.StringDtype,
            [pa.Check(lambda x: x.isin(VALID_FORMATS))],
            name="Format name",
        ),
        pa.Index(
            pd.StringDtype,
            [pa.Check(lambda x: x.str.match(f"^{pipe_name_regexp}$"))],
            name="Pipe name",
            nullable=True,
        ),
        pa.Index(
            pd.StringDtype,
            [pa.Check(lambda x: x.str.match(f"^{format_algorithm_id_regexp}$"))],
            name="ID",
        ),
    ]
)


def add_format_name_index(df):
    df = df.assign(
        format_name=lambda x: x["ID"].str.replace(f"{pipe_regexp}$", "", regex=True)
    )
    df.rename(columns={"format_name": "Format name"}, inplace=True)
    return df


def add_pipe_name(df):
    df = df.assign(
        pipe_name=lambda x: x["ID"].str.extract(f"\(({pipe_name_regexp})\)$")
    )
    df.rename(columns={"pipe_name": "Pipe name"}, inplace=True)
    return df


def set_index_format_name_pipe(df):
    return df.set_index(["Format name", "Pipe name", "ID"])


def create_index_format_name_pipe(df):
    df = add_format_name_index(df)
    df = add_pipe_name(df)
    return set_index_format_name_pipe(df)
