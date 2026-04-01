"""Output module for financial data processing and file generation.

This module handles the transformation, serialization, and output of financial
investment data extracted from PDF documents. It provides classes for representing
financial instruments and functions for writing data in various output formats.
"""

from __future__ import annotations

from abc import ABC
import datetime
from enum import Enum, auto
import tarfile
import gzip
import shutil
import os
from pathlib import Path
from typing import Optional, Annotated, Union, Dict, Any, List, Tuple, Set
import yaml
from pydantic import (
    BaseModel,
    BeforeValidator,
    PositiveFloat,
    confloat,
    AfterValidator,
    ConfigDict,
    model_validator,
)
from pydantic.types import Strict
import pandas as pd
from freeports_analysis.conf_parse import (
    OutStructureNormalMode,
    OutStructureBatchMode,
    OutFlagsBatchMode,
    OutFlagsNormalMode,
)
from freeports_analysis.formats.utils.text_filter import match
from freeports_analysis.data import COMPANIES
from freeports_analysis.consts import Promise, Currency, PromisesResolutionMap
from freeports_analysis.i18n import _
from freeports_analysis import match
from .files_schema import (
    investments_schema,
    assets_managers_schema,
    funds_assets_schema,
    funds_schema,
    investments_managers_schema,
    BondAdditionalInfos,
)


class PageResults:
    investments: List[Equity | Bond]
    assets_managers: List[ManagementCompany | InvestmentsManager]
    funds: List[Fund]
    funds_assets: List[FundAssets]

    def __init__(self):
        self.investments = []
        self.assets_managers = []
        self.funds = []
        self.funds_assets = []

    def fulfill_promises(self, promises_resolution_map):
        for i in self.investments:
            i.fulfill_promises(promises_resolution_map)
        for a in self.assets_managers:
            a.fulfill_promises(promises_resolution_map)
        for f in self.funds:
            f.fulfill_promises(promises_resolution_map)
        for fa in self.funds_assets:
            fa.fulfill_promises(promises_resolution_map)


class PageIndexable:
    data_per_page: List[Any]

    def __init__(self, data: List[Any]):
        self.data_per_page = data

    def __getitem__(self, page_n):
        return self.data_per_page[page_n - 1]


class DocumentResults:
    prefix_out: str
    algorithm: str
    results: List[PageResults]

    def __init__(self, prefix_out: str, algorithm: str):
        self.prefix_out = (prefix_out,)
        self.algorithm = algorithm
        self.results = []

    @property
    def investment():
        return PageIndexable(list(map(lambda x: x.investment), self.results))

    @property
    def assets_managers():
        return PageIndexable(list(map(lambda x: x.assets_managers), self.results))

    @property
    def funds():
        return PageIndexable(list(map(lambda x: x.funds), self.results))

    @property
    def funds_assets():
        return PageIndexable(list(map(lambda x: x.funds_assets), self.results))

    def __getitem__(self, page_n):
        return self.results[page_n - 1]

    def __iter__(self):
        return iter(self.results)

    def fulfill_promises(self, promises_resolution_map):
        for pr in self:
            pr.fulfill_promises(promises_resolution_map)


def validate_company(value: str) -> str:
    """Validate that a company name exists in the predefined companies list.

    Parameters
    ----------
    value : str
        The company name to validate

    Returns
    -------
    str
        The validated company name

    Raises
    ------
    ValueError
        If the company name is not found in the COMPANIES list

    Notes
    -----
    This function is used as a Pydantic validator to ensure that only
    companies from the predefined list are accepted in financial data models.
    """
    if value not in COMPANIES:
        raise ValueError(f"Company must be one of {COMPANIES}, got '{value}'")
    return value


def try_convert_to_currency(value: Union[str, Promise]) -> Union[Currency, Promise]:
    """Attempt to convert a string to Currency, preserving Promise objects.

    Parameters
    ----------
    value : Union[str, Promise]
        The value to convert - either a currency string or Promise object

    Returns
    -------
    Union[Currency, Promise]
        Currency enum if conversion successful, otherwise original Promise

    Raises
    ------
    KeyError
        If the currency string is not a valid Currency enum member

    Notes
    -----
    This function is used as a Pydantic validator to handle both concrete
    currency values and Promise objects that will be resolved later.
    """
    if isinstance(value, Promise):
        return value
    return Currency(value)


# Type aliases for financial data with promise support
Company = Annotated[str, AfterValidator(validate_company)]
PromisedMarketValue = Union[Promise, PositiveFloat]
PromisedCurrency = Annotated[
    Union[Promise, Currency],
    BeforeValidator(try_convert_to_currency),
]
PromisedSubfund = Union[Promise, str]
PromisedPercNetAsstes = Union[Promise, confloat(ge=0.0, lt=1.0)]
PromisedAcquisitionCost = Union[Promise, PositiveFloat]
PromisedAcquisitionCurrency = Annotated[
    Union[Promise, Currency],
    BeforeValidator(try_convert_to_currency),
]
PromisedInterestRate = Union[Promise, confloat(ge=0.0, lt=1.0)]


class PromisableDict:
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
        the resolved values will be validated before assignment. This method
        iterates through all model attributes and resolves any Promise objects
        found, updating the instance in place.
        """
        for k, v in self.model_dump().items():
            if isinstance(v, Promise):
                setattr(self, k, v.fulfill_with(mapping))


class Investment(BaseModel, ABC, PromisableDict):
    """Abstract base class representing a financial investment.

    This class serves as the foundation for different types of financial
    instruments, providing common attributes and validation logic.

    Attributes
    ----------
    company : Company
        Validated company name from predefined list
    company_match : str
        Original company name as matched in the source document
    nominal_quantity : Optional[PositiveFloat]
        Number of units/shares held
    market_value : PromisedMarketValue
        Current market value of the investment
    currency : PromisedCurrency
        Currency of the market value
    perc_net_assets : Optional[PromisedPercNetAsstes]
        Percentage of total net assets represented by this investment
    acquisition_cost : Optional[PromisedAcquisitionCost]
        Original acquisition cost
    acquisition_currency : Optional[PromisedAcquisitionCurrency]
        Currency of the acquisition cost

    Notes
    -----
    This class uses Pydantic for data validation and supports Promise objects
    for deferred value resolution. All currency values are validated against
    the Currency enum, and company names are validated against the predefined
    companies list.
    """

    model_config = ConfigDict(
        validate_assignment=True,
        arbitrary_types_allowed=True,
    )
    company: Company
    company_match: str
    fund: str
    manco: Optional[str] = None
    nominal_quantity: Optional[PositiveFloat] = None
    market_value: PromisedMarketValue
    currency: PromisedCurrency
    perc_net_assets: Optional[PromisedPercNetAsstes] = None
    acquisition_cost: Optional[PromisedAcquisitionCost] = None
    acquisition_currency: Optional[PromisedAcquisitionCurrency] = None

    def __str__(self) -> str:
        """Generate a formatted string representation of the investment.

        Returns
        -------
        str
            Formatted multi-line string with investment details
        """
        string = f"{self.__class__.__name__}:\n"
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

    def __hash__(self):
        return hash(frozenset(self.model_dump(mode="json").items()))


class Equity(Investment):
    """Represents an equity investment (stocks, shares)."""


class Bond(Investment):
    """Represents a bond investment with maturity and interest rate.

    Attributes
    ----------
    maturity : Optional[datetime.date]
        Bond maturity date when principal is repaid
    interest_rate : Optional[PromisedInterestRate]
        Annual interest rate as a decimal value (e.g., 0.05 for 5%)

    Notes
    -----
    Bond investments represent debt securities that pay periodic interest
    and return the principal at maturity. The interest rate is stored as
    a decimal value (e.g., 0.05 represents 5% annual interest).
    """

    maturity: Optional[datetime.date] = None
    interest_rate: Optional[PromisedInterestRate] = None

    def __str__(self) -> str:
        """Generate a formatted string representation of the bond investment.

        Returns
        -------
        str
            Formatted multi-line string with bond details including maturity and interest rate
        """
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


class AssetsManager(BaseModel, ABC, PromisableDict):
    model_config = ConfigDict(
        validate_assignment=True,
        arbitrary_types_allowed=True,
    )
    name: str
    managed_funds: Set[str]

    def __repr__(self):
        return f'{self.__class__.__name__}("{self.name}")'

    def __hash__(self):
        return hash((self.name, frozenset(self.managed_funds)))


class ManagementCompany(AssetsManager):
    """Rappresent the manager"""


class InvestmentsManager(AssetsManager):
    """Rappresent the InvestmentsManager"""


class Fund(BaseModel, match.MatchFund, PromisableDict):
    name: str

    def __init__(self, name):
        BaseModel.__init__(self, name=name)
        match.MatchFund.__init__(self, name)

    def __hash__(self):
        return match.MatchFund.__hash__(self)

    def __eq__(self, other):
        return match.MatchFund.__eq__(self, other)


class FundAssets(BaseModel, PromisableDict):
    model_config = ConfigDict(
        validate_assignment=True,
        arbitrary_types_allowed=True,
    )
    fund: str
    tot_assets: PositiveFloat
    liabilities: PositiveFloat
    net_assets: PositiveFloat
    currency: PromisedCurrency

    @model_validator(mode="after")
    def validate_assets_equation(self) -> "FundAssets":
        if (
            abs(self.liabilities + self.net_assets - self.tot_assets) > 1e-4
        ):  # Tolerance for float comparison
            raise ValueError(
                f"liabilities ({self.liabilities}) + net_assets ({self.net_assets}) "
                f"must equal tot_assets ({self.tot_assets})"
            )
        return self

    def __repr__(self):
        return f'{self.__class__.__name__}(fund="{self.fund}",tot_assets={self.tot_assets},liabilities={self.liabilities},net_assets={self.net_assets},currency={self.currency})'

    def __hash__(self):
        return hash(
            (
                self.tot_assets,
                self.liabilities,
                self.net_assets,
                self.currency,
                self.fund,
            )
        )


def transform_to_files_schema(
    results: List[DocumentResults],
    batch_mode: bool,
) -> Dict[str, Any]:
    """Transform investment results into structured data for file output.

    Parameters
    ----------
    result_documents : List[Tuple[List[List[Investment]], str, Optional[str]]]
        List of document results containing investment data, format info, and prefixes
    batch_mode : bool
        Whether processing is in batch mode (affects output structure)

    Returns
    -------
    Dict[str, Any]
        Dictionary containing:
        - 'investments': DataFrame with investment data
        - 'additional_infos': Dictionary with bond-specific information

    Notes
    -----
    This function processes investment data from multiple documents and pages,
    transforming it into a format suitable for file output. In batch mode,
    additional metadata (format and document identifier) is included.
    Bond-specific information (maturity, interest rate) is separated from
    the main investment data structure.
    """
    add_infos: Dict[int, Dict[str, Any]] = {}
    investments: List[Dict[str, Any]] = []
    assets_managers: List[Dict[str, Any]] = []
    funds: List[Dict[str, Any]] = []
    asset_managers_to_funds: List[Dict[str, Any]] = []
    funds_assets: List[Dict[str, Any]] = []

    _id_investments = 1
    _id_funds = 1
    _id_assets_managers = 1
    new_funds = {}
    asset_managers_names = set()

    for document_results in results:
        for page_n, page_results in enumerate(document_results, start=1):
            for f in page_results.funds:
                if f not in new_funds:
                    d = f.model_dump(mode="json")
                    d["ID"] = _id_funds
                    _id_funds += 1
                    new_funds[f] = d
                if "Report page" not in new_funds[f]:
                    if batch_mode:
                        new_funds[f]["Format"] = document_results.algorithm
                        new_funds[f]["Document"] = document_results.prefix_out
                    new_funds[f]["Report page"] = page_n
                if "Managment company ID" not in new_funds[f]:
                    new_funds[f]["Managment company ID"] = None
            for fa in page_results.funds_assets:
                d = fa.model_dump(mode="json")
                f = Fund(name=d["fund"])
                if f not in new_funds:
                    df = f.model_dump(mode="json")
                    df["ID"] = _id_funds
                    _id_funds += 1
                    new_funds[f] = df
                if batch_mode:
                    d["Format"] = document_results.algorithm
                    d["Document"] = document_results.prefix_out
                d["Report page"] = page_n
                d["Fund ID"] = new_funds[f]["ID"]
                funds_assets.append(d)
            for i in page_results.investments:
                d = i.model_dump(mode="json")
                f = Fund(name=d["fund"])
                if f not in new_funds:
                    df = f.model_dump(mode="json")
                    df["ID"] = _id_funds
                    _id_funds += 1
                    new_funds[f] = df
                if batch_mode:
                    d["Format"] = document_results.algorithm
                    d["Document"] = document_results.prefix_out
                d["Financial instrument"] = i.__class__.__name__.upper()
                d["Report page"] = page_n
                d["ID"] = _id_investments
                d["Fund ID"] = new_funds[f]["ID"]

                if isinstance(i, Bond):
                    infos = ["maturity", "interest_rate"]
                    add_infos[d["ID"]] = BondAdditionalInfos(
                        **{k: v for k, v in d.items() if k in infos}
                    ).model_dump(mode="json")
                    d = {k: v for k, v in d.items() if k not in infos}
                investments.append(d)
                _id_investments += 1
            for a in page_results.assets_managers:
                d = a.model_dump(mode="json")
                for s in d["managed_funds"]:
                    f = Fund(name=s)
                    if f not in new_funds:
                        df = f.model_dump(mode="json")
                        df["ID"] = _id_funds
                        _id_funds += 1
                        new_funds[f] = df
                    if isinstance(a, ManagementCompany):
                        new_funds[f]["Managment company ID"] = _id_assets_managers
                    if isinstance(a, InvestmentsManager):
                        asset_managers_to_funds.append(
                            {
                                "Investment manager ID": _id_assets_managers,
                                "Fund ID": new_funds[f]["ID"],
                            }
                        )
                if batch_mode:
                    d["Format"] = document_results.algorithm
                    d["Document"] = document_results.prefix_out
                d["Report page"] = page_n
                d["ID"] = _id_assets_managers
                if a.name not in asset_managers_names:
                    asset_managers_names.add(a.name)
                    assets_managers.append(d)
                    _id_assets_managers += 1
    funds = list(new_funds.values())
    df_investments = (
        pd.DataFrame.from_dict(investments)
        if len(investments) > 0
        else pd.DataFrame(
            columns=[
                "ID",
                "company",
                "fund",
                "company_match",
                "Fund ID",
                "manco",
                "Financial instrument",
                "nominal_quantity",
                "market_value",
                "perc_net_assets",
                "Currency",
                "acquisition_cost",
                "acquisition_currency",
                "Report page",
            ]
        )
    )
    df_investments_managers = (
        pd.DataFrame.from_dict(asset_managers_to_funds)
        if len(asset_managers_to_funds)
        else pd.DataFrame(columns=["Investment manager ID", "Fund ID"])
    )
    df_funds_assets = (
        pd.DataFrame.from_dict(funds_assets)
        if len(funds_assets) > 0
        else pd.DataFrame(
            columns=[
                "Fund ID",
                "fund",
                "tot_assets",
                "net_assets",
                "liabilities",
                "currency",
                "Report page",
            ]
        )
    )
    df_funds = (
        pd.DataFrame.from_dict(funds)
        if len(funds) > 0
        else pd.DataFrame(columns=["ID", "Managment company ID", "Name", "Report page"])
    )
    df_assets_managers = (
        pd.DataFrame.from_dict(assets_managers)
        if len(assets_managers) > 0
        else pd.DataFrame(columns=["ID", "managed_funds", "name", "Report page"])
    )

    df_investments.set_index("ID", inplace=True)
    df_investments.drop(columns="fund", inplace=True)
    df_investments.rename(
        columns={
            "company": "Company",
            "company_match": "Matched company",
            "manco": "Management company",
            "nominal_quantity": "Nominal/Quantity",
            "market_value": "Market value",
            "currency": "Currency",
            "perc_net_assets": "% net assets",
            "acquisition_cost": "Acquisition cost",
            "acquisition_currency": "Acquisition currency",
        },
        inplace=True,
    )
    df_assets_managers.set_index("ID", inplace=True)
    df_assets_managers.drop(columns="managed_funds", inplace=True)
    df_assets_managers.rename(columns={"name": "Name"}, inplace=True)

    df_funds_assets.drop(columns="fund", inplace=True)
    df_funds_assets.rename(
        columns={
            "tot_assets": "Total assets",
            "net_assets": "Total net assets",
            "liabilities": "Total liabilities",
            "currency": "Currency",
        },
        inplace=True,
    )
    df_funds_assets.set_index(["Fund ID"], inplace=True)

    df_funds.rename(columns={"name": "Name"}, inplace=True)
    df_funds.set_index(["ID", "Managment company ID"], inplace=True)

    df_investments_managers.set_index(
        ["Investment manager ID", "Fund ID"], inplace=True
    )

    df_investments = investments_schema.validate(df_investments)
    df_assets_managers = assets_managers_schema.validate(df_assets_managers)
    df_funds_assets = funds_assets_schema.validate(df_funds_assets)
    df_funds = funds_schema.validate(df_funds)
    df_investments_managers = investments_managers_schema.validate(
        df_investments_managers
    )
    return {
        "investments": df_investments,
        "assets_managers": df_assets_managers,
        "funds": df_funds,
        "funds_assets": df_funds_assets,
        "investments_managers": df_investments_managers,
        "additional_infos": add_infos,
    }


def _write_structured(
    structured_data: pd.DataFrame,
    unstructured_data: Dict[int, Dict[str, Any]],
    data_name: str,
    out_dir: Path,
) -> None:
    """Write structured data to a directory with separate files for table and metadata.

    Parameters
    ----------
    structured_data : pd.DataFrame
        Tabular data to write as CSV
    unstructured_data : Dict[int, Dict[str, Any]]
        Additional metadata to write as YAML
    data_name : str
        Name for the output directory and files
    out_dir : Path
        Parent directory where the structured output will be created
    """
    out_dir.mkdir(exist_ok=True)
    out_path = out_dir / data_name
    out_path.mkdir(exist_ok=True)
    structured_data.to_csv(out_path / "table.csv")
    yaml.dump(
        unstructured_data,
        (out_path / "dicts.yaml").open("w"),
    )


def _write_regular(
    data: Dict[str, Any],
    structured_mapping: Dict[str, str],
    unstructured_mapping: Dict[str, str],
    out_dir: Path,
) -> None:
    """Write data in regular format with separate files for different data types.

    Parameters
    ----------
    data : Dict[str, Any]
        Dictionary containing data to write
    structured_mapping : Dict[str, str]
        Mapping from data keys to output CSV filenames
    unstructured_mapping : Dict[str, str]
        Mapping from data keys to output YAML filenames
    out_dir : Path
        Directory where files will be written
    """
    out_dir.mkdir(exist_ok=True)
    for data_name, file_name in structured_mapping.items():
        data[data_name].to_csv(out_dir / file_name)
    for data_name, file_name in unstructured_mapping.items():
        yaml.dump(data[data_name], (out_dir / file_name).open("w"))


def _write_single_file(data: Dict[str, Any], file_path: Path) -> None:
    """Write all investment data to a single CSV file.

    Parameters
    ----------
    data : Dict[str, Any]
        Dictionary containing investments and additional info
    file_path : Path
        Path to the output CSV file
    """
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


def write_files(
    data: Dict[str, Any],
    out_path: Union[str, Path],
    profile: Union[OutStructureNormalMode, OutStructureBatchMode],
    flags: Union[OutFlagsNormalMode, OutFlagsBatchMode],
) -> None:
    """Write financial data to files according to specified output profile and flags.

    Parameters
    ----------
    data : Dict[str, Any]
        Dictionary containing investment data to write
    out_path : Union[str, Path]
        Output directory or file path
    profile : Union[OutStructureNormalMode, OutStructureBatchMode]
        Output structure profile determining file organization
    flags : Union[OutFlagsNormalMode, OutFlagsBatchMode]
        Output flags controlling compression and other options

    Raises
    ------
    ValueError
        If the specified profile is not recognized

    Notes
    -----
    Supported output profiles:
    - REGULAR: Separate CSV and YAML files for investments and additional info
    - SINGLE_FILE: All data combined into a single CSV file
    - STRUCTURED: Directory-based structure with table and metadata files

    Compression flags create tar.gz archives for directories or gzip for single files.
    """
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
            {
                "investments": "investments.csv",
                "funds_assets": "funds_assets.csv",
                "funds": "funds.csv",
                "assets_managers": "assets_managers.csv",
                "investments_managers": "investments_managers_to_funds.csv",
            },
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
            with gzip.open(archive_name, "wb") as f_out, out_path.open("rb") as f_in:
                shutil.copyfileobj(f_in, f_out)
            if remove_uncompressed_out:
                os.remove(out_path)
        else:
            archive_name = f"{out_path.name}.tar.gz"
            with tarfile.open(archive_name, "w:gz") as tar:
                tar.add(out_path, arcname=out_path.name)
            if remove_uncompressed_out:
                shutil.rmtree(out_path)
