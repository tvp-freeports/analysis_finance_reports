"""Module common to each format, it contains the definitions used by all the formats"""

from typing import List, Callable
import logging as log
from freeports_analysis.formats.unstructured import get_pipelines as get_unstructured
from freeports_analysis.formats.semistructured import get_pipes as get_semistructured
from freeports_analysis.formats.structured import get_pipes as get_structured
from freeports_analysis.consts import FinancialData, PromisesResolutionContext
from freeports_analysis.i18n import _
from .commons import PdfBlock, TextBlock

logger = log.getLogger(__name__)


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


def pdf_filter_exec(
    batch_pages: List[str],
    i_batch_page: int,
    n_pages: int,
    pdf_filter_func: Callable[[List[str]], List[PdfBlock]],
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
    batch_results = []
    logger.propagate = False
    std_err_log = log.StreamHandler()
    page_format_log = LogFormatterWithPage(logger.parent.handlers[0].formatter)
    std_err_log.setFormatter(page_format_log)
    logger.addHandler(std_err_log)

    for page_number, page in enumerate(batch_pages, start=i_batch_page):
        page_results = []
        page_format_log.page = page_number
        if (page_number + i_batch_page) % (n_pages // min(10, n_pages)) == 0:
            logger.info(_("Still filtering..."))
        try:
            for r in pdf_filter_func(page):
                r.metadata["page"] = page_number
                page_results.append(r)
        except Exception as e:
            logger.error("fatal error in pdf filter")
            raise e
        batch_results.append(page_results)
    return batch_results


def text_extract_exec(
    pdf_blocks_batch: List[List[PdfBlock]],
    targets: List[str],
    text_extract_func: Callable[[List[PdfBlock], List[str]], List[TextBlock]],
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
    txt_blocks = None
    try:
        txt_blocks = [
            text_extract_func(pdf_blocks, targets) for pdf_blocks in pdf_blocks_batch
        ]
    except Exception as e:
        logger.error(_("Invalid text extraction!!"))
        raise e
    return txt_blocks


def deserialize_exec(
    text_blocks_batch: List[List[TextBlock]],
    targets: List[str],
    deserialize_func: Callable[
        [TextBlock, List[str]], FinancialData | PromisesResolutionContext
    ],
) -> List[FinancialData | PromisesResolutionContext]:
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
    return [
        deserialize_func(txtblk, targets)
        for (text_blocks) in text_blocks_batch
        for (txtblk) in text_blocks
    ]


def get_pipelines(format_name):
    pipelines = get_structured(format_name)
    pipelines += get_semistructured(format_name)
    pipelines += get_unstructured(format_name)
    return pipelines


def get_pipes(format_name):
    return tuple(zip(*get_pipelines(format_name)))


def get_pdf_filters(format_name):
    return get_pipes(format_name)[0]


def get_text_extract(format_name):
    return get_pipes(format_name)[1]


def get_deserialize(format_name):
    return get_pipes(format_name)[2]
