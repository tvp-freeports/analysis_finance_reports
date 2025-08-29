from abc import ABC
import datetime
from enum import Enum, auto
from pathlib import Path
from typing import Optional, Annotated, Union
import yaml
from pydantic import BaseModel, BeforeValidator, PositiveFloat, confloat, AfterValidator
from pydantic.types import Strict
import pandas as pd
from freeports_analysis.data import COMPANIES
from freeports_analysis.consts import Promise, Currency
from freeports_analysis.i18n import _
from .files_schema import investments_schema, BondAdditionalInfos


def validate_company(value: str) -> str:
    if value not in COMPANIES:
        raise ValueError(f"Color must be one of {COMPANIES}, got '{value}'")
    return value


PromiseStrict = Annotated[Promise, Strict()]


def try_convert_to_currency(value: str) -> Union[Currency, Promise]:
    """Prova a convertire in Currency, altrimenti lascia come Promise"""
    if isinstance(value, Promise):
        return value

    return Currency(value)


Company = Annotated[str, AfterValidator(validate_company)]
PromisedMarketValue = Union[PositiveFloat, PromiseStrict]
PromisedCurrency = Annotated[
    Union[Currency, PromiseStrict],
    BeforeValidator(try_convert_to_currency),
]
PromisedSubfund = Union[str, PromiseStrict]
PromisedPercNetAsstes = Union[confloat(gt=0.0, lt=1.0), PromiseStrict]
PromisedAcquisitionCost = Union[PositiveFloat, PromiseStrict]
PromisedAcquisitionCurrency = Annotated[
    Union[Currency, PromiseStrict],
    BeforeValidator(try_convert_to_currency),
]
PromisedInterestRate = Union[confloat(gt=0.0, lt=1.0), PromiseStrict]


class Investment(BaseModel, ABC):
    company: Company
    company_match: str
    subfund: PromisedSubfund
    nominal_quantity: Optional[PositiveFloat] = None
    market_value: PromisedMarketValue
    currency: Optional[PromisedCurrency] = None
    perc_net_assets: Optional[PromisedPercNetAsstes] = None
    acquisition_cost: Optional[PromisedAcquisitionCost] = None
    acquisition_currency: Optional[PromisedAcquisitionCurrency] = None

    def __str__(self) -> str:
        string = f"{self.__class__.__name__}:\n"
        translated_field = _("Subfund")
        string += f"\t{translated_field}:\t{self.subfund}\n"
        translated_field = _("Company")
        string += f"\t{translated_field}:\t{self.company_match}\t[{self.company}]\n"
        translated_field = _("Currency")
        curr_name = (
            self.currency if isinstance(self.currency, Promise) else self.currency.name
        )
        string += f"\t{translated_field}:\t{curr_name}\n"
        translated_field = _("Market value")
        symbol = "" if isinstance(self.currency, Promise) else self.currency.symbol
        string += f"\t{translated_field}:\t{self.market_value:.2f}{symbol}"
        if self.perc_net_assets is not None:
            translated_field = _("of net assets")
            string += f"\t({self.perc_net_assets:.3%} {translated_field})"
        string += "\n"
        if self.nominal_quantity is not None:
            translated_field = _("Quantity")
            string += f"\t{translated_field}:\t{self.nominal_quantity}\n"
        if self.acquisition_cost is not None:
            translated_field = _("Acquisition cost")
            string += f"\t{translated_field}:\t{self.acquisition_cost:.2f}"
        if self.acquisition_currency is not None:
            symbol = (
                ""
                if isinstance(self.acquisition_currency, Promise)
                else self.acquisition_currency.symbol
            )
            string += f"{symbol}\n"
            translated_field = _("Acquisition currency")
            curr_name = (
                self.acquisition_currency
                if isinstance(self.acquisition_currency, Promise)
                else self.acquisition_currency.name
            )
            string += f"\t{translated_field}:\t{curr_name}"
        string += "\n"
        return string


class Equity(Investment):
    pass


class Bond(Investment):
    maturity: Optional[datetime.date] = None
    interest_rate: Optional[PromisedInterestRate] = None

    def __str__(self):
        add_infos = False
        string = super().__str__()
        translated_field = _("Additional infos")
        string += f"\t{translated_field}: {{"
        if self.maturity is not None:
            add_infos = True
            translated_field = _("Maturity")
            string += f"\n\t\t{translated_field}:\t{self.maturity}"
        if self.interest_rate is not None:
            add_infos = True
            translated_field = _("Interest rate")
            interest_rate = (
                f"{self.interest_rate}"
                if isinstance(self.interest_rate, Promise)
                else f"{self.interest_rate:.3%}"
            )
            string += f"\n\t\t{translated_field}:\t{interest_rate}"
        string += "\n\t}\n" if add_infos else "\t}\n"
        return string


class OutputProfile(Enum):
    REGULAR = auto()
    STRUCTURED = auto()
    CONDENSED = auto()


def transform_to_files_schema(result_pages):
    add_infos = {}
    investments = []
    _id = 1
    for page, result_page in result_pages.items():
        for res in result_page:
            d = res.model_dump(mode="json")
            d["Financial instrument"] = res.__class__.__name__.upper()
            d["Report page"] = page
            d["ID"] = _id
            _id += 1
            if isinstance(res, Bond):
                infos = ["maturity", "interest_rate"]
                add_infos[d["ID"]] = BondAdditionalInfos(
                    **{k: v for k, v in d.items() if k in infos}
                ).model_dump(mode="json")
                d = {k: v for k, v in d.items() if k not in infos}
            investments.append(d)
    df_investments = pd.DataFrame.from_dict(investments)
    df_investments.set_index("ID", inplace=True)
    df_investments.rename(
        columns={
            "company": "Company",
            "company_match": "Matched company",
            "subfund": "Subfund",
            "nominal_quantity": "Nominal/Quantity",
            "market_value": "Market value",
            "currency": "Currency",
            "perc_net_assets": "% net assets",
            "acquisition_cost": "Acquisition cost",
            "acquisition_currency": "Acquisition currency",
        },
        inplace=True,
    )
    df_investments = investments_schema.validate(df_investments)
    # df_investments["Nominal/Quantity"]=df_investments["Nominal/Quantity"].round(3)
    # df_investments["Market value"]=df_investments["Market value"].round(2)
    # df_investments["Acquisition cost"]=df_investments["Acquisition cost"].round(2)
    # pd.set_option('display.float_format', '{:.3f}'.format)
    return {"investments": df_investments, "additional_infos": add_infos}


def write_files(out_path, data, profile=OutputProfile.REGULAR):
    out_path = Path(out_path)
    if profile == OutputProfile.REGULAR:
        out_path.mkdir(exist_ok=True)
        data["investments"].to_csv(out_path / "investments.csv")
        yaml.dump(
            data["additional_infos"],
            (out_path / "investments_add_infos.yaml").open("w"),
        )
    elif profile == OutputProfile.CONDENSED:
        pass
    elif profile == OutputProfile.STRUCTURED:
        pass
    else:
        raise ValueError(_("Profile {} not known").format(profile))
