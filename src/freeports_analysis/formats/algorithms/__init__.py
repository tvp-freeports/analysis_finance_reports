"""Core algorithms module for PDF document processing pipelines.

This module provides the main execution functions for the three-stage processing pipeline:
1. PDF filtering - extract relevant blocks from PDF XML
2. Text extraction - convert PDF blocks to text blocks with company matching
3. Deserialization - convert text blocks to structured financial data

The module also handles pipeline composition and execution coordination.
"""

from typing import List, Callable, Dict, Tuple, Union, Any, Optional
import logging as log
from freeports_analysis.formats.algorithms.unstructured import (
    get_pipes as get_unstructured,
)
from freeports_analysis.formats.algorithms.semistructured import (
    get_pipes as get_semistructured,
)
from freeports_analysis.formats.algorithms.structured import get_pipes as get_structured
from freeports_analysis.formats.utils.text_extract.match import dataframe_to_match
from freeports_analysis.consts import PromisesResolutionContext
from freeports_analysis.formats import LineParseFail, PageParseFail
from freeports_analysis.output import Investment
from freeports_analysis.i18n import _
from freeports_analysis.logging import LOG_CONTEXTUAL_INFOS, LOG_ADAPT_INVESTMENT_INFOS
from .. import PdfBlock, TextBlock

logger_source = log.getLogger(__name__)
logger = log.getLogger("freeports_analysis.formats.utils")


class LogFormatterWithPage(log.Formatter):
    """Log formatter that adds page number context to log messages.

    This formatter wraps an existing formatter and inserts page number
    information into formatted log records.

    Attributes
    ----------
    _parent_fmt : log.Formatter
        The original formatter to wrap
    page : Optional[int]
        Current page number for context
    """

    def __init__(self, old_formatter: log.Formatter):
        """Initialize the LogFormatterWithPage with a reference formatter.

        Parameters
        ----------
        old_formatter : log.Formatter
            The formatter to use as a base for formatting

        Notes
        -----
        The page number is dynamically inserted into log messages
        by replacing colons with page context information.
        """
        super().__init__()
        self._parent_fmt = old_formatter
        self.page: Optional[int] = None

    def format(self, record: log.LogRecord) -> str:
        """Format a log record with page number context.

        Parameters
        ----------
        record : log.LogRecord
            The log record to format

        Returns
        -------
        str
            Formatted log message with page number inserted
        """
        string = self._parent_fmt.format(record).replace(":", f"{{pag. {self.page}}}:")
        return string


def _exec_segment(
    i_batch_page: int,
    n_pages: int,
    args_batch: List[Any],
    funcs: List[Callable],
    progress_msg: Optional[str] = None,
) -> List[List[Any]]:
    """Execute a segment of processing functions with error handling and progress reporting.

    Parameters
    ----------
    i_batch_page : int
        Starting page index for this batch
    n_pages : int
        Total number of pages in the document
    args_batch : List[Any]
        List of arguments to pass to the functions
    funcs : List[Callable]
        List of functions to execute
    progress_msg : Optional[str]
        Progress message to log periodically

    Returns
    -------
    List[List[Any]]
        Combined results from all function executions

    Raises
    ------
    PageParseFail
        If a page cannot be parsed (logged as warning, page is skipped)

    Notes
    -----
    This function handles the execution of processing functions for a batch
    of pages, providing progress reporting and error handling. Pages that
    fail to parse are skipped with a warning, allowing processing to continue.
    """
    args_batch = enumerate(args_batch, start=i_batch_page)
    show_progress = progress_msg is not None
    batch_results: List[List[Any]] = []

    for page, arg in args_batch:
        LOG_CONTEXTUAL_INFOS.page = page
        if show_progress and (
            (page + i_batch_page) % (n_pages // min(10, n_pages)) == 0
        ):
            logger.info(progress_msg)
        try:
            batch_results.append([r for func in funcs for r in func(arg)])
        except PageParseFail as e:
            logger_source.error(e)
            logger.warning(_("Skipping page..."))
    return batch_results


def pdf_filter_exec(
    i_batch_page: int,
    n_pages: int,
    batch_pages: List[str],
    pdf_filter_funcs: List[Callable[[str], List[PdfBlock]]],
) -> List[List[PdfBlock]]:
    """Execute PDF filtering functions to extract relevant blocks from PDF XML.

    Parameters
    ----------
    i_batch_page : int
        Starting page index for this batch
    n_pages : int
        Total number of pages in the document
    batch_pages : List[str]
        List of XML page strings to process
    pdf_filter_funcs : List[Callable[[str], List[PdfBlock]]]
        List of functions that extract PdfBlocks from XML

    Returns
    -------
    List[List[PdfBlock]]
        List of PdfBlock lists, one per page
    """
    batch_results = _exec_segment(
        i_batch_page,
        n_pages,
        batch_pages,
        pdf_filter_funcs,
        _("Still filtering..."),
    )
    return batch_results


def text_extract_exec(
    i_batch_page: int,
    n_pages: int,
    pdf_blocks_batch: List[List[PdfBlock]],
    targets: List[str],
    text_extract_funcs: List[Callable[[List[PdfBlock], Any], List[TextBlock]]],
) -> List[List[TextBlock]]:
    """Execute text extraction functions to convert PdfBlocks to TextBlocks with company matching.

    Parameters
    ----------
    i_batch_page : int
        Starting page index for this batch
    n_pages : int
        Total number of pages in the document
    pdf_blocks_batch : List[List[PdfBlock]]
        Batch of PdfBlock lists to process
    targets : List[str]
        Target companies for matching
    text_extract_funcs : List[Callable[[List[PdfBlock], Any], List[TextBlock]]]
        List of text extraction functions

    Returns
    -------
    List[List[TextBlock]]
        List of TextBlock lists, one per page
    """
    matches = dataframe_to_match(targets)

    def _add_targets_to_txt_extract(f: Callable) -> Callable:
        return lambda blks: f(blks, matches)

    text_extract_funcs_with_targets = [
        _add_targets_to_txt_extract(text_extract) for text_extract in text_extract_funcs
    ]
    batch_results = _exec_segment(
        i_batch_page,
        n_pages,
        pdf_blocks_batch,
        text_extract_funcs_with_targets,
        _("Still extracting..."),
    )
    return batch_results


def deserialize_exec(
    i_batch_page: int,
    n_pages: int,
    text_blocks_batch: List[List[TextBlock]],
    deserialize_funcs: List[
        Callable[[TextBlock], Union[Investment, PromisesResolutionContext]]
    ],
) -> List[List[Union[Investment, PromisesResolutionContext]]]:
    """Execute deserialization functions to convert TextBlocks to financial data objects.

    Parameters
    ----------
    i_batch_page : int
        Starting page index for this batch
    n_pages : int
        Total number of pages in the document
    text_blocks_batch : List[List[TextBlock]]
        Batch of TextBlock lists to process
    deserialize_funcs : List[Callable[[TextBlock], Union[Investment, PromisesResolutionContext]]]
        List of deserialization functions

    Returns
    -------
    List[List[Union[Investment, PromisesResolutionContext]]]
        List of financial data objects or promise contexts
    """

    def _add_loop_to_deserialize(f: Callable) -> Callable:
        def new_f(
            blks: List[TextBlock],
        ) -> List[Union[Investment, PromisesResolutionContext]]:
            results: List[Union[Investment, PromisesResolutionContext]] = []
            for blk in blks:
                try:
                    results.append(f(blk))
                except LineParseFail as e:
                    logger.error(e)
                    LOG_ADAPT_INVESTMENT_INFOS.row = None
                    LOG_ADAPT_INVESTMENT_INFOS.col = None
                    LOG_ADAPT_INVESTMENT_INFOS.field = None
                    logger.warning(_("Skipping line..."))
            return results

        return new_f

    deserialize_funcs_blks = [
        _add_loop_to_deserialize(deserialize) for deserialize in deserialize_funcs
    ]
    batch_results = _exec_segment(
        i_batch_page, n_pages, text_blocks_batch, deserialize_funcs_blks
    )
    return batch_results


def get_pipelines(
    format_name: str, allow_partial_pipelines: bool = False
) -> Dict[str, Tuple[List[Callable], List[Callable], List[Callable]]]:
    """Get processing pipelines for a specific format.

    Combines structured, semi-structured, and unstructured pipelines for the given format.

    Parameters
    ----------
    format_name : str
        Name of the format to get pipelines for
    allow_partial_pipelines : bool
        Whether to allow pipelines with missing components

    Returns
    -------
    Dict[str, Tuple[List[Callable], List[Callable], List[Callable]]]
        Dictionary mapping pipeline names to (pdf_filters, text_extract, deserialize) tuples

    Raises
    ------
    ValueError
        If required pipeline components are missing and allow_partial_pipelines is False

    Notes
    -----
    Each pipeline consists of three components:
    - pdf_filters: Functions that extract relevant blocks from PDF XML
    - text_extract: Functions that convert PDF blocks to text blocks with company matching
    - deserialize: Functions that convert text blocks to structured financial data

    The function combines pipelines from structured, semi-structured, and unstructured
    processing approaches to provide comprehensive format support.
    """
    struct = get_structured(format_name)
    semistruct = get_semistructured(format_name)
    unstruct = get_unstructured(format_name)

    # Combine dictionaries by category
    categories = ["pdf_filters", "text_extract", "deserialize"]
    combined: Dict[str, Dict[str, List[Callable]]] = {}

    for i, category in enumerate(categories):
        combined[category] = {**struct[i], **semistruct[i], **unstruct[i]}

    # Verify dictionaries are not empty
    for category, data in combined.items():
        if not data and not allow_partial_pipelines:
            raise ValueError(_("List of {} cannot be empty").format(category))

    # Create final result with validation
    result: Dict[str, Tuple[List[Callable], List[Callable], List[Callable]]] = {}
    for key in set(
        key for category_data in combined.values() for key in category_data.keys()
    ):
        pdf_filters = combined["pdf_filters"].get(key, [])
        text_extract = combined["text_extract"].get(key, [])
        deserialize = combined["deserialize"].get(key, [])

        # Verify no list is empty
        if not allow_partial_pipelines:
            if not pdf_filters:
                raise ValueError(f"Pipeline '{key}': pdf_filters cannot be empty")
            if not text_extract:
                raise ValueError(f"Pipeline '{key}': text_extract cannot be empty")
            if not deserialize:
                raise ValueError(f"Pipeline '{key}': deserialize cannot be empty")

        result[key] = (pdf_filters, text_extract, deserialize)

    return result
