"""Module common to each format, it contains the definitions used by all the formats"""

from typing import List, Callable
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
from freeports_analysis.logging import LOG_CONTEXTUAL_INFOS
from .. import PdfBlock, TextBlock

logger_source = log.getLogger(__name__)
logger = log.getLogger("freeports_analysis.formats.utils")


class LogFormatterWithPage(log.Formatter):
    """Formatter that inherit the behaviour from
    another formatter given in input, but insert into it
    an attrinbute that rappresent the page number of the pdf report
    """

    def __init__(self, old_formatter: log.Formatter):
        """Initialize the LogFormatterWithPage taking another formatter
        as reference to modify

        Parameters
        ----------
        old_formatter : logging.Formatter
            the formatter to take as reference
        """
        super().__init__()
        self._parent_fmt = old_formatter
        self.page = None

    def format(self, record: log.LogRecord) -> str:
        """Method used to get the rappresentation of the report.
        overwrite the inherited one

        Parameters
        ----------
        record : logging.LogRecord
            the record to format

        Returns
        -------
        str
            formatted version of the record
        """
        string = self._parent_fmt.format(record).replace(":", f"{{pag. {self.page}}}:")
        return string


def _exec_segment(
    i_batch_page, n_pages, args_batch, funcs, error_msg, progress_msg=None
):
    args_batch = enumerate(args_batch, start=i_batch_page)
    show_progress = False if progress_msg is None else True
    batch_results = []
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
            logger.warning(
                _("Skipping page..."),
            )
    return batch_results


def pdf_filter_exec(
    i_batch_page: int,
    n_pages: int,
    batch_pages: List[str],
    pdf_filter_funcs: List[Callable[[List[str]], List[PdfBlock]]],
) -> List[PdfBlock]:
    """Processes a PDF document through a filter function to extract relevant blocks.

    Args
    ----

    document : List[str]
        The PDF document to process as a list of xml pages.
    i_batch_page : int
        Starting page of the batch processed by the instance of `pdf_filter_exec` function,
        used for informative purposes
    n_pages : int
        Total number of pages in the document, used for informative purposes.
    pdf_filter_func : Callable[[List[str]], List[PdfBlock]]
        A function that takes an XML element and returns a list of relevant PdfBlock.

    Returns
    -------
    List[PdfBlock]
        PdfBlock objects containing the filtered content.
    """

    batch_results = _exec_segment(
        i_batch_page,
        n_pages,
        batch_pages,
        pdf_filter_funcs,
        _("Fatal error in pdf filter"),
        _("Still filtering..."),
    )
    return batch_results


def text_extract_exec(
    i_batch_page: int,
    n_pages: int,
    pdf_blocks_batch: List[List[PdfBlock]],
    targets: List[str],
    text_extract_funcs: Callable[[List[PdfBlock], List[str]], List[TextBlock]],
) -> List[TextBlock]:
    """Extracts text content from PDF blocks using a specified extraction function.

    Args
    ----
    pdf_blocks : List[PdfBlock]
        PdfBlock objects to process.
    targets : List[str]
        Target companies identified for extraction.
    text_extract_func : Callable[[List[PdfBlock], List[str]], List[TextBlock]]
        Function that processes PdfBlocks and targets into TextBlocks.

    Returns
    -------
    List[TextBlock]
        TextBlock objects containing the extracted content.
    """

    def _add_targets_to_txt_extract(f):
        return lambda blks: f(blks, dataframe_to_match(targets))

    text_extract_funcs_with_targets = [
        _add_targets_to_txt_extract(text_extract) for text_extract in text_extract_funcs
    ]
    batch_results = _exec_segment(
        i_batch_page,
        n_pages,
        pdf_blocks_batch,
        text_extract_funcs_with_targets,
        _("Invalid text extraction!!"),
        _("Still extracting..."),
    )
    return batch_results


def deserialize_exec(
    i_batch_page: int,
    n_pages: int,
    text_blocks_batch: List[List[TextBlock]],
    deserialize_funcs: List[
        Callable[[TextBlock, List[str]], Investment | PromisesResolutionContext]
    ],
) -> List[Investment | PromisesResolutionContext]:
    """Converts TextBlocks into tabular data using a specified function that
    from an expected formatting, return a python object.

    Args
    ----
    text_blocks : List[List[TextBlock]]
        TextBlock objects to process.
    targets : List[str]
        Targets companies to validate the object creation
    deserialize_func : Callable[[TextBlock, List[str]], FinancialData | PromisesResolutionContext]
        Function that converts a TextBlock into a finantial data class or into
        a bit of context for resolving deferred values

    Returns
    -------
    List[FinancialData | PromisesResolutionContext]
        FinantialData classes containing the deserialized data or context
        for resolving deferred values
    """

    def _add_loop_to_deserialize(f):
        def new_f(blks):
            results = []
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
        i_batch_page,
        n_pages,
        text_blocks_batch,
        deserialize_funcs_blks,
        _("Invalid deserialization!!"),
    )
    return batch_results


def get_pipelines(format_name: str, allow_partial_pipelines: bool = False):
    struct = get_structured(format_name)
    semistruct = get_semistructured(format_name)
    unstruct = get_unstructured(format_name)

    # Combina i dizionari per categoria
    categories = ["pdf_filters", "text_extract", "deserialize"]
    combined = {}

    for i, category in enumerate(categories):
        combined[category] = {**struct[i], **semistruct[i], **unstruct[i]}

    # Verifica che i dizionari non siano vuoti
    for category, data in combined.items():
        if not data and not allow_partial_pipelines:
            raise ValueError(_("List of {} cannot be empty").format(category))

    # Ottieni tutte le chiavi uniche
    all_keys = set(
        key for category_data in combined.values() for key in category_data.keys()
    )

    # Crea il risultato finale con controlli
    result = {}
    for key in all_keys:
        pdf_filters = combined["pdf_filters"].get(key, [])
        text_extract = combined["text_extract"].get(key, [])
        deserialize = combined["deserialize"].get(key, [])

        # Verifica che nessuna lista sia vuota
        if not allow_partial_pipelines:
            if not pdf_filters:
                raise ValueError(f"Pipeline '{key}': pdf_filters non può essere vuoto")
            if not text_extract:
                raise ValueError(f"Pipeline '{key}': text_extract non può essere vuoto")
            if not deserialize:
                raise ValueError(f"Pipeline '{key}': deserialize non può essere vuoto")

        result[key] = (pdf_filters, text_extract, deserialize)

    return result
