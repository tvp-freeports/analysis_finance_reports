from pathlib import Path
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.i18n import _
from typing import Optional

data = Path(__file__).parent

format_name_regexp = r".+\-[A-Z]{2}\d{2}(@[A-Z]{2,3})?(\.[^\.]+)?"

# Structure of the dataframe to validate the list of formats everytime it is imported
formats_schema = pa.DataFrameSchema(
    columns={
        "Name": pa.Column(pd.StringDtype),
        "Locale": pa.Column(pd.StringDtype),
        "Year": pa.Column(pd.Int16Dtype),
        "Country": pa.Column(pd.StringDtype, nullable=True),
        "Version": pa.Column(pd.StringDtype, nullable=True),
    },
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        name="Format name",
        checks=[
            pa.Check(
                lambda x: x.str.match(f"^{format_name_regexp}$"),
                error="Format index not valid",
            )
        ],
        unique=True,
    ),
)


def get_formats() -> pd.DataFrame:
    """Function called to get the list of formats contained in formats.csv
    while validating the structure through format_schema

    Returns
    -------
    DataFrame
        Validated DataFrame of formats
    """
    df = pd.read_csv(data / "formats.csv")
    df = df.assign(
        Format_name=lambda x: (
            x["Name"]
            + "-"
            + x["Locale"]
            + x["Year"].astype(str).str[-2:]
            + x["Country"].apply(lambda v: f"@{v}" if pd.notna(v) and v != "" else "")
            + x["Version"].apply(lambda v: f".{v}" if pd.notna(v) and v != "" else "")
        )
    )
    df.rename(columns={"Format_name": "Format name"}, inplace=True)
    df.set_index("Format name", inplace=True)
    return formats_schema.validate(df)


# A list containing the formats
VALID_FORMATS = get_formats().index.tolist()

# Structure of the dataframe to validate the url mapping everytime it is imported
url_mapping_schema = pa.DataFrameSchema(
    {"Url": pa.Column(str)},
    coerce=True,
    strict=True,
    index=pa.Index(
        pd.StringDtype,
        name="Format name",
        checks=[pa.Check.isin(VALID_FORMATS)],
    ),
)


def _get_url_mapping() -> pd.DataFrame:
    """Function that returns a dataframe linking every url to the format name associated

    Returns
    -------
    DataFrame
        DataFrame of format names and urls
    """
    df = pd.read_csv(data / "url_mapping.csv", index_col=["Format name"])
    return url_mapping_schema.validate(df)


def get_url_mapping() -> pd.DataFrame:
    """Function that returns a dataframe linking the unique format names to a list of urls

    Returns
    -------
    DataFrame
        DataFrame of format names and urls
    """
    return _get_url_mapping().groupby(level="Format name").agg({"Url": list})


def url_to_format(url: str) -> Optional[str]:
    """Function used to associate a single url to a single format name

    Parameters
    ----------
    url : str
        string containing the url

    Returns
    -------
    Optional[str]
        format name or None if the url is invalid
    """
    mapping = _get_url_mapping()
    mask = mapping["Url"].apply(lambda x: str(url).startswith(x))
    detected_format = mapping[mask]["Url"].str.len().idxmax() if mask.any() else None
    return detected_format
