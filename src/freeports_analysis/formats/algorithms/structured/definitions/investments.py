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
from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractFundStandard,
    PdfExtractCurrencyStandard,
)
from freeports_analysis.formats.utils.text_filter import TextFilterInvestmentsStandard
from freeports_analysis.formats.utils.deserialize import (
    DeserializerInvestmentStandard,
    DeserializerFundStandard,
)
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

from ..commons import column_line_set, ALGORITHMS_DIR


pipeline_default = "investments"

INVESTMENTS_DIR = ALGORITHMS_DIR / "structured" / pipeline_default

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


def get_args(formats_repo_dir) -> pd.DataFrame:
    """Gets and validates the args table

    Returns
    -------
    pd.DataFrame
        Validated DataFrame
    """
    df = pd.read_csv(formats_repo_dir / INVESTMENTS_DIR / "args.csv")
    df = create_index_format_name_pipe(
        df,
        pipeline_default=pipeline_default,
        relation_to_principal=FKRelation.ONE_TO_ONE,
    )
    return args_schema.validate(df)


_additional_args_schema = pa.DataFrameSchema(
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
)


def get_additional_args_schema(valid_algorithm_ids):
    _additional_args_schema.index = index_format_pipe(valid_algorithm_ids)
    return _additional_args_schema


def get_additional_args(formats_repo_dir, valid_algorithm_ids) -> pd.DataFrame:
    """Gets and validates the additional args table

    Returns
    -------
    pd.DataFrame
        Validated DataFrame
    """
    df = pd.read_csv(formats_repo_dir / INVESTMENTS_DIR / "additional_args.csv")
    df = create_index_format_name_pipe(
        df,
        pipeline_default=pipeline_default,
        relation_to_principal=FKRelation.ONE_TO_MAYBE,
    )
    return get_additional_args_schema(valid_algorithm_ids).validate(df)


_deselection_list_schema = pa.DataFrameSchema(
    {
        "ID": column_id_format_pipe(FKRelation.ONE_TO_MANY),
        "Deselection set": column_line_set,
    },
    coerce=True,
    strict=True,
)


def get_deselection_list_schema(valid_algorithm_ids):
    _deselection_list_schema.index = index_format_pipe(valid_algorithm_ids)
    return _deselection_list_schema


def get_deselection_lists(formats_repo_dir, valid_algorithm_ids) -> pd.DataFrame:
    """Gets and validates the deselection list table

    Returns
    -------
    pd.DataFrame
        Validated DataFrame
    """
    df = pd.read_csv(formats_repo_dir / INVESTMENTS_DIR / "deselection_lists.csv")
    df = create_index_format_name_pipe(
        df,
        pipeline_default=pipeline_default,
        relation_to_principal=FKRelation.ONE_TO_MANY,
    )
    return get_deselection_list_schema(valid_algorithm_ids).validate(df)


_partial_pipes_schema = pa.DataFrameSchema(
    {
        "ID": column_id_format_pipe(FKRelation.ONE_TO_MAYBE),
        "pdf_extract": pa.Column(pd.BooleanDtype),
        "text_filter": pa.Column(pd.BooleanDtype),
        "deserialize": pa.Column(pd.BooleanDtype),
    },
    coerce=True,
    strict=True,
)


def get_partial_pipes_schema(valid_algorithm_ids):
    _partial_pipes_schema.index = index_format_pipe(valid_algorithm_ids)
    return _partial_pipes_schema


def get_partial_pipes(formats_repo_dir, valid_algorithm_ids) -> pd.DataFrame:
    """Gets and validates the partial pipes table

    Returns
    -------
    pd.DataFrame
        Validated DataFrame
    """
    df = pd.read_csv(formats_repo_dir / INVESTMENTS_DIR / "partial_pipes.csv")
    df = create_index_format_name_pipe(df, pipeline_default, FKRelation.ONE_TO_MAYBE)
    return get_partial_pipes_schema(valid_algorithm_ids).validate(df)


def validate_partial_pipes(
    segment: str, columns: List[str]
) -> Callable[[pd.DataFrame], pd.Series]:
    """Create a validation function for partial pipeline configurations.

    This function generates a validator that ensures when a pipeline segment
    is disabled, the corresponding configuration columns are also empty.

    Parameters
    ----------
    segment : str
        Name of the pipeline segment ('pdf_extract', 'text_filter', or 'deserialize')
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
                "pdf_extract",
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
                "text_filter",
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
    valid_algorithm_ids = tuple(zip(*args.index.to_list()))[-1]
    add_args = get_additional_args(formats_repo_dir, valid_algorithm_ids).drop(
        columns="ID"
    )
    deselection_list = get_deselection_lists(
        formats_repo_dir, valid_algorithm_ids
    ).drop(columns="ID")
    partial_pipes = get_partial_pipes(formats_repo_dir, valid_algorithm_ids).drop(
        columns="ID"
    )
    deselection_list_agg = deselection_list.groupby(
        by=["Format name", "Pipeline name", "Pipe index", "Computed ID"]
    ).agg({"Deselection set": list})
    result = (
        args.join(add_args, how="left", validate="one_to_one")
        .join(deselection_list_agg, how="left", validate="one_to_one")
        .join(partial_pipes, how="left", validate="one_to_one")
    )
    return structured_formats_schema.validate(result)


def get_pipelines(
    format_name: str, formats_repo_dir
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

    def _set_if_not_na(func_arg_dict, key, args, key_value):
        if not pd.isna(args[key_value]):
            func_arg_dict[key] = args[key_value]
        return func_arg_dict

    for pipeline_name, arg in args:
        if pipeline_name not in pipelines:
            pipelines[pipeline_name] = Pipeline()
        if pd.isna(arg["pdf_extract"]) or arg["pdf_extract"]:
            pdf_extract_args = {
                "body_set": pdfline_selection_from_str(arg["Body set"]),
                "currency_set": pdfline_selection_from_str(arg["Currency set"]),
            }
            if isinstance(arg["Deselection set"], list):
                pdf_extract_args["deselection_list"] = [
                    pdfline_selection_from_str(s) for s in arg["Deselection set"]
                ]
            if not pd.isna(arg["Algorithm flags"]):
                pdf_extract_args["algorithm_flags"] = TablePosAlgorithm.from_dict(
                    arg["Algorithm flags"]
                )
            pdf_extract_args = _set_if_not_na(
                pdf_extract_args, "tolerance", arg, "Tolerance"
            )
            pdf_extract_investments = PdfExtractInvestmentsStandard(**pdf_extract_args)
            pdf_extract_fund = PdfExtractFundStandard(
                selection=pdfline_selection_from_str(arg["Subfund set"])
            )
            pdf_extract_currency = PdfExtractCurrencyStandard(
                selection=pdfline_selection_from_str(arg["Currency set"])
            )
            pipelines[pipeline_name].add_pdf_extract(pdf_extract_investments)
            pipelines[pipeline_name].add_pdf_extract(pdf_extract_fund)
            pipelines[pipeline_name].add_pdf_extract(pdf_extract_currency)

        if pd.isna(arg["text_filter"]) or arg["text_filter"]:
            text_filter_args = {"market_value_pos": arg["Market value"]}
            text_filter_args = _set_if_not_na(
                text_filter_args, "geometrical_indexes", arg, "Geometrical indexing"
            )
            text_filter_args = _set_if_not_na(
                text_filter_args, "merge_prev", arg, "Merge previous"
            )
            text_filter_args = _set_if_not_na(
                text_filter_args, "nominal_quantity_pos", arg, "Quantity"
            )
            text_filter_args = _set_if_not_na(
                text_filter_args, "perc_net_assets_pos", arg, "% net assets"
            )
            text_filter_args = _set_if_not_na(
                text_filter_args,
                "acquisition_currency_pos",
                arg,
                "Acquisition currency",
            )
            text_filter_args = _set_if_not_na(
                text_filter_args, "acquisition_cost_pos", arg, "Acquisition cost"
            )
            text_filter = TextFilterInvestmentsStandard(**text_filter_args)
            pipelines[pipeline_name].add_text_filter(text_filter)
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
            deserialize_investment = DeserializerInvestmentStandard(**deserialize_args)
            deserialize_fund = DeserializerFundStandard()
            pipelines[pipeline_name].add_deserialize(deserialize_investment)
            pipelines[pipeline_name].add_deserialize(deserialize_fund)
    return pipelines
