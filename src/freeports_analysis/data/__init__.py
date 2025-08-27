from pathlib import Path
import datetime
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.i18n import _

data = Path(__file__).parent


companies_schema = pa.DataFrameSchema(
    columns={
        "Regexp": pa.Column(pd.StringDtype, nullable=True),
    },
    # Creazione dell'indice composto durante la validazione
    coerce=True,  # Permette trasformazioni
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        name="Name",
        unique=True,
    ),
)


def get_companies():
    df = pd.read_csv(data / "companies.csv", index_col="Name")
    return companies_schema.validate(df)


COMPANIES = get_companies().index.to_list()

lists_schema = pa.DataFrameSchema(
    columns={
        "Institution": pa.Column(pd.StringDtype),
        "Date": pa.Column(datetime.date),
    },
    coerce=True,
    strict=True,
    index=pa.Index(pd.StringDtype, name="Name", unique=True),
)


def get_lists():
    df = pd.read_csv(data / "lists.csv", index_col="Name")
    return lists_schema.validate(df)


TARGET_LISTS = get_lists().index.to_list()

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


def get_company_to_list():
    df = pd.read_csv(
        data / "company_to_list.csv", index_col=["List name", "Company name"]
    )
    df["List name"] = df.index.get_level_values("List name")
    return company_to_list_schema.validate(df)


markets_schema = pa.DataFrameSchema(
    columns={},
    coerce=True,
    strict=True,
    index=pa.Index(pd.StringDtype, unique=True, name="Name"),
)


def get_markets():
    df = pd.read_csv(data / "markets.csv", index_col="Name")
    return markets_schema.validate(df)


MARKETS = get_markets().index.to_list()


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


def get_tickers():
    df = pd.read_csv(data / "tickers.csv", index_col=["Market name", "Company name"])
    return tickers_schema.validate(df)


def get_companies_data():
    companies = get_companies()
    company_to_list = get_company_to_list()
    tickers = get_tickers()

    company_to_lists_agg = company_to_list.groupby(level="Company name").agg(
        {"List name": list}
    )
    tickers_agg = tickers.groupby(level="Company name").agg({"Symbol": list})
    results = companies.join(
        company_to_lists_agg, how="left", validate="one_to_one"
    ).join(tickers_agg, how="left", validate="one_to_one")
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


def get_target_companies(target_lists):
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
