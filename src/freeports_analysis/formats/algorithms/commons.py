"""Common utilities and data structures for algorithm pipeline management.

This module provides shared functionality for handling format and pipeline
identifiers, including validation schemas and index manipulation utilities.
"""

import pandera.pandas as pa
import pandas as pd
from enum import Enum
from typing import Optional


from freeports_analysis.formats.data import FORMAT_NAME_REGEXP, VALID_FORMATS


class PipeIndexMode(Enum):
    INFER = "infer"
    EXPLICIT = "explicit"


class MissingIndexPolicy(Enum):
    ZERO = "zero"
    INFER = "infer"


class FKRelation(Enum):
    ONE_TO_MAYBE = "one to maybe one"
    ONE_TO_ONE = "one to one"
    ONE_TO_MANY = "one to many"


# ============================================================
# Regular expressions
# ============================================================

pipeline_name_regexp: str = r"[0-9a-z_]*"
pipeline_regexp: str = rf"\(({pipeline_name_regexp})\)"
index_regexp: str = r"/([0-9]+)"


class IDFormat(Enum):
    EXPANDIBLE_NO_INDEX = 0
    EXPANDIBLE = 1
    COMPLETE = 2


def check_id_column(id_format: IDFormat):
    reg = rf"{FORMAT_NAME_REGEXP}{pipeline_regexp}{index_regexp}"
    if id_format == IDFormat.EXPANDIBLE:
        reg = rf"{FORMAT_NAME_REGEXP}({pipeline_regexp})?({index_regexp})?"
    elif id_format == IDFormat.EXPANDIBLE_NO_INDEX:
        reg = rf"{FORMAT_NAME_REGEXP}({pipeline_regexp})?"

    return pa.Check(lambda x: x.str.match(f"^{reg}$"))


def add_format_name(df: pd.DataFrame) -> pd.DataFrame:
    """Extract format name from ID (removes pipeline and index suffix)."""

    df = df.assign(
        format_name=lambda x: x["ID"].str.replace(
            rf"({pipeline_regexp})?({index_regexp})?$", "", regex=True
        )
    )

    return df.rename(columns={"format_name": "Format name"})


def add_pipeline_name(
    df: pd.DataFrame,
    default: Optional[str] = None,
) -> pd.DataFrame:
    """
    Extract pipeline name from ID.

    If missing, fill with `default`.
    """

    df = df.assign(
        pipeline_name=lambda x: x["ID"].str.extract(rf"\(({pipeline_name_regexp})\)")[0]
    )

    if default is not None:
        df["pipeline_name"] = df["pipeline_name"].fillna(default)

    return df.rename(columns={"pipeline_name": "Pipeline name"})


def add_pipe_index(
    df: pd.DataFrame, relation_to_principal: FKRelation.ONE_TO_ONE
) -> pd.DataFrame:
    """
    Add 'Pipe index' column.

    Parameters
    ----------
    mode:
        PipeIndexMode.INFER
        PipeIndexMode.EXPLICIT

    missing_index_policy (only for EXPLICIT mode):
        MissingIndexPolicy.ZERO
        MissingIndexPolicy.INFER
    """
    mode = (
        PipeIndexMode.INFER
        if relation_to_principal == FKRelation.ONE_TO_ONE
        else PipeIndexMode.EXPLICIT
    )
    missing_index_policy = (
        MissingIndexPolicy.ZERO
        if relation_to_principal == FKRelation.ONE_TO_MANY
        else MissingIndexPolicy.INFER
    )
    df = df.copy()

    if mode is PipeIndexMode.EXPLICIT:
        extracted = df["ID"].str.extract(rf"{index_regexp}$")[0]
        df["Pipe index"] = extracted.astype("Int32")

        if missing_index_policy is MissingIndexPolicy.ZERO:
            df["Pipe index"] = df["Pipe index"].fillna(0)

        elif missing_index_policy is MissingIndexPolicy.INFER:
            mask_missing = df["Pipe index"].isna()

            df.loc[mask_missing, "Pipe index"] = (
                df[mask_missing].groupby(["Format name", "Pipeline name"]).cumcount()
            )

        df["Pipe index"] = df["Pipe index"].astype("Int32")

    elif mode is PipeIndexMode.INFER:
        df["Pipe index"] = (
            df.groupby(["Format name", "Pipeline name"]).cumcount().astype("Int32")
        )

    else:
        raise ValueError(f"Unsupported PipeIndexMode: {mode}")

    return df


def create_index_format_name_pipe(
    df: pd.DataFrame,
    pipeline_default: Optional[str] = None,
    relation_to_principal: FKRelation = FKRelation.ONE_TO_ONE,
) -> pd.DataFrame:
    """
    Full pipeline:
        1. Add Format name
        2. Add Pipeline name
        3. Add Pipe index
        4. Set MultiIndex
    """

    df = add_format_name(df)
    df = add_pipeline_name(df, default=pipeline_default)
    df = add_pipe_index(df, relation_to_principal=relation_to_principal)
    df["Computed ID"] = (
        df["Format name"]
        + "("
        + df["Pipeline name"]
        + ")/"
        + df["Pipe index"].astype(str)
    )
    return df.set_index(["Format name", "Pipeline name", "Pipe index", "Computed ID"])


def column_id_format_pipe(relation_to_principal: FKRelation.ONE_TO_ONE):
    return pa.Column(
        pd.StringDtype,
        checks=[
            check_id_column(
                IDFormat.EXPANDIBLE_NO_INDEX
                if relation_to_principal == FKRelation.ONE_TO_ONE
                else IDFormat.EXPANDIBLE
            )
        ],
        nullable=True,
    )


def index_format_pipe(id_principal_table=None):
    # Pandera schema for validating format-pipeline index structure
    checks_id_idx = [check_id_column(IDFormat.COMPLETE)]
    if id_principal_table is not None:
        checks_id_idx.append(pa.Check(lambda x: x.isin(id_principal_table)))
    index_format_pipe_multindex: pa.MultiIndex = pa.MultiIndex(
        [
            pa.Index(
                pd.StringDtype,
                [pa.Check(lambda x: x.isin(VALID_FORMATS))],
                name="Format name",
            ),
            pa.Index(
                pd.StringDtype,
                [pa.Check(lambda x: x.str.match(f"^{pipeline_name_regexp}$"))],
                name="Pipeline name",
                nullable=False,
            ),
            pa.Index(
                pd.UInt16Dtype,
                name="Pipe index",
            ),
            pa.Index(
                pd.StringDtype,
                checks_id_idx,
                name="Computed ID",
            ),
        ]
    )
    return index_format_pipe_multindex


class PipelineSegement:
    pipes = set()

    def add_pipe(self, pipe):
        if not callable(pipe):
            raise Exception(
                f"Pipe added to {self.__class__.__name__} has to be callable"
            )
        self.pipes.add(pipe)

    def __init__(self, pipes=None):
        self.pipes = set()
        try:
            for p in pipes:
                self.add_pipe(p)
        except TypeError:
            if callable(pipes):
                self.add_pipe(pipes)
            elif pipes is not None:
                raise Exception(
                    f"Specified pipes {pipes} is nor an iterable or a callable"
                )

    def __repr__(self):
        return "{}{}".format(self.__class__.__name__, repr(self.pipes))

    def __iter__(self):
        return iter(self.pipes)

    def __add__(self, other):
        cls = self.__class__
        if not isinstance(other, cls):
            raise Exception(
                f"Cannot sum segments of different type. First is {self.__class__.__name__}, second {other.__class__.__name__}"
            )
        new_seg = cls()
        new_seg.pipes = self.pipes.union(other.pipes)
        return new_seg


class PdfExtractSegment(PipelineSegement):
    """Pdf Extract"""

    def __call__(self, page):
        return [pdf_blk for pipe in self for pdf_blk in pipe(page)]


class TextFilterSegment(PipelineSegement):
    """Text Filter"""

    def __call__(self, pdf_blks, filter_data):
        return [txt_blk for pipe in self for txt_blk in pipe(pdf_blks, filter_data)]


class DeserializeSegment(PipelineSegement):
    """Deserialize"""

    def __call__(self, txt_blks):
        return [pipe(blk) for pipe in self for blk in txt_blks]


class Pipeline:
    pdf_extract: PdfExtractSegment
    text_filter: TextFilterSegment
    deserialize: DeserializeSegment

    def __init__(self, pdf_extract=None, text_filter=None, deserialize=None):
        self.pdf_extract = (
            pdf_extract
            if isinstance(pdf_extract, PdfExtractSegment)
            else PdfExtractSegment(pdf_extract)
        )
        self.text_filter = (
            text_filter
            if isinstance(text_filter, TextFilterSegment)
            else TextFilterSegment(text_filter)
        )
        self.deserialize = (
            deserialize
            if isinstance(deserialize, DeserializeSegment)
            else DeserializeSegment(deserialize)
        )

    def add_pdf_extract(self, pdf_extract):
        self.pdf_extract.add_pipe(pdf_extract)

    def add_text_filter(self, text_filter):
        self.text_filter.add_pipe(text_filter)

    def add_deserialize(self, deserialize):
        self.deserialize.add_pipe(deserialize)

    def complete(self) -> bool:
        return all(map(lambda seg: len(seg.pipes) > 0, self))

    def __iter__(self):
        return iter((self.pdf_extract, self.text_filter, self.deserialize))

    def __repr__(self):
        return "{}: =[{}--{}--{}]=>".format(
            self.__class__.__name__,
            repr(self.pdf_extract.pipes),
            repr(self.text_filter.pipes),
            repr(self.deserialize.pipes),
        )

    def __call__(self, page, filter_data):
        pdf_blks = self.pdf_extract(page)
        txt_blks = self.text_filter(pdf_blks, filter_data)
        return self.deserialize(txt_blks)

    def __add__(self, other):
        cls = self.__class__
        if not isinstance(other, cls):
            raise Exception(
                f"Cannot sum segments of different type. First is {self.__class__.__name__}, second {other.__class__.__name__}"
            )
        return cls(
            pdf_extract=self.pdf_extract + other.pdf_extract,
            text_filter=self.text_filter + other.text_filter,
            deserialize=self.deserialize + other.deserialize,
        )
