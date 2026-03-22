"""Structured algorithm pipeline management.

This module handles the loading and configuration of structured
PDF processing algorithms for formats with well-defined layouts
and consistent data structures.
"""

from pathlib import Path
from typing import Dict, List, Tuple, Any, Callable
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import LINE_SET_REGEXP
from freeports_analysis.formats.utils.pdf_extract import PdfExtractFundStandard
from freeports_analysis.formats.utils.deserialize import DeserializerFundStandard
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    pdfline_selection_from_str,
)
from freeports_analysis.formats.utils.pdf_extract.select_position import (
    TablePosAlgorithm,
)
from freeports_analysis.formats.algorithms.commons import (
    FKRelation,
    create_index_format_name_pipe,
    column_id_format_pipe,
    index_format_pipe,
    PdfExtractSegment,
    TextFilterSegment,
    DeserializeSegment,
    Pipeline,
)

from ..commons import column_line_set

data = Path(__file__).parent
pipeline_default = "investments"

args_schema = pa.DataFrameSchema(
    {
        "ID": column_id_format_pipe(FKRelation.ONE_TO_MANY),
        "Fund set": column_line_set,
    },
    strict=True,
    coerce=True,
    index=index_format_pipe(),
)


def get_args() -> pd.DataFrame:
    """Gets and validates the args table

    Returns
    -------
    pd.DataFrame
        Validated DataFrame
    """
    df = pd.read_csv(data / "args.csv")
    pd.set_option("future.no_silent_downcasting", True)
    df = create_index_format_name_pipe(
        df,
        pipeline_default=pipeline_default,
        relation_to_principal=FKRelation.ONE_TO_MANY,
    )
    return args_schema.validate(df)


def get_structured_formats() -> pd.DataFrame:
    """Get complete structured formats configuration with all parameters.

    Returns
    -------
    pd.DataFrame
        DataFrame containing all structured format configurations

    Notes
    -----
    This function combines multiple configuration tables into a single
    comprehensive DataFrame with all parameters needed for structured
    PDF processing algorithms.
    """
    args = get_args().drop(columns="ID")

    return args


def get_pipelines(
    format_name: str,
) -> Tuple[
    Dict[str, List[Callable]], Dict[str, List[Callable]], Dict[str, List[Callable]]
]:
    """Get processing pipelines for a specific structured format.

    Parameters
    ----------
    format_name : str
        Name of the format to get pipelines for

    Returns
    -------
    Tuple[Dict[str, List[Callable]], Dict[str, List[Callable]], Dict[str, List[Callable]]]
        Tuple containing three dictionaries for pdf_extract, text_filter, and deserialize segments.
        Each dictionary maps pipeline names to lists of processing functions.

    Notes
    -----
    Returns empty dictionaries if the format name is not found in the mapping.
    """
    args: List[Tuple[str, pd.Series]] = []
    try:
        selected_row = get_structured_formats().loc[format_name]
        args = [
            (idx[0] if not pd.isna(idx[0]) else "", row)
            for idx, row in selected_row.iterrows()
        ]
    except KeyError:
        pass
    pipelines = {}
    for pipeline_name, arg in args:
        pipelines[pipeline_name] = Pipeline(
            pdf_extract=PdfExtractFundStandard(
                selection=pdfline_selection_from_str(arg["Fund set"])
            ),
            deserialize=DeserializerFundStandard(),
        )

    return pipelines
