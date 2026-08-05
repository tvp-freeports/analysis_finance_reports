"""Data management for PDF format definitions and URL mappings.

This module handles the loading and validation of format definitions and
URL-to-format mappings used in document processing.
"""

from pathlib import Path
from typing import Optional, List
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


def get_formats(formats_repo_dir: Path) -> pd.DataFrame:
    """Load and validate the list of formats from formats.csv.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.

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
    return df


# Schema for validating URL mappings
_url_mapping_schema = pa.DataFrameSchema(
    {"Url": pa.Column(str)},
    coerce=True,
    strict=True,
    index=pa.Index(pd.StringDtype, name="Format name"),
)


def get_url_mapping_schema(format_names: List[str]) -> pa.DataFrameSchema:
    """Build a pandera schema for URL mapping validation.

    Parameters
    ----------
    format_names : List[str]
        List of valid format names.

    Returns
    -------
    pa.DataFrameSchema
        Pandera schema for URL mapping validation.
    """
    schema = _url_mapping_schema.copy()
    schema.index.checks.append(pa.Check.isin(format_names))
    return schema


def _get_url_mapping(formats_repo_dir: Path, format_names: List[str]) -> pd.DataFrame:
    """Load and validate URL mappings from url_mapping.csv.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.
    format_names : List[str]
        List of valid format names.

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
    return get_url_mapping_schema(format_names).validate(df)


def get_url_mapping(formats_repo_dir: Path, format_names: List[str]) -> pd.DataFrame:
    """Get URL mappings grouped by format name.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.
    format_names : List[str]
        List of valid format names.

    Returns
    -------
    pd.DataFrame
        DataFrame with format names as index and lists of URLs as values

    Notes
    -----
    The returned DataFrame aggregates all URLs associated with each format
    name into lists, allowing multiple URLs to map to the same format.
    """
    return (
        _get_url_mapping(formats_repo_dir, format_names)
        .groupby(level="Format name")
        .agg({"Url": list})
    )


def url_to_format(
    formats_repo_dir: Path, format_names: List[str], url: str
) -> Optional[str]:
    """Associate a URL with a format name.

    Parameters
    ----------
    formats_repo_dir : Path
        Path to the formats repository directory.
    format_names : List[str]
        List of valid format names.
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
    mapping = _get_url_mapping(formats_repo_dir, format_names)
    mask = mapping["Url"].apply(lambda x: str(url).startswith(x))
    detected_format = mapping[mask]["Url"].str.len().idxmax() if mask.any() else None
    return detected_format
