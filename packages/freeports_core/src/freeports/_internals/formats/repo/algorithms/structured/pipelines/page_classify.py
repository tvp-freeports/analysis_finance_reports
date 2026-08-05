"""Structured algorithm pipeline management.

This module handles the loading and configuration of structured
PDF processing algorithms for formats with well-defined layouts
and consistent data structures.
"""

from pathlib import Path
from typing import Dict, List, Tuple, Any, Callable
import pandera.pandas as pa
import pandas as pd
from freeports._internals.formats.utils.pdf_extract.pdf_blks_acquire import (
    LINE_SET_REGEXP,
)
from freeports._internals.formats.utils.pdf_extract.standard_funcs import (
    PdfExtractPageClassifyStandard,
)
from freeports._internals.formats.utils.text_filter.standard_funcs import (
    TextFilterPageClassifyStandard,
)
from freeports._internals.formats.utils.deserialize.standard_funcs import (
    DeserializerPageClassifyStandard,
)
from freeports._internals.formats.utils.pdf_extract.pdf_blks_acquire import (
    pdfline_selection_from_str,
)
from freeports._internals.formats.utils.pdf_extract.position import (
    TablePosAlgorithm,
)
from freeports._internals.formats.repo.algorithms.pipelines_definition import (
    FKRelation,
    create_index_format_name_pipe,
    column_id_format_pipe,
    index_format_pipe,
    PdfExtractSegment,
    TextFilterSegment,
    DeserializeSegment,
    Pipeline,
)

from ..commons import column_line_set, ALGORITHMS_DIR

pipeline_default = ""

PAGE_CLASSIFY_DIR = ALGORITHMS_DIR / "structured" / "page_classify"

args_schema = pa.DataFrameSchema(
    {
        "ID": column_id_format_pipe(FKRelation.ONE_TO_MANY),
        "Header set": column_line_set,
        "Class": pa.Column(pd.StringDtype),
    },
    strict=True,
    coerce=True,
    index=index_format_pipe(),
)


def get_args(formats_repo_dir) -> pd.DataFrame:
    """Gets and validates the args table

    Returns
    -------
    pd.DataFrame
        Validated DataFrame
    """
    df = pd.read_csv(formats_repo_dir / PAGE_CLASSIFY_DIR / "args.csv")
    df = create_index_format_name_pipe(
        df,
        pipeline_default=pipeline_default,
        relation_to_principal=FKRelation.ONE_TO_MANY,
    )
    return args_schema.validate(df)


def get_structured_formats(formats_repo_dir) -> pd.DataFrame:
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
    args = get_args(formats_repo_dir).drop(columns="ID")

    def aggregate_classes(classes):
        res = set(classes)
        if len(res) != 1:
            raise Exception("Same computed ID require same class")
        return list(res)[0]

    args_agg = args.groupby(
        by=["Format name", "Pipeline name", "Pipe index", "Computed ID"]
    ).agg({"Class": aggregate_classes, "Header set": list})
    result = args_agg.rename(columns={"Header set": "Header sets"})
    return result


def get_pipelines(
    formats_repo_dir, format_name: str
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
        selected_row = get_structured_formats(formats_repo_dir).loc[format_name]
        args = [
            (idx[0] if not pd.isna(idx[0]) else "", row)
            for idx, row in selected_row.iterrows()
        ]
    except KeyError:
        pass
    pipelines = {}
    for pipeline_name, arg in args:
        if pipeline_name not in pipelines:
            pipelines[pipeline_name] = Pipeline(
                pdf_extract=PdfExtractPageClassifyStandard(
                    header_sets=[
                        pdfline_selection_from_str(s) for s in arg["Header sets"]
                    ],
                    page_type=arg["Class"],
                ),
                text_filter=TextFilterPageClassifyStandard(),
                deserialize=DeserializerPageClassifyStandard(),
            )
        else:
            pipelines[pipeline_name].pdf_extract.add_pipe(
                PdfExtractPageClassifyStandard(
                    header_sets=[
                        pdfline_selection_from_str(s) for s in arg["Header sets"]
                    ],
                    page_type=arg["Class"],
                )
            )

    return pipelines
