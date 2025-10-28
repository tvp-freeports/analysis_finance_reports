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

# Schema for validating investments DataFrame
investments_schema = pa.DataFrameSchema(
    {
        "Report page": pa.Column(pd.Int16Dtype, checks=pa.Check.greater_than(0)),
        "Company": pa.Column(pd.StringDtype, checks=pa.Check.isin(COMPANIES)),
        "Matched company": pa.Column(pd.StringDtype),
        "Financial instrument": pa.Column(
            pd.StringDtype, checks=pa.Check.isin(list_of_instruments)
        ),
        "Subfund": pa.Column(pd.StringDtype),
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
        "Format": pa.Column(
            pd.StringDtype, checks=pa.Check.isin(VALID_FORMATS), required=False
        ),
        "Document": pa.Column(pd.StringDtype, required=False),
    },
    strict=True,
    coerce=True,
    index=pa.Index(
        pd.Int32Dtype, checks=pa.Check.greater_than(0), unique=True, name="ID"
    ),
    checks=pa.Check(
        lambda df: (
            ("Format" in df and "Document" in df)
            or ("Format" not in df and "Document" not in df)
        )
    ),
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
