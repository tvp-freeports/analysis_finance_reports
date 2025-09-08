from typing import Optional
import datetime
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.data import COMPANIES
from freeports_analysis.formats.data import VALID_FORMATS
from freeports_analysis.consts import Currency
from pydantic import BaseModel, confloat


list_of_instruments = ["EQUITY", "BOND"]

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
            pd.Float32Dtype, checks=pa.Check.greater_than(0), nullable=True
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
    maturity: Optional[datetime.date]
    interest_rate: Optional[confloat(gt=0.0, lt=1.0)]
