"""Core algorithms module for PDF document processing pipelines.

This module provides the main execution functions for the three-stage processing pipeline:
1. PDF filtering - extract relevant blocks from PDF XML
2. Text extraction - convert PDF blocks to text blocks with company matching
3. Deserialization - convert text blocks to structured financial data

The module also handles pipeline composition and execution coordination.
"""

from typing import List, Callable, Dict, Tuple, Union, Any, Optional, Set
import logging as log
from multiprocessing import Pool
import freeports_lib
from .unstructured import get_compute_page_class
from freeports_analysis.formats.algorithms.data import (
    get_schedule,
    get_pageclassify_pipelines,
    get_mapping,
)
from freeports_analysis.consts import PromisesResolutionContext
from freeports_analysis.formats import LineParseFail, PageParseFail
from freeports_analysis.output import Investment
from freeports_analysis.i18n import _
from freeports_analysis.logging import LOG_CONTEXTUAL_INFOS, LOG_ADAPT_INVESTMENT_INFOS
from .. import PdfBlock, TextBlock
from .commons import Pipeline
from .pipelines import get_pipelines

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


class PageClassificationPipeline(Pipeline):
    def __call__(self, page, page_classes):
        pdf_blks = self.pdf_extract(page)
        txt_blk = self.text_filter(pdf_blks)
        return self.deserialize(txt_blk, page_classes)

    def __repr__(self):
        return "{}: =[{}--{}--{}]=>".format(
            self.__class__.__name__,
            repr(self.pdf_extract.pipes),
            repr(self.text_filter.pipes),
            repr(self.deserialize.pipe),
        )


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
    LOG_CONTEXTUAL_INFOS.page = None
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
    matches = None
    matches = (
        freeports_lib.text_extract.matcher.CompanyMatchInfos.compile_from_pandas_df(
            targets
        )
    )

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


type PageType = str


class PoolWorkersSettings:
    documents: int
    pages: int
    pipelines: int
    pipes: int


class PipelinesBundle:
    pipelines: Set[Pipeline]

    def __init__(self, pipelines=None):
        self.pipelines = set()
        if pipelines is not None:
            if isinstance(pipelines, Pipeline):
                self.pipelines.add(Pipeline)
            else:
                for p in pipelines:
                    self.pipelines.add(p)

    def __call__(self, page, filter_data):
        return [r for p in self.pipelines for r in p(page, filter_data)]

    def apply_pdf_extract(self, page):
        return [r for p in self.pipelines for r in p.pdf_extract(page)]

    def apply_text_filter(self, pdf_blks, filter_data):
        return [r for p in self.pipelines for r in p.text_filter(pdf_blks, filter_data)]

    def apply_deserialize(self, text_blks):
        return [r for p in self.pipelines for r in p.deserialize(text_blks)]

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}({len(self.pipelines)} pipelines)"

    def add_pipeline(self, pipeline: Pipeline):
        if not isinstance(pipeline, Pipeline):
            raise Exception(
                f"Pipelines bundle can contain only Pipeline, tried to add `{type(pipeline)}`"
            )
        self.pipelines.add(pipeline)


class Algorithm:
    page_classify_bundle: PipelinesBundle
    page_classify_finalizer: Callable[Any, PageType]
    schedule: List[Set[PageType]]
    bundles_mapping: Dict[PageType, PipelinesBundle]

    def __init__(
        self,
        pipelines_map: Dict[str, Pipeline],
        page_classify_pipelines: Set[str],
        page_classify_finalizer: Callable[Any, PageType],
        schedule: List[Set[PageType]],
        page_type_pipelines_mapping: Dict[PageType, Set[str]],
    ):
        known_pipelines = set(pipelines_map.keys())
        if not page_classify_pipelines.issubset(known_pipelines):
            unknown = page_classify_pipelines - know_pipelines
            raise Exception(
                f"Some page classify pipelines names have no mapping to pipeline implementation: {unknown}"
            )
        self.page_classify_bundle = PipelinesBundle(
            set(map(lambda name: pipelines_map[name], page_classify_pipelines))
        )
        self.page_classify_finalizer = page_classify_finalizer
        self.schedule = schedule
        self._page_classes = set([pt for step in self.schedule for pt in step])
        page_types_mapped_to_pipelines = set(page_type_pipelines_mapping)
        if self._page_classes != page_types_mapped_to_pipelines:
            diff = self._page_classes.symmetric_difference(
                page_types_mapped_to_pipelines
            )
            raise Exception(
                f"Page classes in schedule have to be mapped to pipelines names. The difference is {diff}"
            )
        pipelines_mapped_to_pagetype = set(
            [
                pn
                for pipeline_names in page_type_pipelines_mapping.values()
                for pn in pipeline_names
            ]
        )

        tot_pipelines_names = pipelines_mapped_to_pagetype.union(
            page_classify_pipelines
        )
        if tot_pipelines_names != known_pipelines:
            unknown = tot_pipelines_names - known_pipelines
            useless = known_pipelines - tot_pipelines_names
            raise Exception(
                f"There are pipeline names not mapped to implementation or mapped and not used. Unmapped: {unknown} Not used: {useless}"
            )

        self.bundles_mapping = {
            pt: PipelinesBundle(
                set(map(lambda name: pipelines_map[name], pipeline_names))
            )
            for pt, pipeline_names in page_type_pipelines_mapping.items()
        }
        self._page_classes = set([pt for step in self.schedule for pt in step])
        self._page_classes.add(None)

    @classmethod
    def load(cls, format_name: str):
        return cls(
            pipelines_map=get_pipelines(format_name, allow_partial_pipelines=False),
            page_classify_pipelines=get_pageclassify_pipelines(format_name),
            page_classify_finalizer=get_compute_page_class(format_name),
            schedule=get_schedule(format_name),
            page_type_pipelines_mapping=get_mapping(format_name),
        )

    def schedule_pages(self, pages):
        page_classification = [
            c for p in pages for c in self.page_classify_bundle(p, None)
        ]
        page_classification = self.page_classify_finalizer(page_classification)
        if len(page_classification) != len(pages):
            raise Exception(
                "Number of pages unclassified must be equal of number of page classified"
            )
        if not set(page_classification).issubset(self._page_classes):
            not_present = set(page_classification) - page_classes
            raise Exception(
                f"All pages have to enter in some point in the schedule, {not_present} are not part of the schedule"
            )
        pages_scheduled = [
            {
                pt: {
                    i: pages[i]
                    for i, page in enumerate(pages)
                    if page_classification[i] == pt
                }
                for step in self.schedule
                for pt in step
            }
        ]
        return pages_scheduled

    def __call__(self, list_pages, target_companies):
        compiled_target_companies = (
            freeports_lib.text_extract.matcher.CompanyMatchInfos.compile_from_pandas_df(
                target_companies
            )
        )
        pages_scheduled = self.schedule_pages(list_pages)
        res = {}
        new_filter_data = []
        for pages_type, pages in pages_scheduled[0].items():
            for page_n, page in pages.items():
                LOG_CONTEXTUAL_INFOS.page = page_n
                list_res = []
                try:
                    list_res = self.bundles_mapping[pages_type](
                        page, compiled_target_companies
                    )
                except PageParseFail as e:
                    logger_source.error(e)
                    logger.warning(_("Skipping page..."))
                new_filter_data.extend(list_res)
                res[page_n + 1] = list_res
                LOG_CONTEXTUAL_INFOS.page = None
        filter_data = new_filter_data
        for i in range(1, len(self.schedule)):
            new_filter_data = []
            for pages_type, pages in pages_scheduled[i].items():
                for page_n, page in pages.items():
                    LOG_CONTEXTUAL_INFOS.page = page_n
                    list_res = []
                    try:
                        list_res = self.bundles_mapping[pages_type](page, filter_data)
                    except PageParseFail as e:
                        logger_source.error(e)
                        logger.warning(_("Skipping page..."))
                    new_filter_data.extend(list_res)
                    res[page_n + 1] = list_res
                    LOG_CONTEXTUAL_INFOS.page = None
            filter_data = new_filter_data
        return res

    def classify_page(self, pages, page_number):
        page_classification = [
            c for p in pages for c in self.page_classify_bundle(p, None)
        ]
        return self.page_classify_finalizer(page_classification)[page_number - 1]

    def apply_to_page(self, pages, page_number, filter_data, page_class):
        return self.bundles_mapping[page_class](pages[page_number - 1], filter_data)

    def apply_pdf_extract(self, page, page_class):
        return self.bundles_mapping[page_class].apply_pdf_extract(page)

    def apply_text_filter(self, pdf_blks, filter_data, page_class):
        return self.bundles_mapping[page_class].apply_text_filter(pdf_blks, filter_data)

    def apply_deserialize(self, txt_blks, page_class):
        return self.bundles_mapping[page_class].apply_deserialize(txt_blks)
