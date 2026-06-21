"""Data schema definitions for financial investment data validation.

This module defines the data schemas used to validate and structure financial
investment data extracted from PDF documents. It includes DataFrame schemas
for tabular data and Pydantic models for structured data validation.
"""

from typing import Optional, List
import datetime
import pandera.pandas as pa
import pandas as pd
from pydantic import BaseModel, confloat
from freeports_analysis.data import COMPANIES
from freeports_analysis.formats.data import VALID_FORMATS
from freeports_analysis.consts import Currency


# List of valid financial instrument types
list_of_instruments: List[str] = ["EQUITY", "BOND"]
list_of_sfdr_articles: List[str] = ["Art. 6", "Art. 8", "Art. 9"]

list_of_change_name_events: List[str] = ["MERGING", "RENAMING"]

# Schema for validating investments DataFrame
common_columns = {
    "ID": pa.Column(pd.Int32Dtype, checks=pa.Check.greater_than(0), unique=True),
    "Format": pa.Column(
        pd.StringDtype, checks=pa.Check.isin(VALID_FORMATS), required=False
    ),
    "Document": pa.Column(pd.StringDtype, required=False),
    "Report page": pa.Column(pd.Int16Dtype, checks=pa.Check.greater_than(0)),
}

common_checks = [
    pa.Check(
        lambda df: (
            ("Format" in df and "Document" in df)
            or ("Format" not in df and "Document" not in df)
        )
    )
]

common_schema_settings = {
    "ordered": True,
    "strict": True,
    "coerce": True,
    "checks": common_checks,
}

investments_schema = pa.DataFrameSchema(
    {
        **common_columns,
        "Triggering text": pa.Column(pd.StringDtype),
        "Investee": pa.Column(pd.StringDtype, checks=pa.Check.isin(COMPANIES)),
        "Financial instrument": pa.Column(
            pd.StringDtype, checks=pa.Check.isin(list_of_instruments)
        ),
        "Nominal/Quantity": pa.Column(
            pd.Float32Dtype, checks=pa.Check.greater_than(0), nullable=True
        ),
        "Market value": pa.Column(pd.Float32Dtype, checks=pa.Check.greater_than(0)),
        "Currency": pa.Column(
            pd.StringDtype, checks=pa.Check.isin([e.value for e in Currency])
        ),
        "% net assets": pa.Column(
            pd.Float32Dtype, checks=pa.Check.in_range(0.0, 1.0), nullable=True
        ),
        "Fund ID": pa.Column(pd.Int32Dtype, checks=pa.Check.greater_than(0)),
        "Acquisition cost": pa.Column(
            pd.Float32Dtype,
            checks=pa.Check.greater_than_or_equal_to(0.0),
            nullable=True,
        ),
        "Acquisition currency": pa.Column(
            pd.StringDtype,
            checks=pa.Check.isin([e.value for e in Currency]),
            nullable=True,
        ),
    },
    **common_schema_settings,
)


funds_assets_schema = pa.DataFrameSchema(
    {
        **common_columns,
        "Fund ID": pa.Column(pd.Int32Dtype, checks=pa.Check.greater_than(0)),
        "Date": pa.Column(pa.Timestamp, nullable=True),
        "Total assets": pa.Column(pd.Float32Dtype, checks=pa.Check.greater_than(0)),
        "Total liabilities": pa.Column(
            pd.Float32Dtype, checks=pa.Check.greater_than(0)
        ),
        "Total net assets": pa.Column(pd.Float32Dtype, checks=pa.Check.greater_than(0)),
        "Currency": pa.Column(
            pd.StringDtype, checks=pa.Check.isin([e.value for e in Currency])
        ),
    },
    unique=["Fund ID", "Date"],
    **common_schema_settings,
)

funds_change_name_schema = pa.DataFrameSchema(
    {
        **common_columns,
        "Fund ID": pa.Column(pd.Int32Dtype, checks=pa.Check.greater_than(0)),
        "From": pa.Column(datetime.date),
        "Type of event": pa.Column(
            pd.StringDtype, checks=pa.Check.isin(list_of_change_name_events)
        ),
        "Old name": pa.Column(pd.StringDtype),
    },
    unique=["Fund ID", "From", "Type of event", "Old name"],
    **common_schema_settings,
)

funds_schema = pa.DataFrameSchema(
    {
        **common_columns,
        "Name": pa.Column(pd.StringDtype, unique=True),
        "Managment company ID": pa.Column(
            pd.Int32Dtype, checks=pa.Check.greater_than(0), nullable=True
        ),
    },
    **common_schema_settings,
)

funds_sfdr_classification_schema = pa.DataFrameSchema(
    {
        "Fund ID": pa.Column(
            pd.Int32Dtype, checks=pa.Check.greater_than(0), unique=True
        ),
        "SFDR classification": pa.Column(
            pd.StringDtype, checks=pa.Check.isin(list_of_sfdr_articles)
        ),
        "Report page": pa.Column(pd.Int16Dtype, checks=pa.Check.greater_than(0)),
        "Format": pa.Column(
            pd.StringDtype, checks=pa.Check.isin(VALID_FORMATS), required=False
        ),
        "Document": pa.Column(pd.StringDtype, required=False),
    },
    **common_schema_settings,
)

funds_esg_indicators_schema = pa.DataFrameSchema(
    {
        "Fund ID": pa.Column(pd.Int32Dtype, checks=pa.Check.greater_than(0)),
        "Indicator": pa.Column(pd.StringDtype),
        "Value": pa.Column(pd.StringDtype),
        "Report page": pa.Column(pd.Int16Dtype, checks=pa.Check.greater_than(0)),
        "Format": pa.Column(
            pd.StringDtype, checks=pa.Check.isin(VALID_FORMATS), required=False
        ),
        "Document": pa.Column(pd.StringDtype, required=False),
    },
    **common_schema_settings,
)

assets_managers_schema = pa.DataFrameSchema(
    {**common_columns, "Name": pa.Column(pd.StringDtype, unique=True)},
    **common_schema_settings,
)


investments_managers_schema = pa.DataFrameSchema(
    {
        "Investment manager ID": pa.Column(
            pd.Int32Dtype, checks=pa.Check.greater_than(0)
        ),
        "Fund ID": pa.Column(pd.Int32Dtype, checks=pa.Check.greater_than(0)),
    },
    strict=True,
    coerce=True,
    ordered=True,
    unique=["Investment manager ID", "Fund ID"],
)


class BondAdditionalInfos(BaseModel):
    """Additional information specific to bond investments.

    This model captures bond-specific attributes that are not part of the
    core investment data structure.

    Attributes
    ----------
    maturity : Optional[datetime.date]
        The date when the bond reaches maturity and principal is repaid
    interest_rate : Optional[confloat(ge=0.0, lt=1.0)]
        The annual interest rate as a decimal value between 0.0 and 1.0

    Notes
    -----
    This model is used to store bond-specific information separately from
    the main investment data structure, allowing for cleaner separation
    between common investment attributes and bond-specific ones.
    """

    maturity: Optional[datetime.date]
    interest_rate: Optional[confloat(ge=0.0, lt=1.0)]
