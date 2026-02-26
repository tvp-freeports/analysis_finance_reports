"""Structured algorithm pipeline management.

This module handles the loading and configuration of structured
PDF processing algorithms for formats with well-defined layouts
and consistent data structures.
"""

from pathlib import Path
from typing import Dict, List, Tuple, Any, Callable
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import LINE_SET_REGEXP
from freeports_analysis.formats.utils.pdf_filter import StandardInvestmentsPdfFilter
from freeports_analysis.formats.utils.text_extract import standard_text_extraction
from freeports_analysis.formats.utils.deserialize import standard_deserialization
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import (
    pdfline_selection_from_str,
)
from freeports_analysis.formats.utils.pdf_filter.select_position import (
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
)

from .. import column_line_set

data = Path(__file__).parent
pipeline_default = data.name

args_schema = pa.DataFrameSchema(
    {
        "ID": column_id_format_pipe(FKRelation.ONE_TO_ONE),
        "Subfund set": column_line_set,
        "Currency set": column_line_set,
        "Body set": column_line_set,
        "Market value": pa.Column(pd.Int16Dtype, nullable=True),
        "Quantity": pa.Column(pd.Int16Dtype, nullable=True),
        "% net assets": pa.Column(pd.Int16Dtype, nullable=True),
        "Acquisition cost": pa.Column(pd.Int16Dtype, nullable=True),
        "Acquisition currency": pa.Column(pd.Int16Dtype, nullable=True),
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
    df = create_index_format_name_pipe(
        df,
        pipeline_default=pipeline_default,
        relation_to_principal=FKRelation.ONE_TO_ONE,
    )
    return args_schema.validate(df)


VALID_ALGORITHM_ID = get_args().index.get_level_values("Computed ID").to_list()


additional_args_schema = pa.DataFrameSchema(
    {
        "ID": column_id_format_pipe(FKRelation.ONE_TO_MAYBE),
        "Algorithm flags": pa.Column(pd.StringDtype, nullable=True),
        "Tolerance": pa.Column(pd.Float32Dtype, nullable=True),
        "Interpret quantity as float": pa.Column(pd.BooleanDtype, nullable=True),
        "Interpret cost and value as int": pa.Column(pd.BooleanDtype, nullable=True),
        "Geometrical indexing": pa.Column(pd.BooleanDtype, nullable=True),
        "Merge previous": pa.Column(pd.BooleanDtype, nullable=True),
    },
    coerce=True,
    strict=True,
    index=index_format_pipe(VALID_ALGORITHM_ID),
)


def get_additional_args() -> pd.DataFrame:
    """Gets and validates the additional args table

    Returns
    -------
    pd.DataFrame
        Validated DataFrame
    """
    df = pd.read_csv(data / "additional_args.csv")
    df = create_index_format_name_pipe(
        df,
        pipeline_default=pipeline_default,
        relation_to_principal=FKRelation.ONE_TO_MAYBE,
    )
    return additional_args_schema.validate(df)


# additional_headers_schema = pa.DataFrameSchema(
#     {"Header set": column_line_set}, coerce=True, strict=True, index=None
# )


# def get_additional_headers() -> pd.DataFrame:
#     """Gets and validates the additional headers table

#     Returns
#     -------
#     pd.DataFrame
#         Validated DataFrame
#     """
#     df = pd.read_csv(data / "additional_headers.csv", index_col=["ID"])
#     return additional_headers_schema.validate(df)


deselection_list_schema = pa.DataFrameSchema(
    {
        "ID": column_id_format_pipe(FKRelation.ONE_TO_MANY),
        "Deselection set": column_line_set,
    },
    coerce=True,
    strict=True,
    index=index_format_pipe(VALID_ALGORITHM_ID),
)


def get_deselection_lists() -> pd.DataFrame:
    """Gets and validates the deselection list table

    Returns
    -------
    pd.DataFrame
        Validated DataFrame
    """
    df = pd.read_csv(data / "deselection_lists.csv")
    df = create_index_format_name_pipe(
        df,
        pipeline_default=pipeline_default,
        relation_to_principal=FKRelation.ONE_TO_MANY,
    )
    return deselection_list_schema.validate(df)


partial_pipes_schema = pa.DataFrameSchema(
    {
        "ID": column_id_format_pipe(FKRelation.ONE_TO_MAYBE),
        "pdf_filter": pa.Column(pd.BooleanDtype),
        "text_extract": pa.Column(pd.BooleanDtype),
        "deserialize": pa.Column(pd.BooleanDtype),
    },
    coerce=True,
    strict=True,
    index=index_format_pipe(VALID_ALGORITHM_ID),
)


def get_partial_pipes() -> pd.DataFrame:
    """Gets and validates the partial pipes table

    Returns
    -------
    pd.DataFrame
        Validated DataFrame
    """
    df = pd.read_csv(data / "partial_pipes.csv")
    df = create_index_format_name_pipe(df, pipeline_default, FKRelation.ONE_TO_MAYBE)
    return partial_pipes_schema.validate(df)


def validate_partial_pipes(
    segment: str, columns: List[str]
) -> Callable[[pd.DataFrame], pd.Series]:
    """Create a validation function for partial pipeline configurations.

    This function generates a validator that ensures when a pipeline segment
    is disabled, the corresponding configuration columns are also empty.

    Parameters
    ----------
    segment : str
        Name of the pipeline segment ('pdf_filter', 'text_extract', or 'deserialize')
    columns : List[str]
        List of column names that should be empty when the segment is disabled

    Returns
    -------
    Callable[[pd.DataFrame], pd.Series]
        Validation function that returns a boolean Series indicating valid rows
    """

    def validate_columns(args: pd.DataFrame) -> pd.Series:
        """Validate that disabled segments don't have associated configuration."""
        columns_not_empty = False
        pd.set_option("future.no_silent_downcasting", True)
        pipe_present = args[segment].fillna(True, inplace=False)
        for col in columns:
            columns_not_empty = columns_not_empty | ~args[col].isna()
        invalid_mask = ~pipe_present & columns_not_empty
        return ~invalid_mask

    return validate_columns


structured_formats_schema = pa.DataFrameSchema(
    checks=[
        pa.Check(
            validate_partial_pipes(
                "pdf_filter",
                [
                    "Subfund set",
                    "Currency set",
                    "Body set",
                    "Deselection set",
                    "Algorithm flags",
                    "Tolerance",
                ],
            )
        ),
        pa.Check(
            validate_partial_pipes(
                "text_extract",
                [
                    "Market value",
                    "Quantity",
                    "% net assets",
                    "Acquisition cost",
                    "Acquisition currency",
                    "Geometrical indexing",
                    "Merge previous",
                ],
            )
        ),
        pa.Check(
            validate_partial_pipes(
                "deserialize",
                ["Interpret quantity as float", "Interpret cost and value as int"],
            )
        ),
    ]
)


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
    add_args = get_additional_args().drop(columns="ID")
    deselection_list = get_deselection_lists().drop(columns="ID")
    partial_pipes = get_partial_pipes().drop(columns="ID")
    deselection_list_agg = deselection_list.groupby(
        by=["Format name", "Pipeline name", "Pipe index", "Computed ID"]
    ).agg({"Deselection set": list})
    result = (
        args.join(add_args, how="left", validate="one_to_one")
        .join(deselection_list_agg, how="left", validate="one_to_one")
        .join(partial_pipes, how="left", validate="one_to_one")
    )

    return structured_formats_schema.validate(result)


def get_pipes(
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
        Tuple containing three dictionaries for pdf_filter, text_extract, and deserialize segments.
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
    pdf_filter_segment: Dict[str, List[Callable]] = {}
    text_extract_segment: Dict[str, List[Callable]] = {}
    deserialize_segment: Dict[str, List[Callable]] = {}

    for pipeline, arg in args:

        def _set_if_not_na(func_arg_dict, key, args, key_value):
            if not pd.isna(args[key_value]):
                func_arg_dict[key] = args[key_value]
            return func_arg_dict

        # PDF Filter segment
        if pd.isna(arg["pdf_filter"]) or arg["pdf_filter"]:
            pdf_filter_args = {
                "header_set": [
                    pdfline_selection_from_str(s) for s in arg["Header sets"]
                ],
                "subfund_set": pdfline_selection_from_str(arg["Subfund set"]),
                "body_set": pdfline_selection_from_str(arg["Body set"]),
                "currency_set": pdfline_selection_from_str(arg["Currency set"]),
            }
            if isinstance(arg["Deselection set"], list):
                pdf_filter_args["deselection_list"] = [
                    pdfline_selection_from_str(s) for s in arg["Deselection set"]
                ]
            if not pd.isna(arg["Algorithm flags"]):
                pdf_filter_args["algorithm_flags"] = TablePosAlgorithm.from_dict(
                    arg["Algorithm flags"]
                )
            pdf_filter_args = _set_if_not_na(
                pdf_filter_args, "tolerance", arg, "Tolerance"
            )
            pdf_filter = StandardInvestmentsPdfFilter(**pdf_filter_args)
            if pipeline not in pdf_filter_segment:
                pdf_filter_segment[pipeline] = PdfExtractSegment()
            pdf_filter_segment[pipeline].add_pipe(pdf_filter)

        # Text Extract segment
        if pd.isna(arg["text_extract"]) or arg["text_extract"]:
            text_extract_args = {"market_value_pos": arg["Market value"]}
            text_extract_args = _set_if_not_na(
                text_extract_args, "geometrical_indexes", arg, "Geometrical indexing"
            )
            text_extract_args = _set_if_not_na(
                text_extract_args, "merge_prev", arg, "Merge previous"
            )
            text_extract_args = _set_if_not_na(
                text_extract_args, "nominal_quantity_pos", arg, "Quantity"
            )
            text_extract_args = _set_if_not_na(
                text_extract_args, "perc_net_assets_pos", arg, "% net assets"
            )
            text_extract_args = _set_if_not_na(
                text_extract_args,
                "acquisition_currency_pos",
                arg,
                "Acquisition currency",
            )
            text_extract_args = _set_if_not_na(
                text_extract_args, "acquisition_cost_pos", arg, "Acquisition cost"
            )
            text_extract = standard_text_extraction(**text_extract_args)(
                lambda blks, targets: None
            )
            if pipeline not in text_extract_segment:
                text_extract_segment[pipeline] = TextFilterSegment()
            text_extract_segment[pipeline].add_pipe(text_extract)

        # Deserialize segment
        if pd.isna(arg["deserialize"]) or arg["deserialize"]:
            deserialize_args = {}
            deserialize_args = _set_if_not_na(
                deserialize_args,
                "quantity_interpret_float",
                arg,
                "Interpret quantity as float",
            )
            deserialize_args = _set_if_not_na(
                deserialize_args,
                "cost_and_value_interpret_int",
                arg,
                "Interpret cost and value as int",
            )
            deserialize = standard_deserialization(**deserialize_args)(
                lambda blk, targets: None
            )
            if pipeline not in deserialize_segment:
                deserialize_segment[pipeline] = DeserializeSegment()
            deserialize_segment[pipeline].add_pipe(deserialize)
    return pdf_filter_segment, text_extract_segment, deserialize_segment
