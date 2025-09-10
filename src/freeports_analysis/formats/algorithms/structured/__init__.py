from pathlib import Path
import pandera.pandas as pa
import pandas as pd
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import line_set_regexp
from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.text_extract import standard_text_extraction
from freeports_analysis.formats.utils.deserialize import standard_deserialization
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.formats.utils.pdf_filter.select_position import (
    TablePosAlgorithm,
)
from ..commons import create_index_format_name_pipe, index_format_pipe

data = Path(__file__).parent


column_line_set = pa.Column(
    pd.StringDtype,
    checks=[
        pa.Check(lambda x: x.str.match(f"^{line_set_regexp}$")),
    ],
    nullable=True,
)
args_schema = pa.DataFrameSchema(
    {
        "Header set": column_line_set,
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
    index=index_format_pipe,
)


def get_args():
    df = pd.read_csv(data / "args.csv")
    df = create_index_format_name_pipe(df)
    return args_schema.validate(df)


VALID_ALGORITHM_ID = get_args().index.get_level_values("ID").to_list()

id_index = index = pa.Index(
    pd.StringDtype, checks=[pa.Check(lambda x: x.isin(VALID_ALGORITHM_ID))], name="ID"
)


additional_args_schema = pa.DataFrameSchema(
    {
        "Algorithm flags": pa.Column(pd.StringDtype, nullable=True),
        "Tolerance": pa.Column(pd.Float32Dtype, nullable=True),
        "Interpret quantity as float": pa.Column(pd.BooleanDtype, nullable=True),
        "Interpret cost and value as int": pa.Column(pd.BooleanDtype, nullable=True),
        "Geometrical indexing": pa.Column(pd.BooleanDtype, nullable=True),
        "Merge previous": pa.Column(pd.BooleanDtype, nullable=True),
    },
    coerce=True,
    strict=True,
    index=id_index,
)


def get_additional_args():
    df = pd.read_csv(data / "additional_args.csv", index_col=["ID"])
    return additional_args_schema.validate(df)


additional_headers_schema = pa.DataFrameSchema(
    {"Header set": column_line_set}, coerce=True, strict=True, index=id_index
)


def get_additional_headers():
    df = pd.read_csv(data / "additional_headers.csv", index_col=["ID"])
    return additional_headers_schema.validate(df)


deselection_list_schema = pa.DataFrameSchema(
    {"Deselection set": column_line_set}, coerce=True, strict=True, index=id_index
)


def get_deselection_lists():
    df = pd.read_csv(data / "deselection_lists.csv", index_col=["ID"])
    return deselection_list_schema.validate(df)


partial_pipes_schema = pa.DataFrameSchema(
    {
        "pdf_filter": pa.Column(pd.BooleanDtype),
        "text_extract": pa.Column(pd.BooleanDtype),
        "deserialize": pa.Column(pd.BooleanDtype),
    },
    coerce=True,
    strict=True,
    index=id_index,
)


def get_partial_pipes():
    df = pd.read_csv(data / "partial_pipes.csv", index_col=["ID"])
    return partial_pipes_schema.validate(df)


def validate_partial_pipes(segment, columns):
    def validate_columns(args):
        columns_not_empty = False
        for col in columns:
            columns_not_empty = columns_not_empty | ~args[col].isna()
        invalid_mask = (~args[segment].isna() & ~args[segment]) & columns_not_empty
        return ~invalid_mask

    return validate_columns


structured_formats_schema = pa.DataFrameSchema(
    checks=[
        pa.Check(
            validate_partial_pipes(
                "pdf_filter",
                [
                    "Header sets",
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


def get_structured_formats():
    args = get_args()
    add_args = get_additional_args()
    add_headers = get_additional_headers()
    deselection_list = get_deselection_lists()
    partial_pipes = get_partial_pipes()
    deselection_list_agg = deselection_list.groupby(by="ID").agg(
        {"Deselection set": list}
    )
    add_headers_agg = add_headers.groupby(by="ID").agg({"Header set": list})
    result = (
        args.join(add_args, how="left", validate="one_to_one")
        .join(deselection_list_agg, how="left", validate="one_to_one")
        .join(
            add_headers_agg, how="left", validate="one_to_one", rsuffix="s additional"
        )
        .join(partial_pipes, how="left", validate="one_to_one")
    )
    result["Header sets additional"] = [
        x if isinstance(x, list) else [] for x in result["Header sets additional"]
    ]
    result["Header sets"] = [
        [main] + add if not pd.isna(main) else pd.NA
        for main, add in zip(result["Header set"], result["Header sets additional"])
    ]
    result.drop(columns=["Header set", "Header sets additional"], inplace=True)
    return structured_formats_schema.validate(result)


def get_pipes(format_name):
    args = []
    try:
        selected_row = get_structured_formats().loc[format_name]
        args = [
            (idx[0] if not pd.isna(idx[0]) else "", row)
            for idx, row in selected_row.iterrows()
        ]
    except KeyError:
        pass
    pdf_filter_segment = {}
    text_extract_segment = {}
    deserialize_segment = {}
    for pipeline, arg in args:
        if pd.isna(arg["pdf_filter"]) or arg["pdf_filter"]:
            pdf_filter_args = {
                "header_set": [PdfLineSet.from_str(s) for s in arg["Header sets"]],
                "subfund_set": PdfLineSet.from_str(arg["Subfund set"]),
                "body_set": PdfLineSet.from_str(arg["Body set"]),
                "currency_set": PdfLineSet.from_str(arg["Currency set"]),
            }
            if isinstance(arg["Deselection set"], list):
                pdf_filter_args["deselection_list"] = [
                    PdfLineSet.from_str(s) for s in arg["Deselection set"]
                ]
            if not pd.isna(arg["Algorithm flags"]):
                pdf_filter_args["algorithm_flags"] = TablePosAlgorithm.from_dict(
                    arg["Algorithm flags"]
                )
            if not pd.isna(arg["Tolerance"]):
                pdf_filter_args["tolerance"] = arg["Tolerance"]
            pdf_filter = standard_pdf_filtering(**pdf_filter_args)(
                lambda xml_root: None
            )
            if pipeline not in pdf_filter_segment:
                pdf_filter_segment[pipeline] = []
            pdf_filter_segment[pipeline].append(pdf_filter)

        if pd.isna(arg["text_extract"]) or arg["text_extract"]:
            text_extract_args = {"market_value_pos": arg["Market value"]}
            if not pd.isna(arg["Geometrical indexing"]):
                text_extract_args["geometrical_indexes"] = arg["Geometrical indexing"]
            if not pd.isna(arg["Merge previous"]):
                text_extract_args["merge_prev"] = arg["Merge previous"]
            if not pd.isna(arg["Quantity"]):
                text_extract_args["nominal_quantity_pos"] = arg["Quantity"]
            if not pd.isna(arg["% net assets"]):
                text_extract_args["perc_net_assets_pos"] = arg["% net assets"]
            if not pd.isna(arg["Acquisition currency"]):
                text_extract_args["acquisition_currency_pos"] = arg[
                    "Acquisition currency"
                ]
            if not pd.isna(arg["Acquisition cost"]):
                text_extract_args["acquisition_cost_pos"] = arg["Acquisition cost"]

            text_extract = standard_text_extraction(**text_extract_args)(
                lambda blks, targets: None
            )
            if pipeline not in text_extract_segment:
                text_extract_segment[pipeline] = []
            text_extract_segment[pipeline].append(text_extract)

        if pd.isna(arg["deserialize"]) or arg["deserialize"]:
            deserialize_args = {}
            if not pd.isna(arg["Interpret quantity as float"]):
                deserialize_args["quantity_interpret_float"] = arg[
                    "Interpret quantity as float"
                ]
            if not pd.isna(arg["Interpret cost and value as int"]):
                deserialize_args["cost_and_value_interpret_int"] = arg[
                    "Interpret cost and value as int"
                ]

            deserialize = standard_deserialization(**deserialize_args)(
                lambda blk, targets: None
            )
            if pipeline not in deserialize_segment:
                deserialize_segment[pipeline] = []
            deserialize_segment[pipeline].append(deserialize)
    return pdf_filter_segment, text_extract_segment, deserialize_segment
