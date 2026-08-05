"""Data management for PDF format definitions and URL mappings.

This module handles the loading and validation of format definitions and
URL-to-format mappings used in document processing.
"""

from pathlib import Path
from typing import Any, Optional, List
import pandera.pandas as pa
import pandas as pd
from freeports.i18n import _


METADATA_DIR = "metadata"


FORMAT_NAME_REGEXP = r".+\-[A-Z]{2}\d{2}(@[A-Z]{2,3})?(\.[^\.\/]+)?"

# Schema for validating the list of formats
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
                lambda x: x.str.match(f"^{FORMAT_NAME_REGEXP}$"),
                error="Format index not valid",
            )
        ],
        unique=True,
    ),
)


def get_formats(
    formats_repo_dir: Path, format_repo_validation_data: Any
) -> pd.DataFrame:
    """Load and validate the list of formats from formats.csv.

    Returns
    -------
    pd.DataFrame
        Validated DataFrame of formats with 'Format name' as index

    Raises
    ------
    pa.errors.SchemaError
        If the format data does not conform to the expected schema

    Notes
    -----
    Format names are constructed as: Name-LocaleYear[Country][Version]
    For example: 'Amundi-IT23' or 'Eurizon-IT24@IT.v2'
    """
    df = pd.read_csv(formats_repo_dir / METADATA_DIR / "formats.csv")
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
    df = formats_schema.validate(df)
    format_repo_validation_data.formats = df.index.to_list()
    return df


# Schema for validating URL mappings
_url_mapping_schema = pa.DataFrameSchema(
    {"Url": pa.Column(str)},
    coerce=True,
    strict=True,
    index=pa.Index(pd.StringDtype, name="Format name"),
)


def get_url_mapping_schema(format_repo_validation_data: Any) -> pa.DataFrameSchema:
    _url_mapping_schema.index.checks.append(
        pa.Check.isin(format_repo_validation_data.formats)
    )
    return _url_mapping_schema


def _get_url_mapping(
    formats_repo_dir: Path, format_repo_validation_data: Any
) -> pd.DataFrame:
    """Load and validate URL mappings from url_mapping.csv.

    Returns
    -------
    pd.DataFrame
        DataFrame of format names and URLs with 'Format name' as index

    Raises
    ------
    pa.errors.SchemaError
        If the URL mapping data does not conform to the expected schema
    """
    df = pd.read_csv(
        formats_repo_dir / METADATA_DIR / "url_mapping.csv", index_col=["Format name"]
    )
    return get_url_mapping_schema(format_repo_validation_data).validate(df)


def get_url_mapping(
    formats_repo_dir: Path, format_repo_validation_data: Any
) -> pd.DataFrame:
    """Get URL mappings grouped by format name.

    Returns
    -------
    pd.DataFrame
        DataFrame with format names as index and lists of URLs as values

    Notes
    -----
    The returned DataFrame aggregates all URLs associated with each format
    name into lists, allowing multiple URLs to map to the same format.
    """
    get_formats(formats_repo_dir, format_repo_validation_data)
    return (
        _get_url_mapping(formats_repo_dir, format_repo_validation_data)
        .groupby(level="Format name")
        .agg({"Url": list})
    )


def url_to_format(
    formats_repo_dir: Path, format_repo_validation_data: Any, url: str
) -> Optional[str]:
    """Associate a URL with a format name.

    Parameters
    ----------
    url : str
        URL to match against known format URLs

    Returns
    -------
    Optional[str]
        Format name if a match is found, None otherwise

    Notes
    -----
    This function uses prefix matching to determine the format - it selects
    the format with the longest matching URL prefix. This allows for more
    specific URLs to override more general ones.
    """
    mapping = _get_url_mapping(formats_repo_dir, format_repo_validation_data)
    mask = mapping["Url"].apply(lambda x: str(url).startswith(x))
    detected_format = mapping[mask]["Url"].str.len().idxmax() if mask.any() else None
    return detected_format
