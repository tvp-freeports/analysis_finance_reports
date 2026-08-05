"""Common utilities and data structures for algorithm pipeline management.

This module provides shared functionality for handling format and pipeline
identifiers, including validation schemas and index manipulation utilities.
"""

import pandera.pandas as pa
import pandas as pd
from enum import Enum
from typing import Optional, Callable, Any, Set, Iterator, List


from freeports._internals.formats.repo.metadata import FORMAT_NAME_REGEXP


class PipeIndexMode(Enum):
    """Strategy for determining pipe indices within a pipeline group."""

    INFER = "infer"
    """Infer index from row order within group."""
    EXPLICIT = "explicit"
    """Read index explicitly from the ID string."""


class MissingIndexPolicy(Enum):
    """Policy for handling missing pipe indices in explicit mode."""

    ZERO = "zero"
    """Fill missing indices with 0."""
    INFER = "infer"
    """Infer missing indices from row order within group."""


class FKRelation(Enum):
    """Type of foreign key relationship between tables."""

    ONE_TO_MAYBE = "one to maybe one"
    """One-to-zero-or-one relationship."""
    ONE_TO_ONE = "one to one"
    """One-to-one relationship."""
    ONE_TO_MANY = "one to many"
    """One-to-many relationship."""


# ============================================================
# Regular expressions
# ============================================================

pipeline_name_regexp: str = r"[0-9a-z_]*"
pipeline_regexp: str = rf"\(({pipeline_name_regexp})\)"
index_regexp: str = r"/([0-9]+)"


class IDFormat(Enum):
    """Expected format of ID column for validation."""

    EXPANDIBLE_NO_INDEX = 0
    """Format name with optional pipeline, no index suffix."""
    EXPANDIBLE = 1
    """Format name with optional pipeline and optional index suffix."""
    COMPLETE = 2
    """Format name with required pipeline and required index suffix."""


def check_id_column(id_format: IDFormat) -> pa.Check:
    """Build a pandera Check that validates ID column format.

    Parameters
    ----------
    id_format : IDFormat
        The expected ID format level.

    Returns
    -------
    pa.Check
        Pandera check for ID column validation.
    """
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
    df: pd.DataFrame, relation_to_principal: FKRelation = FKRelation.ONE_TO_ONE
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


def column_id_format_pipe(
    relation_to_principal: FKRelation = FKRelation.ONE_TO_ONE,
) -> pa.Column:
    """Build a pandera Column schema for ID with format-pipe validation.

    Parameters
    ----------
    relation_to_principal : FKRelation
        Relationship type determining ID format strictness.

    Returns
    -------
    pa.Column
        Pandera column schema for ID validation.
    """
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


def index_format_pipe(id_principal_table: Optional[Set[str]] = None) -> pa.MultiIndex:
    """Build a pandera MultiIndex schema for the format-pipeline index.

    Parameters
    ----------
    id_principal_table : Optional[Set[str]]
        Optional set of valid principal table IDs to validate against.

    Returns
    -------
    pa.MultiIndex
        Pandera MultiIndex schema for format-pipeline index validation.
    """
    # Pandera schema for validating format-pipeline index structure
    checks_id_idx = [check_id_column(IDFormat.COMPLETE)]
    if id_principal_table is not None:
        checks_id_idx.append(pa.Check(lambda x: x.isin(id_principal_table)))
    index_format_pipe_multindex: pa.MultiIndex = pa.MultiIndex(
        [
            pa.Index(
                pd.StringDtype,
                # [pa.Check(lambda x: x.isin(VALID_FORMATS))],
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
    """A collection of callable pipes forming one stage of a processing pipeline."""

    pipes: Set[Callable] = set()

    def add_pipe(self, pipe: Callable) -> None:
        """Add a callable pipe to this segment.

        Parameters
        ----------
        pipe : Callable
            The callable to add as a pipe.

        Raises
        ------
        Exception
            If the pipe is not callable.
        """
        if not callable(pipe):
            raise Exception(
                f"Pipe added to {self.__class__.__name__} has to be callable"
            )
        self.pipes.add(pipe)

    def __init__(self, pipes=None):
        self.pipes: Set[Callable] = set()
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

    def __repr__(self) -> str:
        """Return string representation of the segment and its pipes.

        Returns
        -------
        str
            Segment name and pipe set representation.
        """
        return "{}{}".format(self.__class__.__name__, repr(self.pipes))

    def __iter__(self) -> Iterator[Callable]:
        """Iterate over pipes in this segment.

        Returns
        -------
        Iterator[Callable]
            Iterator over the callable pipes.
        """
        return iter(self.pipes)

    def __add__(self, other: "PipelineSegement") -> "PipelineSegement":
        """Combine two segments of the same type by unioning their pipes.

        Parameters
        ----------
        other : PipelineSegement
            Another segment of the same type to merge.

        Returns
        -------
        PipelineSegement
            New segment with the union of pipes.

        Raises
        ------
        Exception
            If other is not the same segment type.
        """
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

    def __call__(self, page: Any) -> List[Any]:
        """Extract PDF blocks from a page using all pipes in this segment.

        Parameters
        ----------
        page : Any
            The PDF page to extract blocks from.

        Returns
        -------
        List[Any]
            List of extracted PDF blocks.
        """
        return [pdf_blk for pipe in self for pdf_blk in pipe(page)]


class TextFilterSegment(PipelineSegement):
    """Text Filter"""

    def __call__(self, pdf_blks: List[Any], filter_data: Any) -> List[Any]:
        """Filter text blocks using all pipes in this segment.

        Parameters
        ----------
        pdf_blks : List[Any]
            PDF blocks to filter.
        filter_data : Any
            Filtering context data.

        Returns
        -------
        List[Any]
            List of filtered text blocks.
        """
        return [txt_blk for pipe in self for txt_blk in pipe(pdf_blks, filter_data)]


class DeserializeSegment(PipelineSegement):
    """Deserialize"""

    def __call__(self, txt_blks: List[Any]) -> List[Any]:
        """Deserialize text blocks using all pipes in this segment.

        Parameters
        ----------
        txt_blks : List[Any]
            Text blocks to deserialize.

        Returns
        -------
        List[Any]
            List of deserialized results.
        """
        return [pipe(blk) for pipe in self for blk in txt_blks]


class Pipeline:
    """A three-stage processing pipeline for PDF data extraction.

    Composed of three PipelineSegments executed in order:
    pdf_extract, text_filter, and deserialize.
    """

    pdf_extract: PdfExtractSegment
    text_filter: TextFilterSegment
    deserialize: DeserializeSegment

    def __init__(
        self,
        pdf_extract: Optional[PdfExtractSegment] = None,
        text_filter: Optional[TextFilterSegment] = None,
        deserialize: Optional[DeserializeSegment] = None,
    ) -> None:
        """Initialize pipeline with optional segment configurations.

        Parameters
        ----------
        pdf_extract : Optional[PdfExtractSegment]
            PDF extraction segment or callable(s) to wrap.
        text_filter : Optional[TextFilterSegment]
            Text filter segment or callable(s) to wrap.
        deserialize : Optional[DeserializeSegment]
            Deserialization segment or callable(s) to wrap.
        """
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

    def add_pdf_extract(self, pdf_extract: Callable) -> None:
        """Add a pipe to the PDF extraction segment.

        Parameters
        ----------
        pdf_extract : Callable
            Callable to add as a pdf_extract pipe.
        """
        self.pdf_extract.add_pipe(pdf_extract)

    def add_text_filter(self, text_filter: Callable) -> None:
        """Add a pipe to the text filter segment.

        Parameters
        ----------
        text_filter : Callable
            Callable to add as a text_filter pipe.
        """
        self.text_filter.add_pipe(text_filter)

    def add_deserialize(self, deserialize: Callable) -> None:
        """Add a pipe to the deserialization segment.

        Parameters
        ----------
        deserialize : Callable
            Callable to add as a deserialize pipe.
        """
        self.deserialize.add_pipe(deserialize)

    def complete(self) -> bool:
        """Check if all pipeline segments contain at least one pipe.

        Returns
        -------
        bool
            True if all segments are non-empty.
        """
        return all(map(lambda seg: len(seg.pipes) > 0, self))

    def __iter__(self) -> Iterator[PipelineSegement]:
        """Iterate over pipeline segments in processing order.

        Returns
        -------
        Iterator[PipelineSegement]
            Iterator over pdf_extract, text_filter, deserialize segments.
        """
        return iter((self.pdf_extract, self.text_filter, self.deserialize))

    def __repr__(self) -> str:
        """Return string representation of the pipeline.

        Returns
        -------
        str
            Pipeline representation showing segment contents.
        """
        return "{}: =[{}--{}--{}]=>".format(
            self.__class__.__name__,
            repr(self.pdf_extract.pipes),
            repr(self.text_filter.pipes),
            repr(self.deserialize.pipes),
        )

    def __call__(self, page: Any, filter_data: Any) -> List[Any]:
        """Execute the full pipeline on a page.

        Parameters
        ----------
        page : Any
            The PDF page to process.
        filter_data : Any
            Filtering context data.

        Returns
        -------
        List[Any]
            Deserialized results from the pipeline.
        """
        pdf_blks = self.pdf_extract(page)
        txt_blks = self.text_filter(pdf_blks, filter_data)
        return self.deserialize(txt_blks)

    def __add__(self, other: "Pipeline") -> "Pipeline":
        """Combine two pipelines by merging their segments.

        Parameters
        ----------
        other : Pipeline
            Another Pipeline to merge with.

        Returns
        -------
        Pipeline
            New Pipeline with merged segments.

        Raises
        ------
        Exception
            If other is not a Pipeline instance.
        """
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
