"""Data module for loading and validating company and financial data.

This module provides functions to load various CSV data files containing
company information, target lists, markets, and tickers, with schema
validation to ensure data integrity.

``get_target_companies`` is now implemented in Rust — see
``packages/freeports_engine/src/input/companies_db.rs`` and
``analysis_finance_reports/agent-memory/rust-native-binary-plan.md`` (Phase D). It now returns an
already-compiled ``List[CompanyMatchInfos]`` directly (calling ``freeports_engine.core.CompanyMatchInfos``
— ``CompanyMatchInfos`` used to live in the separate ``freeports_lib`` crate, merged into
``freeports_engine`` in Fase E, see ``analysis_finance_reports/agent-memory/rust-native-binary-plan.md``)
instead of a ``pd.DataFrame`` for ``Algorithm.__call__`` to compile itself — see the matching
simplification in ``packages/freeports_engine/src/pipeline.rs``'s ``Algorithm::call``. The
original Python body (``_legacy_get_target_companies``) was removed during the freeports_core ->
freeports_engine consolidation (see
``analysis_finance_reports/agent-memory/freeports-core-consolidation-plan.md``) — it only ever
called ``get_companies_data`` below, which stays (it has other live callers).

**Bug found and fixed at the root (user confirmed, 2026-08-19)**: ``_regex_match_name``'s
validation was a complete no-op — ``re.match`` returns ``None`` on failure (not ``False``), and
``pandas.Series.all()`` defaults to ``skipna=True``, which silently drops ``None`` instead of
treating it as a failed check. Combined with a second issue (the check used position-0-anchored
``re.match`` while every real consumer of the same pattern searches unanchored), this let 6 rows
of genuinely non-matching data into ``companies.csv`` unnoticed. The Rust port fixes both: a
failed match now actually raises, and matching is unanchored (searches anywhere), consistent with
how these patterns are used everywhere else. 5 of those 6 rows already used unanchored-compatible
patterns and needed no data change; the 6th (Airbnb's own ``Regex`` mistakenly referencing its
ticker, ``\\babnb``) was fixed in ``analysis_finance_reports_formats/tests/input_db``: the ticker
pattern moved to ``companies_additional_regexs.csv`` (where it doesn't need to match the name),
and a real name-matching regex (``\\bairbnb\\b``) took its place in ``companies.csv``.
"""

from pathlib import Path
from copy import deepcopy
import re
import logging as log
from typing import List, Union
import pandera.pandas as pa
import pandas as pd
import freeports_engine
from freeports.i18n import _

logger = log.getLogger()

get_target_companies = freeports_engine.core.get_target_companies
deep_normalize_string = freeports_engine.core.deep_normalize_string

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
                deep_normalize_string(row["Bud"]) in deep_normalize_string(row["Name"])
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
            lambda row: re.match(row["Regex"], deep_normalize_string(row["Name"])),
            axis=1,
        )
        if not check_mask.all():
            invalid_rows = valid_rows[~check_mask]
            logger.error(_("Regex not matching name for rows:"))
            logger.error(str(invalid_rows))
            raise ValueError(_("Principal regex has to be contained in complete name"))
    return True


# Structure of the dataframe to validate the companies list everytime it is imported
companies_schema = pa.DataFrameSchema(
    columns={
        "Name": pa.Column(
            pd.StringDtype,
            checks=pa.Check(lambda x: x.apply(deep_normalize_string) == x),
        ),
        "Bud": pa.Column(
            pd.StringDtype,
            checks=pa.Check(lambda x: x.apply(deep_normalize_string) == x),
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


def get_companies(input_db_directory: Path) -> pd.DataFrame:
    """Load and validate the list of companies from companies.csv.

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of companies with normalized names
    """
    df = pd.read_csv(input_db_directory / COMPANIES_DIR / "companies.csv")
    df.set_index("Name", drop=False, inplace=True)
    df["Name"] = df["Name"].apply(deep_normalize_string)
    df = companies_schema.validate(df)
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


def get_companies_additional_regexs_schema(company_names: List[str]):
    """Build schema validating additional regexs index against known company names."""
    schema = deepcopy(_companies_additional_regexs_schema)  # pylint: disable=no-member
    schema.index.checks.append(pa.Check.isin(company_names))
    return schema


def get_companies_additional_regexs(
    input_db_directory: Path, company_names: List[str]
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

    return get_companies_additional_regexs_schema(company_names).validate(df)


# Structure of the dataframe to validate the lists table
lists_schema = pa.DataFrameSchema(
    columns={
        "Institution": pa.Column(pd.StringDtype),
        "Date": pa.Column("datetime64[ns]"),
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
            checks=pa.Check(lambda x: x.apply(deep_normalize_string) == x),
        )
    },
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        name="Company name",
    ),
)


def get_companies_additional_buds_schema(company_names: List[str]):
    """Build schema validating additional BUDs index against known company names."""
    schema = deepcopy(_companies_additional_buds_schema)  # pylint: disable=no-member
    schema.index.checks.append(pa.Check.isin(company_names))
    return schema


def get_companies_additional_buds(
    input_db_directory: Path, company_names: List[str]
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
    return get_companies_additional_buds_schema(company_names).validate(df)


def get_lists(input_db_directory: Path) -> pd.DataFrame:
    """Load and validate target lists from lists.csv.

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of target lists
    """
    df = pd.read_csv(input_db_directory / LISTS_DIR / "lists.csv", index_col="Name")
    df = lists_schema.validate(df)
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


def get_company_to_list_schema(list_names: List[str], company_names: List[str]):
    """Build schema validating company-to-list mappings against known names."""
    schema = deepcopy(_company_to_list_schema)  # pylint: disable=no-member
    schema.columns["List name"].checks.append(pa.Check.isin(list_names))
    schema.index.indexes[0].checks.append(pa.Check.isin(list_names))
    schema.index.indexes[1].checks.append(pa.Check.isin(company_names))
    return schema


def get_company_to_list(
    input_db_directory: Path, list_names: List[str], company_names: List[str]
) -> pd.DataFrame:
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
    return get_company_to_list_schema(list_names, company_names).validate(df)


# Structure of the dataframe to validate the markets table
markets_schema = pa.DataFrameSchema(
    columns={},
    coerce=True,
    strict=True,
    index=pa.Index(pd.StringDtype, unique=True, name="Name"),
)


def get_markets(input_db_directory: Path) -> pd.DataFrame:
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


def get_tickers_schema(market_names: List[str], company_names: List[str]):
    """Build schema validating tickers index against known market and company names."""
    schema = deepcopy(_tickers_schema)  # pylint: disable=no-member
    schema.index.indexes[0].checks.append(pa.Check.isin(market_names))
    schema.index.indexes[1].checks.append(pa.Check.isin(company_names))
    return schema


def get_tickers(
    input_db_directory: Path, market_names: List[str], company_names: List[str]
) -> pd.DataFrame:
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
    return get_tickers_schema(market_names, company_names).validate(df)


def get_companies_data(input_db_directory: Path) -> pd.DataFrame:
    """Load and combine all company-related data into a comprehensive DataFrame.

    Returns
    -------
    pd.DataFrame
        Combined DataFrame containing companies, lists, tickers, BUDs, and regex patterns
    """
    markets_df = get_markets(input_db_directory)
    market_names = markets_df.index.to_list()
    lists_df = get_lists(input_db_directory)
    list_names = lists_df.index.to_list()
    companies = get_companies(input_db_directory)
    company_names = companies.index.to_list()
    company_to_list = get_company_to_list(input_db_directory, list_names, company_names)
    tickers = get_tickers(input_db_directory, market_names, company_names)
    additional_buds = get_companies_additional_buds(input_db_directory, company_names)
    additional_regexs = get_companies_additional_regexs(
        input_db_directory, company_names
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
