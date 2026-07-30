"""Data module for loading and validating company and financial data.

This module provides functions to load various CSV data files containing
company information, target lists, markets, and tickers, with schema
validation to ensure data integrity.
"""

from pathlib import Path
import datetime
import re
import logging as log
from typing import List, Union
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.i18n import _
from freeports_analysis import match

logger = log.getLogger()

LISTS_DIR = "lists"
COMPANIES_DIR = "companies"


def _stem_contained_in_name(df: pd.DataFrame) -> bool:
    """Check if the main BUD is included inside the company name.

    Parameters
    ----------
    df : pd.DataFrame
        DataFrame of companies

    Returns
    -------
    bool
        True if all BUDs are contained in their respective company names

    Raises
    ------
    ValueError
        If any principal BUD is not contained in the company name
    """
    mask = df["Bud"].notna()

    # Apply check only where Bud is not null
    valid_rows = df[mask]

    if not valid_rows.empty:
        check_mask = valid_rows.apply(
            lambda row: (
                match.normalize_string(row["Bud"])
                in match.normalize_string(row["Name"])
            ),
            axis=1,
        )
        if not check_mask.all():
            logger.error(_("Invalid principal buds"))
            logger.error(str(valid_rows[~check_mask]))
            raise ValueError(_("Principal bud has to be contained in complete name"))
    return True


def _regex_match_name(df: pd.DataFrame) -> bool:
    """Check if the main regex matches the company name.

    Parameters
    ----------
    df : pd.DataFrame
        DataFrame of companies

    Returns
    -------
    bool
        True if all regex patterns match their respective company names

    Raises
    ------
    ValueError
        If any regex pattern does not match the company name
    """
    mask = df["Regex"].notna()

    valid_rows = df[mask]

    if not valid_rows.empty:
        check_mask = valid_rows.apply(
            lambda row: re.match(row["Regex"], match.normalize_string(row["Name"])),
            axis=1,
        )
        if not check_mask.all():
            invalid_rows = valid_rows[~check_mask]
            logger.error(_("Regex not matching name for rows:"))
            logger.error(str(invalid_rows))
            raise ValueError(_("Principal bud has to be contained in complete name"))
    return True


# Structure of the dataframe to validate the companies list everytime it is imported
companies_schema = pa.DataFrameSchema(
    columns={
        "Name": pa.Column(
            pd.StringDtype,
            checks=pa.Check(lambda x: x.apply(match.normalize_string) == x),
        ),
        "Bud": pa.Column(
            pd.StringDtype,
            checks=pa.Check(lambda x: x.apply(match.normalize_string) == x),
            nullable=True,
        ),
        "Regex": pa.Column(pd.StringDtype, nullable=True),
    },
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        name="Name",
        unique=True,
    ),
    checks=[pa.Check(_stem_contained_in_name), pa.Check(_regex_match_name)],
)


def get_companies(input_db_directory: Path, input_db_validation_data) -> pd.DataFrame:
    """Load and validate the list of companies from companies.csv.

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of companies with normalized names
    """
    df = pd.read_csv(input_db_directory / COMPANIES_DIR / "companies.csv")
    df.set_index("Name", drop=False, inplace=True)
    df["Name"] = df["Name"].apply(match.normalize_string)
    df = companies_schema.validate(df)
    input_db_validation_data.companies = df.index.to_list()
    return df


# Structure of the dataframe to validate the additional regex table
_companies_additional_regexs_schema = pa.DataFrameSchema(
    columns={"Regex": pa.Column(pd.StringDtype)},
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        name="Company name",
    ),
)


def get_companies_additional_regexs_schema(input_db_validation_data):
    _companies_additional_regexs_schema.index.checks.append(
        pa.Check.isin(input_db_validation_data.companies)
    )
    return _companies_additional_regexs_schema


def get_companies_additional_regexs(
    input_db_directory, input_db_validation_data
) -> pd.DataFrame:
    """Load and validate additional regex patterns from companies_additional_regexs.csv.

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of additional regex patterns
    """
    df = pd.read_csv(
        input_db_directory / COMPANIES_DIR / "companies_additional_regexs.csv",
        index_col=["Company name"],
    )

    return get_companies_additional_regexs_schema(input_db_validation_data).validate(df)


# Structure of the dataframe to validate the lists table
lists_schema = pa.DataFrameSchema(
    columns={
        "Institution": pa.Column(pd.StringDtype),
        "Date": pa.Column(datetime.date),
    },
    coerce=True,
    strict=True,
    index=pa.Index(pd.StringDtype, name="Name", unique=True),
)

# Structure of the dataframe to validate the additional buds table
_companies_additional_buds_schema = pa.DataFrameSchema(
    columns={
        "Bud": pa.Column(
            pd.StringDtype,
            checks=pa.Check(lambda x: x.apply(match.normalize_string) == x),
        )
    },
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        name="Company name",
    ),
)


def get_companies_additional_buds_schema(input_db_validation_data):
    _companies_additional_buds_schema.index.checks.append(
        pa.Check.isin(input_db_validation_data.companies)
    )
    return _companies_additional_buds_schema


def get_companies_additional_buds(
    input_db_directory, input_db_validation_data
) -> pd.DataFrame:
    """Load and validate additional BUDs from companies_additional_buds.csv.

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of additional BUDs
    """
    df = pd.read_csv(
        input_db_directory / COMPANIES_DIR / "companies_additional_buds.csv",
        index_col=["Company name"],
    )
    return get_companies_additional_buds_schema(input_db_validation_data).validate(df)


def get_lists(input_db_directory, input_db_validation_data) -> pd.DataFrame:
    """Load and validate target lists from lists.csv.

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of target lists
    """
    df = pd.read_csv(input_db_directory / LISTS_DIR / "lists.csv", index_col="Name")
    df = lists_schema.validate(df)
    input_db_validation_data.lists = df.index.to_list()
    return df


# Structure of the dataframe to validate the company_to_lists table
_company_to_list_schema = pa.DataFrameSchema(
    columns={"List name": pa.Column(pd.StringDtype)},
    coerce=True,
    strict=True,
    index=pa.MultiIndex(
        [
            pa.Index(
                pd.StringDtype,
                name="List name",
            ),
            pa.Index(
                pd.StringDtype,
                name="Company name",
            ),
        ]
    ),
)


def get_company_to_list_schema(input_db_validation_data):
    _company_to_list_schema.columns["List name"].checks.append(
        pa.Check.isin(input_db_validation_data.lists)
    )
    _company_to_list_schema.index.indexes[0].checks.append(
        pa.Check.isin(input_db_validation_data.lists)
    )
    _company_to_list_schema.index.indexes[1].checks.append(
        pa.Check.isin(input_db_validation_data.companies)
    )
    return _company_to_list_schema


def get_company_to_list(input_db_directory, input_db_validation_data) -> pd.DataFrame:
    """Load and validate company-to-list mappings from company_to_list.csv.

    Returns
    -------
    pd.DataFrame
        Validated DataFrame mapping companies to their target lists
    """
    df = pd.read_csv(
        input_db_directory / LISTS_DIR / "company_to_list.csv",
        index_col=["List name", "Company name"],
    )
    df["List name"] = df.index.get_level_values("List name")
    return get_company_to_list_schema(input_db_validation_data).validate(df)


# Structure of the dataframe to validate the markets table
markets_schema = pa.DataFrameSchema(
    columns={},
    coerce=True,
    strict=True,
    index=pa.Index(pd.StringDtype, unique=True, name="Name"),
)


def get_markets(input_db_directory, input_db_validation_data) -> pd.DataFrame:
    """Load and validate market information from markets.csv.

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of markets
    """
    df = pd.read_csv(
        input_db_directory / COMPANIES_DIR / "markets.csv", index_col="Name"
    )
    df = markets_schema.validate(df)
    input_db_validation_data.markets = df.index.to_list()
    return df


# Structure of the dataframe to validate the tickers table
_tickers_schema = pa.DataFrameSchema(
    columns={
        "Symbol": pa.Column(
            pd.StringDtype, checks=pa.Check(lambda x: x.str.match("^[A-Z]{2,6}$"))
        )
    },
    coerce=True,
    strict=True,
    index=pa.MultiIndex(
        [
            pa.Index(
                pd.StringDtype,
                name="Market name",
            ),
            pa.Index(
                pd.StringDtype,
                name="Company name",
            ),
        ]
    ),
)


def get_tickers_schema(input_db_validation_data):
    _tickers_schema.index.indexes[0].checks.append(
        pa.Check.isin(input_db_validation_data.markets)
    )
    _tickers_schema.index.indexes[1].checks.append(
        pa.Check.isin(input_db_validation_data.companies)
    )
    return _tickers_schema


def get_tickers(input_db_directory, input_db_validation_data) -> pd.DataFrame:
    """Load and validate ticker information from tickers.csv.

    Returns
    -------
    pd.DataFrame
        Validated DataFrame mapping companies to their market symbols
    """
    df = pd.read_csv(
        input_db_directory / COMPANIES_DIR / "tickers.csv",
        index_col=["Market name", "Company name"],
    )
    return get_tickers_schema(input_db_validation_data).validate(df)


def get_companies_data(input_db_directory, input_db_validation_data) -> pd.DataFrame:
    """Load and combine all company-related data into a comprehensive DataFrame.

    Returns
    -------
    pd.DataFrame
        Combined DataFrame containing companies, lists, tickers, BUDs, and regex patterns
    """
    get_markets(input_db_directory, input_db_validation_data)
    get_lists(input_db_directory, input_db_validation_data)
    companies = get_companies(input_db_directory, input_db_validation_data)
    company_to_list = get_company_to_list(input_db_directory, input_db_validation_data)
    tickers = get_tickers(input_db_directory, input_db_validation_data)
    additional_buds = get_companies_additional_buds(
        input_db_directory, input_db_validation_data
    )
    additional_regexs = get_companies_additional_regexs(
        input_db_directory, input_db_validation_data
    )

    additional_buds_agg = additional_buds.groupby(level="Company name").agg(
        {"Bud": list}
    )
    additional_regexs_agg = additional_regexs.groupby(level="Company name").agg(
        {"Regex": list}
    )
    company_to_lists_agg = company_to_list.groupby(level="Company name").agg(
        {"List name": list}
    )
    tickers_agg = tickers.groupby(level="Company name").agg({"Symbol": list})
    results = (
        companies.join(company_to_lists_agg, how="left", validate="one_to_one")
        .join(tickers_agg, how="left", validate="one_to_one")
        .join(
            additional_buds_agg,
            how="left",
            validate="one_to_one",
            rsuffix="s additional",
        )
        .join(
            additional_regexs_agg,
            how="left",
            validate="one_to_one",
            rsuffix="s additional",
        )
    )
    results["Bud"] = results["Bud"].apply(lambda x: [] if pd.isna(x) else [x])
    results["Buds additional"] = results["Buds additional"].apply(
        lambda x: x if isinstance(x, list) else []
    )
    results["Regex"] = results["Regex"].apply(lambda x: [] if pd.isna(x) else [x])
    results["Regexs additional"] = results["Regexs additional"].apply(
        lambda x: x if isinstance(x, list) else []
    )
    results["Buds"] = results["Bud"] + results["Buds additional"]
    results["Regexs"] = results["Regex"] + results["Regexs additional"]
    results.drop(
        columns=["Bud", "Buds additional", "Regex", "Regexs additional"], inplace=True
    )
    results["List name"] = results["List name"].apply(
        lambda x: x if isinstance(x, list) else []
    )
    results["Symbol"] = results["Symbol"].apply(
        lambda x: x if isinstance(x, list) else []
    )
    results.rename(
        columns={"List name": "List names", "Symbol": "Symbols"}, inplace=True
    )
    return results


def get_target_companies(
    input_db_directory: Path,
    target_lists: Union[List[str], str],
    input_db_validation_data,
) -> pd.DataFrame:
    """Filter companies data to include only those in specified target lists.

    Parameters
    ----------
    target_lists : Union[List[str], str]
        The required list name or list of list names

    Returns
    -------
    pd.DataFrame
        Filtered DataFrame containing only companies from the specified lists

    Notes
    -----
    The returned DataFrame includes all company information but excludes
    the 'List names' column since it's used for filtering. Companies are
    included if they belong to any of the specified target lists.
    """
    if isinstance(target_lists, str):
        target_lists = [target_lists]
    df = get_companies_data(input_db_directory, input_db_validation_data)
    filtered_df = df[
        df["List names"].apply(
            lambda x: any(list_name in x for list_name in target_lists)
        )
    ]
    filtered_df = filtered_df.drop(columns=["List names"])
    return filtered_df
