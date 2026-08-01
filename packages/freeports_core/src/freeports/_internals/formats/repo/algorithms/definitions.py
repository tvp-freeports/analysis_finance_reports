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
from freeports._internals.formats.repo.algorithms.unstructured.acquisition import (
    get_compute_page_class,
)
from freeports._internals.formats.repo.orchestration import (
    get_schedule,
    get_pageclassify_pipelines,
    get_mapping,
)
from freeports._internals.core.promises import PromisesResolutionContext
from freeports._internals.core.classes import LineParseFail, PageParseFail
from freeports._internals.output.classes_schema import Investment
from freeports.i18n import _
from freeports._internals.core.logging import (
    LOG_CONTEXTUAL_INFOS,
    LOG_ADAPT_INVESTMENT_INFOS,
)
from freeports.core import PdfBlock, TextBlock
from .pipelines_definition import Pipeline
from .pipelines_acquisition import get_pipelines

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

    def apply_text_filter(self, page, filter_data):
        return [
            r
            for p in self.pipelines
            for r in p.text_filter(p.pdf_extract(page), filter_data)
        ]

    def apply_deserialize(self, page, filter_data):
        return [
            r
            for p in self.pipelines
            for r in p.deserialize(p.text_filter(p.pdf_extract(page), filter_data))
            if r is not None
        ]

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
            unknown = page_classify_pipelines - known_pipelines
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
    def load(cls, formats_repo_dir, format_name: str, format_repo_validation_data):
        return cls(
            pipelines_map=get_pipelines(
                formats_repo_dir, format_name, allow_partial_pipelines=False
            ),
            page_classify_pipelines=get_pageclassify_pipelines(
                formats_repo_dir, format_name, format_repo_validation_data
            ),
            page_classify_finalizer=get_compute_page_class(
                formats_repo_dir, format_name
            ),
            schedule=get_schedule(
                formats_repo_dir, format_name, format_repo_validation_data
            ),
            page_type_pipelines_mapping=get_mapping(
                formats_repo_dir, format_name, format_repo_validation_data
            ),
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
            not_present = set(page_classification) - self._page_classes
            raise Exception(
                f"All pages have to enter in some point in the schedule, {not_present} are not part of the schedule"
            )
        pages_scheduled = [
            {
                pt: {
                    i + 1: pages[i]
                    for i, page in enumerate(pages)
                    if page_classification[i] == pt
                }
                for pt in step
            }
            for step in self.schedule
        ]
        return pages_scheduled

    def __call__(self, list_pages, target_companies):
        compiled_target_companies = (
            freeports_lib.text_filter.matcher.CompanyMatchInfos.compile_from_pandas_df(
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
                    list_res = [
                        r
                        for r in self.bundles_mapping[pages_type](
                            page, compiled_target_companies
                        )
                        if r is not None
                    ]
                except PageParseFail as e:
                    logger_source.error(e)
                    logger.warning(_("Skipping page..."))
                new_filter_data.extend(list_res)
                res[page_n] = list_res
                LOG_CONTEXTUAL_INFOS.page = None
        filter_data = [n for n in new_filter_data]

        for i in range(1, len(self.schedule)):
            for pages_type, pages in pages_scheduled[i].items():
                for page_n, page in pages.items():
                    LOG_CONTEXTUAL_INFOS.page = page_n
                    list_res = []
                    try:
                        list_res = [
                            r
                            for r in self.bundles_mapping[pages_type](page, filter_data)
                            if r is not None
                        ]
                    except PageParseFail as e:
                        logger_source.error(e)
                        logger.warning(_("Skipping page..."))
                    new_filter_data.extend(list_res)
                    res[page_n] = list_res
                    LOG_CONTEXTUAL_INFOS.page = None
            filter_data.extend([n for n in new_filter_data])
        return res

    def classify_pages(self, pages):
        page_classification = [
            c for p in pages for c in self.page_classify_bundle(p, None)
        ]
        return self.page_classify_finalizer(page_classification)

    def apply_to_page(self, pages, page_number, filter_data, page_class):
        return self.bundles_mapping[page_class](pages[page_number - 1], filter_data)

    def apply_pdf_extract(self, page, page_class):
        return self.bundles_mapping[page_class].apply_pdf_extract(page)

    def apply_text_filter(self, page, filter_data, page_class):
        return self.bundles_mapping[page_class].apply_text_filter(page, filter_data)

    def apply_deserialize(self, page, filter_data, page_class):
        return self.bundles_mapping[page_class].apply_deserialize(page, filter_data)
