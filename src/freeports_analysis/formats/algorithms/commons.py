"""Common utilities and data structures for algorithm pipeline management.

This module provides shared functionality for handling format and pipeline
identifiers, including validation schemas and index manipulation utilities.
"""

import pandera.pandas as pa
import pandas as pd
from freeports_analysis.formats.data import FORMAT_NAME_REGEXP, VALID_FORMATS

# Regular expressions for pipeline naming conventions
pipe_name_regexp: str = "[0-9a-z_]*"
pipe_regexp: str = rf"\({pipe_name_regexp}\)"
format_algorithm_id_regexp: str = f"{FORMAT_NAME_REGEXP}({pipe_regexp})?"

# Pandera schema for validating format-pipeline index structure
index_format_pipe: pa.MultiIndex = pa.MultiIndex(
    [
        pa.Index(
            pd.StringDtype,
            [pa.Check(lambda x: x.isin(VALID_FORMATS))],
            name="Format name",
        ),
        pa.Index(
            pd.StringDtype,
            [pa.Check(lambda x: x.str.match(f"^{pipe_name_regexp}$"))],
            name="Pipe name",
            nullable=True,
        ),
        pa.Index(
            pd.StringDtype,
            [pa.Check(lambda x: x.str.match(f"^{format_algorithm_id_regexp}$"))],
            name="ID",
        ),
    ]
)


def add_format_name_index(df: pd.DataFrame) -> pd.DataFrame:
    """Extract format name from ID column and add as separate column.

    Parameters
    ----------
    df : pd.DataFrame
        DataFrame containing algorithm IDs in 'ID' column

    Returns
    -------
    pd.DataFrame
        DataFrame with added 'Format name' column

    Notes
    -----
    The format name is extracted by removing any pipeline suffix from the ID.
    For example, 'Amundi-IT23' becomes 'Amundi-IT23' and 'Amundi-IT23(pipeline1)'
    also becomes 'Amundi-IT23'.
    """
    df = df.assign(
        format_name=lambda x: x["ID"].str.replace(f"{pipe_regexp}$", "", regex=True)
    )
    df.rename(columns={"format_name": "Format name"}, inplace=True)
    return df


def add_pipe_name(df: pd.DataFrame) -> pd.DataFrame:
    """Extract pipe name from ID column and add as separate column.

    Parameters
    ----------
    df : pd.DataFrame
        DataFrame containing algorithm IDs in 'ID' column

    Returns
    -------
    pd.DataFrame
        DataFrame with added 'Pipe name' column

    Notes
    -----
    The pipe name is extracted from the pipeline suffix in parentheses.
    For example, 'Amundi-IT23(pipeline1)' becomes 'pipeline1', while
    'Amundi-IT23' without a pipeline suffix becomes NaN.
    """
    df = df.assign(
        pipe_name=lambda x: x["ID"].str.extract(f"\(({pipe_name_regexp})\)$")
    )
    df.rename(columns={"pipe_name": "Pipe name"}, inplace=True)
    return df


def set_index_format_name_pipe(df: pd.DataFrame) -> pd.DataFrame:
    """Set multi-index using Format name, Pipe name, and ID columns.

    Parameters
    ----------
    df : pd.DataFrame
        DataFrame with 'Format name', 'Pipe name', and 'ID' columns

    Returns
    -------
    pd.DataFrame
        DataFrame with multi-index set to (Format name, Pipe name, ID)

    Notes
    -----
    This creates a hierarchical index that allows efficient lookup of
    algorithms by format name, pipeline name, and algorithm ID.
    """
    return df.set_index(["Format name", "Pipe name", "ID"])


def create_index_format_name_pipe(df: pd.DataFrame) -> pd.DataFrame:
    """Create complete format-pipe-name index from ID column.

    This is a convenience function that combines:
    1. Extracting format name from ID
    2. Extracting pipe name from ID
    3. Setting the multi-index

    Parameters
    ----------
    df : pd.DataFrame
        DataFrame containing algorithm IDs in 'ID' column

    Returns
    -------
    pd.DataFrame
        DataFrame with multi-index set to (Format name, Pipe name, ID)

    Notes
    -----
    This function provides a complete pipeline for converting algorithm IDs
    into a structured multi-index format suitable for algorithm lookup and
    management. It handles both formats with and without pipeline names.
    """
    df = add_format_name_index(df)
    df = add_pipe_name(df)
    return set_index_format_name_pipe(df)
