from abc import ABC
import datetime
from enum import Enum, auto
import tarfile
import gzip
import shutil
import os
from pathlib import Path
from typing import Optional, Annotated, Union
import yaml
from pydantic import (
    BaseModel,
    BeforeValidator,
    PositiveFloat,
    confloat,
    AfterValidator,
    ConfigDict,
)
from pydantic.types import Strict
import pandas as pd
from freeports_analysis.conf_parse import (
    OutStructureNormalMode,
    OutStructureBatchMode,
    OutFlagsBatchMode,
    OutFlagsNormalMode,
)
from freeports_analysis.data import COMPANIES
from freeports_analysis.consts import Promise, Currency, PromisesResolutionMap
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
    model_config = ConfigDict(validate_assignment=True)
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

    def fulfill_promises(self, mapping: PromisesResolutionMap) -> None:
        """Resolve all promise objects in this financial data instance.

        Processes each attribute that may contain a Promise object, resolving it
        using the provided mapping and performing validation where required.

        Parameters
        ----------
        mapping : PromisesResolutionMap
            Dictionary containing values to resolve promises from.

        Notes
        -----
        For attributes that require validation (perc_net_assets, company),
        the resolved values will be validated before assignment.
        """
        for k, v in self.model_dump().items():
            if isinstance(v, Promise):
                self[k] = v.fulfill_with(mapping)


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


def transform_to_files_schema(result_documents, batch_mode):
    add_infos = {}
    investments = []
    _id = 1
    for result_pages, format_name, prefix_out in result_documents:
        for page, result_page in enumerate(result_pages, start=1):
            for res in result_page:
                d = res.model_dump(mode="json")
                if batch_mode:
                    d["Format"] = format_name
                    d["Document"] = prefix_out
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

    return {"investments": df_investments, "additional_infos": add_infos}


def _write_structured(
    structured_data,
    unstructured_data,
    data_name,
    out_dir,
):
    out_dir.mkdir(exist_ok=True)
    out_path = out_dir / data_name
    out_path.mkdir(exist_ok=True)
    structured_data.to_csv(out_path / "table.csv")
    yaml.dump(
        unstructured_data,
        (out_path / "dicts.yaml").open("w"),
    )


def _write_regular(data, structured_mapping, unstructured_mapping, out_dir):
    out_dir.mkdir(exist_ok=True)
    for data_name, file_name in structured_mapping.items():
        data[data_name].to_csv(out_dir / file_name)
    for data_name, file_name in unstructured_mapping.items():
        yaml.dump(data[data_name], (out_dir / file_name).open("w"))


def _write_single_file(data, file_path):
    instruments = data["investments"].copy()
    bond_ids = instruments[instruments["Financial instrument"] == "BOND"].index
    info_dict = data["additional_infos"]
    info_dict_bond = {k: v for k, v in info_dict.items() if k in bond_ids}
    info_df = pd.DataFrame.from_dict(info_dict_bond, orient="index")
    info_df.index.name = "ID"
    instruments = instruments.merge(info_df, on="ID", how="left")
    instruments.rename(
        columns={"interest_rate": "Interest rate", "maturity": "Maturity"}, inplace=True
    )
    instruments.to_csv(file_path)


def write_files(data, out_path, profile, flags):
    out_path = Path(out_path)
    profiles_cls = OutStructureNormalMode
    flags_cls = OutFlagsNormalMode
    remove_uncompressed_out = not out_path.exists()
    if isinstance(profile, OutStructureBatchMode):
        profiles_cls = OutStructureBatchMode
        flags_cls = OutFlagsBatchMode

    if profile == profiles_cls.REGULAR:
        _write_regular(
            data,
            {"investments": "investments.csv"},
            {"additional_infos": "investments_add_infos.yaml"},
            out_path,
        )

    elif profile == profiles_cls.SINGLE_FILE:
        _write_single_file(data, out_path)
    elif profile == profiles_cls.STRUCTURED:
        _write_structured(
            data["investments"], data["additional_infos"], "investments", out_path
        )
    else:
        raise ValueError(_("Profile {} not known").format(profile))

    if flags_cls.COMPRESSED in flags:
        if profile == profiles_cls.SINGLE_FILE:
            archive_name = f"{out_path.name}.gz"
            with gzip.open(archive_name, "wb") as f_out:
                shutil.copyfileobj(out_path.open("rb"), f_out)
            if remove_uncompressed_out:
                os.remove(out_path)
        else:
            archive_name = f"{out_path.name}.tar.gz"
            with tarfile.open(archive_name, "w:gz") as tar:
                tar.add(out_path, arcname=out_path.name)
            if remove_uncompressed_out:
                shutil.rmtree(out_path)
