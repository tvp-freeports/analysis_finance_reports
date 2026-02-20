"""Data management for PDF format definitions and URL mappings.

This module handles the loading and validation of format definitions and
URL-to-format mappings used in document processing.
"""

from pathlib import Path
from typing import Optional, List
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.i18n import _

from .commons import create_index_format_name_pipe, index_format_pipe

data = Path(__file__).parent

FORMAT_NAME_REGEXP = r".+\-[A-Z]{2}\d{2}(@[A-Z]{2,3})?(\.[^\.]+)?"

# Schema for validating the list of formats
alghoritms_schedule_schema = pa.DataFrameSchema(
    columns={
        "Page type": pa.Column(pd.StringDtype),
        "Filter next iteration": pa.Column(pd.BooleanDtype, nullable=True),
    },
    coerce=True,
    strict=True,
    index=index_format_pipe,
)


def get_alghoritms_schedule() -> pd.DataFrame:
    df = pd.read_csv(data / "alghoritms_schedule.csv")
    df = create_index_format_name_pipe(df)
    return alghoritms_schedule_schema.validate(df)


def get_schedule_of(format_name: str):
    df = get_alghoritms_schedule()
    return df.loc[format_name]
