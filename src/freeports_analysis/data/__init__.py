from pathlib import Path
import datetime
import re
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.i18n import _
from freeports_analysis.formats.utils.text_extract.match import normalize_string

data = Path(__file__).parent


def _stem_contained_in_name(df: pd.DataFrame) -> bool:
    """Checks if the main BUD is included inside the company name

    Parameters
    ----------
    df : pd.DataFrame
        DataFrame of companies

    Returns
    -------
    bool
        True if included

    Raises
    ------
    ValueError
        Raises error if main BUD not included in company name
    """
    mask = df["Bud"].notna()

    # Applica il controllo solo dove Bud non è nullo
    valid_rows = df[mask]

    if not valid_rows.empty:
        check_mask = valid_rows.apply(
            lambda row: normalize_string(row["Bud"]) in normalize_string(row["Name"]),
            axis=1,
        )
        if not check_mask.all():
            raise ValueError(_("Principal bud has to be contained in complete name"))
    return True


def _regex_match_name(df: pd.DataFrame) -> bool:
    """Checks if the main regex is included inside the company name

    Parameters
    ----------
    df : pd.DataFrame
        DataFrame of companies

    Returns
    -------
    bool
        True if included

    Raises
    ------
    ValueError
        _descripRaises error if main regex not included in company nametion_
    """
    mask = df["Regex"].notna()

    valid_rows = df[mask]

    if not valid_rows.empty:
        check_mask = valid_rows.apply(
            lambda row: re.match(row["Regex"], normalize_string(row["Name"])), axis=1
        )
        if not check_mask.all():
            raise ValueError(_("Principal bud has to be contained in complete name"))
    return True


# Structure of the dataframe to validate the companies list everytime it is imported
companies_schema = pa.DataFrameSchema(
    columns={
        "Name": pa.Column(
            pd.StringDtype,
            checks=pa.Check(lambda x: x.apply(normalize_string) == x),
        ),
        "Bud": pa.Column(
            pd.StringDtype,
            checks=pa.Check(lambda x: x.apply(normalize_string) == x),
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


def get_companies() -> pd.DataFrame:
    """Function called to get the list of companies contained in companies.csv
    while validating the structure through companies_schema

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of companies
    """
    df = pd.read_csv(data / "companies.csv")
    df.set_index("Name", drop=False, inplace=True)
    df["Name"] = df["Name"].apply(normalize_string)
    return companies_schema.validate(df)


# A list containing the companies
COMPANIES = get_companies().index.to_list()

# Structure of the dataframe to validate the additional regex table
companies_additional_regexs_schema = pa.DataFrameSchema(
    columns={"Regex": pa.Column(pd.StringDtype)},
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        checks=pa.Check(lambda x: x.isin(COMPANIES)),
        name="Company name",
    ),
)


def get_companies_additional_regexs() -> pd.DataFrame:
    """Function called to get the additional regex contained in companies_additional_regexs.csv
    while validating the structure through companies_additional_regexs_schema

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of additional regex
    """
    df = pd.read_csv(
        data / "companies_additional_regexs.csv", index_col=["Company name"]
    )
    return companies_additional_regexs_schema.validate(df)


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
companies_additional_buds_schema = pa.DataFrameSchema(
    columns={
        "Bud": pa.Column(
            pd.StringDtype,
            checks=pa.Check(lambda x: x.apply(normalize_string) == x),
        )
    },
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        checks=pa.Check(lambda x: x.isin(COMPANIES)),
        name="Company name",
    ),
)


def get_companies_additional_buds() -> pd.DataFrame:
    """Function called to get the additional buds contained in companies_additional_buds.csv
    while validating the structure through companies_additional_buds_schema

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of additional buds
    """
    df = pd.read_csv(data / "companies_additional_buds.csv", index_col=["Company name"])
    return companies_additional_buds_schema.validate(df)


def get_lists() -> pd.DataFrame:
    """Function called to get the additional lists contained in lists.csv
    while validating the structure through lists_schema

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of additional lists
    """
    df = pd.read_csv(data / "lists.csv", index_col="Name")
    return lists_schema.validate(df)


# A list containing the company lists
TARGET_LISTS = get_lists().index.to_list()

# Structure of the dataframe to validate the company_to_lists table
company_to_list_schema = pa.DataFrameSchema(
    columns={
        "List name": pa.Column(
            pd.StringDtype,
            checks=pa.Check(lambda x: x.isin(TARGET_LISTS)),
        )
    },
    coerce=True,
    strict=True,
    index=pa.MultiIndex(
        [
            pa.Index(
                pd.StringDtype,
                checks=pa.Check(lambda x: x.isin(TARGET_LISTS)),
                name="List name",
            ),
            pa.Index(
                pd.StringDtype,
                checks=pa.Check(lambda x: x.isin(COMPANIES)),
                name="Company name",
            ),
        ]
    ),
)


def get_company_to_list() -> pd.DataFrame:
    """Function called to get a DataFrame matching the companies and the lists they are included in

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of companies and lists
    """
    df = pd.read_csv(
        data / "company_to_list.csv", index_col=["List name", "Company name"]
    )
    df["List name"] = df.index.get_level_values("List name")
    return company_to_list_schema.validate(df)


# Structure of the dataframe to validate the markets table
markets_schema = pa.DataFrameSchema(
    columns={},
    coerce=True,
    strict=True,
    index=pa.Index(pd.StringDtype, unique=True, name="Name"),
)


def get_markets() -> pd.DataFrame:
    """Function called to get a DataFrame of the possible markets

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of markets
    """
    df = pd.read_csv(data / "markets.csv", index_col="Name")
    return markets_schema.validate(df)


# A list containing the markets
MARKETS = get_markets().index.to_list()

# Structure of the dataframe to validate the tickers table
tickers_schema = pa.DataFrameSchema(
    columns={
        "Symbol": pa.Column(
            pd.StringDtype, checks=pa.Check(lambda x: x.str.match("^[A-Z]{2,5}$"))
        )
    },
    coerce=True,
    strict=True,
    index=pa.MultiIndex(
        [
            pa.Index(
                pd.StringDtype,
                checks=pa.Check(lambda x: x.isin(MARKETS)),
                name="Market name",
            ),
            pa.Index(
                pd.StringDtype,
                checks=pa.Check(lambda x: x.isin(COMPANIES)),
                name="Company name",
            ),
        ]
    ),
)


def get_tickers() -> pd.DataFrame:
    """Function called to get a DataFrame matching the companies names and the markets they are included in

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of companies and markets
    """
    df = pd.read_csv(data / "tickers.csv", index_col=["Market name", "Company name"])
    return tickers_schema.validate(df)


def get_companies_data() -> pd.DataFrame:
    """Function called to get a DataFrame containing all the relevant companies data

    Returns
    -------
    pd.DataFrame
        The full DataFrame
    """
    companies = get_companies()
    company_to_list = get_company_to_list()
    tickers = get_tickers()
    additional_buds = get_companies_additional_buds()
    additional_regexs = get_companies_additional_regexs()

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


def get_target_companies(target_lists: list | str) -> pd.DataFrame:
    """Function called to get a DataFrame containing the data of all the companies
    included in a certain list

    Parameters
    ----------
    target_lists : list | str
        The required list name (or lists names)

    Returns
    -------
    pd.DataFrame
        The DataFrame with the required data
    """
    if isinstance(target_lists, str):
        target_lists = [target_lists]
    df = get_companies_data()
    filtered_df = df[
        df["List names"].apply(
            lambda x: any(list_name in x for list_name in target_lists)
        )
    ]
    filtered_df = filtered_df.drop(columns=["List names"])
    return filtered_df
