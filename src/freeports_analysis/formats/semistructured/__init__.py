from pathlib import Path
import inspect
import yaml
import pandera.pandas as pa
import pandas as pd
import freeports_analysis.formats.semistructured.pdf_filter as p
import freeports_analysis.formats.semistructured.text_extract as t
import freeports_analysis.formats.semistructured.deserialize as d
from ..commons import index_format_pipe, create_index_format_name_pipe

data = Path(__file__).parent


def _get_defined_list(module, condition):
    return [
        fn
        for fn, f in inspect.getmembers(module, condition)
        if f.__module__ == module.__name__
    ]


pdf_filter_funcs = _get_defined_list(p, inspect.isfunction)
pdf_filter_cls = _get_defined_list(p, inspect.isclass)

text_extract_funcs = _get_defined_list(t, inspect.isfunction)
text_extract_cls = _get_defined_list(t, inspect.isclass)

deserialize_funcs = _get_defined_list(d, inspect.isfunction)
deserialize_cls = _get_defined_list(d, inspect.isclass)

algorithms_with_pdf_filter_args = yaml.safe_load(
    (data / "pdf_filter" / "args.yaml").open("r")
).keys()
algorithms_with_text_extract_args = yaml.safe_load(
    (data / "text_extract" / "args.yaml").open("r")
).keys()
algorithms_with_deserialize_args = yaml.safe_load(
    (data / "deserialize" / "args.yaml").open("r")
).keys()

formats_mapping_schema = pa.DataFrameSchema(
    {
        "pdf_filter": pa.Column(
            pd.StringDtype,
            [pa.Check(lambda x: x.isin(pdf_filter_funcs))],
            nullable=True,
        ),
        "InputPdfFilter": pa.Column(
            pd.StringDtype, [pa.Check(lambda x: x.isin(pdf_filter_cls))], nullable=True
        ),
        "text_extract": pa.Column(
            pd.StringDtype,
            [pa.Check(lambda x: x.isin(text_extract_funcs))],
            nullable=True,
        ),
        "InputTextExtract": pa.Column(
            pd.StringDtype,
            [pa.Check(lambda x: x.isin(text_extract_cls))],
            nullable=True,
        ),
        "deserialize": pa.Column(
            pd.StringDtype,
            [pa.Check(lambda x: x.isin(deserialize_funcs))],
            nullable=True,
        ),
        "InputDeserialize": pa.Column(
            pd.StringDtype, [pa.Check(lambda x: x.isin(deserialize_cls))], nullable=True
        ),
    },
    strict=True,
    coerce=True,
    index=index_format_pipe,
)


def get_formats_mapping():
    df = pd.read_csv(data / "formats_mapping.csv")
    df = create_index_format_name_pipe(df)

    def _input_from_func(x):
        return x.where(
            x.isna(), "Input" + x.astype(str).str.title().str.replace("_", "")
        )

    df = df.assign(
        InputPdfFilter=lambda x: _input_from_func(x["pdf_filter"]),
        InputTextExtract=lambda x: _input_from_func(x["text_extract"]),
        InputDeserialize=lambda x: _input_from_func(x["deserialize"]),
    )
    return formats_mapping_schema.validate(df)


VALID_ALGORITHM_ID = get_formats_mapping().index.get_level_values("ID").to_list()


def _get_segment(format_name, segment_name, pipes_mapping):
    segment = {}
    input_segment = "Input" + segment_name.title().replace("_", "")
    args = yaml.safe_load((data / segment_name / "args.yaml").open("r"))
    for pipeline, mapping in pipes_mapping:
        algorithm_id = f"{format_name}({pipeline})"
        if pd.isna(mapping[segment_name]):
            continue
        if pipeline not in segment:
            segment[pipeline] = []
            selected_args = None
            try:
                selected_args = args[algorithm_id]
            except KeyError:
                if pipeline == "":
                    selected_args = args[format_name]
                else:
                    raise
            arg = (
                selected_args[len(segment[pipeline])]
                if isinstance(selected_args, list)
                else selected_args
            )
            selected_func = getattr(p, mapping[segment_name])
            selected_input = getattr(p, mapping[input_segment])
            func = selected_func(selected_input(**arg))(lambda xml_root: None)
            segment[pipeline].append(func)
    return segment


def get_pipes(format_name):
    pipes_mapping = []
    try:
        selected_row = get_formats_mapping().loc[format_name]
        pipes_mapping = [
            (idx[0] if not pd.isna(idx[0]) else "", row)
            for idx, row in selected_row.iterrows()
        ]
    except KeyError:
        pass
    pdf_filter_segment = _get_segment(format_name, "pdf_filter", pipes_mapping)
    text_extract_segment = _get_segment(format_name, "text_extract", pipes_mapping)
    deserialize_segment = _get_segment(format_name, "deserialize", pipes_mapping)
    return pdf_filter_segment, text_extract_segment, deserialize_segment
