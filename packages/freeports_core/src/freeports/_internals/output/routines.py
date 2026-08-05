"""Routines to transform the classes into the csv form and to output on file."""

from typing import Any, Dict, Iterator, List, Union
from pathlib import Path
import os
import gzip
import shutil
import tarfile
import yaml

import pandas as pd

from freeports.i18n import _
from freeports.consts import SfdrArticle
from freeports._internals.cli.conf_parse import (
    OutFlagsBatchMode,
    OutFlagsNormalMode,
    OutStructureBatchMode,
    OutStructureNormalMode,
)
from freeports.output import (
    Equity,
    Bond,
    ManagementCompany,
    InvestmentsManager,
    Fund,
    FundSfdrClassification,
    FundEsgIndicator,
    FundAssets,
    FundRename,
    FundMerge,
)
from .files_schema import (
    BondAdditionalInfos,
    investments_schema,
    funds_schema,
    funds_sfdr_classification_schema,
    funds_esg_indicators_schema,
    funds_assets_schema,
    funds_change_name_schema,
    assets_managers_schema,
    investments_managers_schema,
)


class ResultsAccumulator:
    """Accumulate parsed results across all documents and pages for output generation.

    Holds lists and dictionaries for each entity type, and provides
    sequential ID counters via properties.
    """

    investments: List[Dict[str, Any]] = []
    funds: Dict[Any, Dict[str, Any]] = {}
    add_infos: Dict[int, Any] = {}
    assets_managers: Dict[str, Dict[str, Any]] = {}
    funds_change_name: List[Dict[str, Any]] = []
    funds_assets: List[Dict[str, Any]] = []
    funds_sfdr_classification: List[Dict[str, Any]] = []
    funds_esg_indicators: List[Dict[str, Any]] = []
    investments_managers_to_funds: List[Dict[str, Any]] = []

    def __init__(self):
        self.investments = []
        self.funds = {}
        self.add_infos = {}
        self.funds_change_name = []
        self.funds_assets = []
        self.funds_sfdr_classification = []
        self.funds_esg_indicators = []
        self.investments_managers_to_funds = []

    @property
    def new_investment_id(self) -> int:
        """Next available investment ID."""
        return len(self.investments) + 1

    @property
    def new_asset_manager_id(self) -> int:
        """Next available asset manager ID."""
        return len(self.assets_managers) + 1

    @property
    def new_fund_id(self) -> int:
        """Next available fund ID."""
        return len(self.funds) + 1

    @property
    def new_fund_asset_id(self) -> int:
        """Next available fund asset ID."""
        return len(self.funds_assets) + 1

    @property
    def new_fund_change_name_id(self) -> int:
        """Next available fund change name ID."""
        return len(self.funds_change_name) + 1


class PageResults:
    """Holds parsed entities for a single page of a document."""

    investments: List[Equity | Bond]
    assets_managers: List[ManagementCompany | InvestmentsManager]
    funds: List[Fund]
    funds_sfdr_classification: List[FundSfdrClassification]
    funds_esg_indicators: List[FundEsgIndicator]
    funds_assets: List[FundAssets]
    funds_change_name: List[FundRename | FundMerge]

    def __init__(self):
        self.investments = []
        self.assets_managers = []
        self.funds = []
        self.funds_sfdr_classification = []
        self.funds_esg_indicators = []
        self.funds_assets = []
        self.funds_change_name = []

    def _fulfill_and_filter(
        self,
        old_list: List[Any],
        promises_resolution_map: Dict[str, Any],
    ) -> List[Any]:
        """Resolve promises for items in a list, filtering out those that fail.

        Parameters
        ----------
        old_list : List[Any]
            List of entities with promises to resolve.
        promises_resolution_map : Dict[str, Any]
            Mapping used to resolve unfilled promises.

        Returns
        -------
        List[Any]
            List of entities whose promises were successfully resolved.
        """
        new_list: List[Any] = []
        for v in old_list:
            try:
                v.fulfill_promises(promises_resolution_map)
                new_list.append(v)
            except KeyError:
                pass

        return new_list

    def fulfill_promises(self, promises_resolution_map: Dict[str, Any]) -> None:
        """Resolve promises for all entity lists on this page.

        Parameters
        ----------
        promises_resolution_map : Dict[str, Any]
            Mapping used to resolve unfilled promises.
        """
        self.investments = self._fulfill_and_filter(
            self.investments, promises_resolution_map
        )
        self.assets_managers = self._fulfill_and_filter(
            self.assets_managers, promises_resolution_map
        )
        self.funds = self._fulfill_and_filter(self.funds, promises_resolution_map)
        self.funds_assets = self._fulfill_and_filter(
            self.funds_assets, promises_resolution_map
        )
        self.funds_change_name = self._fulfill_and_filter(
            self.funds_change_name, promises_resolution_map
        )
        self.funds_sfdr_classification = self._fulfill_and_filter(
            self.funds_sfdr_classification, promises_resolution_map
        )
        self.funds_esg_indicators = self._fulfill_and_filter(
            self.funds_esg_indicators, promises_resolution_map
        )


class PageIndexable:
    """Lightweight wrapper providing 1-based page indexing into a list of page results."""

    data_per_page: List[Any]

    def __init__(self, data: List[Any]):
        self.data_per_page = data

    def __getitem__(self, page_n: int) -> Any:
        """Return page data for the given 1-based page number.

        Parameters
        ----------
        page_n : int
            1-based page index.

        Returns
        -------
        Any
            Data for the requested page.
        """
        return self.data_per_page[page_n - 1]


class DocumentResults:
    """Holds page-level results for a single processed document."""

    prefix_out: str
    algorithm: str
    results: List[PageResults]

    def __init__(self, prefix_out: str, algorithm: str):
        """Initialize document results.

        Parameters
        ----------
        prefix_out : str
            Output prefix for this document.
        algorithm : str
            Name of the extraction algorithm used.
        """
        self.prefix_out = (prefix_out,)
        self.algorithm = algorithm
        self.results = []

    @property
    def investment(self) -> PageIndexable:
        """Paginated view of investments across all pages."""
        return PageIndexable(list(map(lambda x: x.investment), self.results))

    @property
    def assets_managers(self) -> PageIndexable:
        """Paginated view of asset managers across all pages."""
        return PageIndexable(list(map(lambda x: x.assets_managers), self.results))

    @property
    def funds(self) -> PageIndexable:
        """Paginated view of funds across all pages."""
        return PageIndexable(list(map(lambda x: x.funds), self.results))

    @property
    def funds_sfdr_classification(self) -> PageIndexable:
        """Paginated view of SFDR classifications across all pages."""
        return PageIndexable(
            list(map(lambda x: x.funds_sfdr_classification), self.results)
        )

    @property
    def funds_esg_indicators(self) -> PageIndexable:
        """Paginated view of ESG indicators across all pages."""
        return PageIndexable(list(map(lambda x: x.funds_esg_indicators), self.results))

    @property
    def funds_assets(self) -> PageIndexable:
        """Paginated view of fund assets across all pages."""
        return PageIndexable(list(map(lambda x: x.funds_assets), self.results))

    @property
    def funds_change_name(self) -> PageIndexable:
        """Paginated view of fund name changes across all pages."""
        return PageIndexable(list(map(lambda x: x.funds_change_name), self.results))

    def __getitem__(self, page_n: int) -> PageResults:
        """Return page results for the given 1-based page number.

        Parameters
        ----------
        page_n : int
            1-based page index.

        Returns
        -------
        PageResults
            Parsed results for the requested page.
        """
        return self.results[page_n - 1]

    def __iter__(self) -> Iterator[PageResults]:
        """Iterate over page results."""
        return iter(self.results)

    def add_batch_infos(self, d: Dict[str, Any]) -> Dict[str, Any]:
        """Add algorithm and document metadata to a result dictionary.

        Parameters
        ----------
        d : Dict[str, Any]
            Result dictionary to augment.

        Returns
        -------
        Dict[str, Any]
            Augmented dictionary with Format and Document entries.
        """
        d["Format"] = self.algorithm
        d["Document"] = self.prefix_out
        return d

    def fulfill_promises(self, promises_resolution_map: Dict[str, Any]) -> None:
        """Resolve promises for all page results in this document.

        Parameters
        ----------
        promises_resolution_map : Dict[str, Any]
            Mapping used to resolve unfilled promises.
        """
        for pr in self:
            pr.fulfill_promises(promises_resolution_map)


class CompanyValidator:
    """Pydantic-compatible validator that checks company names against a predefined list.

    Parameters
    ----------
    companies : List[str]
        List of valid company names.
    """

    companies: List[str] | None = None

    def __init__(self, companies: List[str]) -> None:
        """Initialize the validator with a list of allowed company names.

        Parameters
        ----------
        companies : List[str]
            List of valid company names.
        """
        self.companies = companies

    def __call__(self, value: str) -> str:
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
        if value not in self.companies:
            raise ValueError(f"Company must be one of {self.companies}, got '{value}'")
        return value


def add_debug_infos(
    batch_mode: bool,
    document_results: DocumentResults,
    n_page: int,
    d: Dict[str, Any],
) -> Dict[str, Any]:
    """Add debug metadata (page number, format, document) to a result dictionary.

    Parameters
    ----------
    batch_mode : bool
        Whether processing is in batch mode.
    document_results : DocumentResults
        Document results used to add batch-specific infos.
    n_page : int
        Current page number.
    d : Dict[str, Any]
        Result dictionary to augment.

    Returns
    -------
    Dict[str, Any]
        Augmented dictionary with debug metadata.
    """
    d["Report page"] = n_page
    if batch_mode:
        d = document_results.add_batch_infos(d)
    return d


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
    curr_results = ResultsAccumulator()

    for document_results in results:
        for page_n, page_results in enumerate(document_results, start=1):
            for f in page_results.funds:
                if f is None:
                    continue
                if f not in curr_results.funds:
                    d = f.model_dump(mode="json", by_alias=True)
                    d["ID"] = curr_results.new_fund_id
                    d["Management company ID"] = None
                    curr_results.funds[f] = d
                if "Report page" not in curr_results.funds[f]:
                    curr_results.funds[f] = add_debug_infos(
                        batch_mode, document_results, page_n, curr_results.funds[f]
                    )
            for fcm in page_results.funds_change_name:
                if fcm is None:
                    continue
                d = fcm.model_dump(mode="json", by_alias=True)
                f = Fund(name=fcm.current_name)
                if f not in curr_results.funds:
                    curr_results.funds[f] = {
                        "ID": curr_results.new_fund_id,
                        "Name": f.name,
                    }
                d = add_debug_infos(batch_mode, document_results, page_n, d)
                if isinstance(fcm, FundRename):
                    d["Type of event"] = "RENAMING"
                elif isinstance(fcm, FundMerge):
                    d["Type of event"] = "MERGING"
                d["Fund ID"] = curr_results.funds[f]["ID"]
                d["ID"] = curr_results.new_fund_change_name_id
                curr_results.funds_change_name.append(d)

            for fa in page_results.funds_assets:
                if fa is None:
                    continue
                d = fa.model_dump(mode="json", by_alias=True)
                f = Fund(name=fa.fund)
                if f not in curr_results.funds:
                    curr_results.funds[f] = {
                        "ID": curr_results.new_fund_id,
                        "Name": f.name,
                    }
                d = add_debug_infos(batch_mode, document_results, page_n, d)
                d["Fund ID"] = curr_results.funds[f]["ID"]
                d["ID"] = curr_results.new_fund_asset_id
                curr_results.funds_assets.append(d)

            for fsc in page_results.funds_sfdr_classification:
                if fsc is None:
                    continue
                d = fsc.model_dump(mode="json", by_alias=True)
                if fsc.article == SfdrArticle.ART_6:
                    d["SFDR classification"] = "Art. 6"
                elif fsc.article == SfdrArticle.ART_8:
                    d["SFDR classification"] = "Art. 8"
                elif fsc.article == SfdrArticle.ART_9:
                    d["SFDR classification"] = "Art. 9"
                else:
                    raise ValueError("SFDR classification value not recognized")
                f = Fund(name=fsc.fund)
                if f not in curr_results.funds:
                    curr_results.funds[f] = {
                        "ID": curr_results.new_fund_id,
                        "Name": f.name,
                    }
                d = add_debug_infos(batch_mode, document_results, page_n, d)
                d["Fund ID"] = curr_results.funds[f]["ID"]
                curr_results.funds_sfdr_classification.append(d)

            for fei in page_results.funds_esg_indicators:
                if fei is None:
                    continue
                d = fei.model_dump(mode="json", by_alias=True)
                f = Fund(name=fei.fund)
                if f not in curr_results.funds:
                    curr_results.funds[f] = {
                        "ID": curr_results.new_fund_id,
                        "Name": f.name,
                    }
                d = add_debug_infos(batch_mode, document_results, page_n, d)
                d["Fund ID"] = curr_results.funds[f]["ID"]
                curr_results.funds_esg_indicators.append(d)

            for i in page_results.investments:
                if i is None:
                    continue
                d = i.model_dump(mode="json", by_alias=True)
                f = Fund(name=i.fund)
                if f not in curr_results.funds:
                    curr_results.funds[f] = {
                        "ID": curr_results.new_fund_id,
                        "Name": f.name,
                    }
                d = add_debug_infos(batch_mode, document_results, page_n, d)
                d["ID"] = curr_results.new_investment_id
                d["Fund ID"] = curr_results.funds[f]["ID"]
                if isinstance(i, Equity):
                    d["Financial instrument"] = "EQUITY"
                elif isinstance(i, Bond):
                    d["Financial instrument"] = "BOND"
                    infos = ["maturity", "interest_rate"]
                    curr_results.add_infos[d["ID"]] = BondAdditionalInfos(
                        **{k: v for k, v in d.items() if k in infos}
                    ).model_dump(mode="json", by_alias=True)
                    d = {k: v for k, v in d.items() if k not in infos}
                curr_results.investments.append(d)

            for am in page_results.assets_managers:
                if am is None:
                    continue
                d = am.model_dump(mode="json", by_alias=True)
                if am.name not in curr_results.assets_managers:
                    d["ID"] = curr_results.new_asset_manager_id
                    d = add_debug_infos(batch_mode, document_results, page_n, d)
                    curr_results.assets_managers[am.name] = d
                for s in am.managed_funds:
                    f = Fund(name=s)
                    if f not in curr_results.funds:
                        curr_results.funds[f] = {
                            "ID": curr_results.new_fund_id,
                            "Name": f.name,
                        }
                    if isinstance(am, ManagementCompany):
                        curr_results.funds[f]["Managment company ID"] = (
                            curr_results.assets_managers[am.name]["ID"]
                        )
                    if isinstance(am, InvestmentsManager):
                        curr_results.investments_managers_to_funds.append(
                            {
                                "Investment manager ID": curr_results.assets_managers[
                                    am.name
                                ]["ID"],
                                "Fund ID": curr_results.funds[f]["ID"],
                            }
                        )

    components = [
        ("investments", curr_results.investments, investments_schema),
        (
            "assets_managers",
            list(curr_results.assets_managers.values()),
            assets_managers_schema,
        ),
        ("funds", list(curr_results.funds.values()), funds_schema),
        (
            "investments_managers",
            curr_results.investments_managers_to_funds,
            investments_managers_schema,
        ),
        (
            "funds_sfdr_classification",
            curr_results.funds_sfdr_classification,
            funds_sfdr_classification_schema,
        ),
        (
            "funds_esg_indicators",
            curr_results.funds_esg_indicators,
            funds_esg_indicators_schema,
        ),
        ("funds_change_name", curr_results.funds_change_name, funds_change_name_schema),
        ("funds_assets", curr_results.funds_assets, funds_assets_schema),
    ]
    validated_dataframes = {}
    for k, res_list, schema in components:
        r = None
        columns = [
            c
            for c in schema.columns.keys()
            if batch_mode or c not in ("Format", "Document")
        ]
        if len(res_list) > 0:
            r = pd.DataFrame.from_records(res_list, columns=columns)
        else:
            r = pd.DataFrame(columns=columns)
        validated_dataframes[k] = schema.validate(r)

    return {
        **validated_dataframes,
        "additional_infos": curr_results.add_infos,
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
        data[data_name].to_csv(out_dir / file_name, index=False)
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
                "funds_sfdr_classification": "funds_sfdr_classification.csv",
                "funds_esg_indicators": "funds_esg_indicators.csv",
                "assets_managers": "assets_managers.csv",
                "investments_managers": "investments_managers_to_funds.csv",
                "funds_change_name": "funds_change_name.csv",
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
