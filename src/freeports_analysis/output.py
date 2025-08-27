import datetime
from abc import ABC
from typing import Annotated, Optional
from pydantic import BaseModel, PositiveInt, PositiveFloat, AfterValidator
from freeports_analysis.data import COMPANIES
from .consts import Currency


Company = Annotated[str, AfterValidator(lambda x: x in COMPANIES)]


class FinancialData(BaseModel, ABC):
    ID: PositiveInt
    page: PositiveInt
    company: Company
    company_match: str
    subfund: str
    nominal_quantity: PositiveFloat
    market_value: PositiveFloat
    currency: Currency
    perc_net_assets: Optional[PositiveFloat]
    acquisition_cost: Optional[PositiveFloat]
    acquisition_currency: Optional[Currency]


class Equity(FinancialData):
    pass


class BondAdditionalInfos(BaseModel):
    ID: PositiveInt
    maturity: Optional[datetime.date]
    interest_rate: Optional[PositiveFloat]


class Bond(FinancialData):
    additional_infos = BondAdditionalInfos
