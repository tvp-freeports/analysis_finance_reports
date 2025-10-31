"""Module containing useful definitions for program logging configuration.

This module provides custom logging formatters, filters, and configuration
for the standard `logging` Python package, tailored for PDF processing and
financial data extraction workflows.
"""

import logging
from typing import Any, Optional, Union, Dict
import pandas as pd
from freeports_analysis.i18n import _


class DevDebugFormatter(logging.Formatter):
    """Formatter class for comprehensive debug logging with full location information.

    This formatter is useful for debugging purposes as it provides detailed
    information about code location and PDF document position.

    Attributes
    ----------
    None
        Inherits all attributes from logging.Formatter
    """

    def format(self, record: logging.LogRecord) -> str:
        """Format log record with full information on code and PDF document location.

        Parameters
        ----------
        record : logging.LogRecord
            The log record to format

        Returns
        -------
        str
            Formatted debug message with comprehensive location information

        Notes
        -----
        The formatted output includes:
        - Process ID (if multiprocessing)
        - Log level and source location (function, line, file)
        - Report and page context
        - PDF document coordinates and position references
        - The actual log message
        """
        debug_msg = f"[{record.process}] " if getattr(record, "mproc", False) else ""
        debug_msg += "=" * 70 + "\n"
        debug_msg += (
            f'{record.levelname} from "{record.funcName}", '
            + f"line {record.lineno} of {record.pathname}\n"
        )
        report_str = (
            f"Report {record.report} "
            if getattr(record, "report", None) is not None
            else ""
        )
        page_str = (
            f"page {record.page} " if getattr(record, "page", None) is not None else ""
        )
        locate_str = (
            f"in {record.horizontal_ref} "
            if getattr(record, "horizontal_ref", None) is not None
            else ""
        )
        locate_str += (
            f"of {record.vertical_ref} "
            if getattr(record, "vertical_ref", None) is not None
            else ""
        )
        coordinates = (
            "\t["
            if getattr(record, "c2", None) is not None
            or getattr(record, "c1", None) is not None
            else ""
        )
        coordinates += (
            f"c1={record.c1}" if getattr(record, "c1", None) is not None else ""
        )
        coordinates += (
            ","
            if getattr(record, "c2", None) is not None
            and getattr(record, "c1", None) is not None
            else ""
        )
        coordinates += (
            f"c2={record.c2}" if getattr(record, "c2", None) is not None else ""
        )
        coordinates += (
            "]"
            if getattr(record, "c2", None) is not None
            or getattr(record, "c1", None) is not None
            else ""
        )
        line_location = report_str + page_str + locate_str + coordinates
        debug_msg += line_location + "\n" if line_location != "" else ""
        debug_msg += record.getMessage()
        return debug_msg


class StderrFormatter(logging.Formatter):
    """Formatter class for concise single-line logging suitable for live streams like stderr.

    This formatter produces compact log messages ideal for real-time monitoring
    and command-line output.

    Attributes
    ----------
    None
        Inherits all attributes from logging.Formatter
    """

    def format(self, record: logging.LogRecord) -> str:
        """Format the log as a concise one-line message for live stream display.

        Parameters
        ----------
        record : logging.LogRecord
            The log record to format

        Returns
        -------
        str
            Compact formatted log message

        Notes
        -----
        The formatted output includes minimal context:
        - Process ID (if multiprocessing)
        - Log level
        - Report and page context (if available)
        - Position references (if available)
        - The actual log message
        """
        log_msg = f"[{record.process}] " if getattr(record, "mproc", False) else ""
        log_msg += f"{record.levelname} "
        log_msg += (
            "{"
            if getattr(record, "report", None) is not None
            or getattr(record, "page", None) is not None
            else ""
        )
        log_msg += (
            f"{record.report}" if getattr(record, "report", None) is not None else ""
        )
        log_msg += (
            " "
            if getattr(record, "report", None) is not None
            and getattr(record, "page", None) is not None
            else ""
        )
        log_msg += (
            f"pag.{record.page}" if getattr(record, "page", None) is not None else ""
        )
        log_msg += (
            "} "
            if getattr(record, "report", None) is not None
            or getattr(record, "page", None) is not None
            else ""
        )
        log_msg += (
            f"in {record.horizontal_ref} "
            if getattr(record, "horizontal_ref", None) is not None
            else ""
        )
        log_msg += (
            " "
            if getattr(record, "horizontal_ref", None) is not None
            and getattr(record, "vertical_ref", None) is not None
            else ""
        )
        log_msg += (
            f"of {record.vertical_ref} "
            if getattr(record, "vertical_ref", None) is not None
            else ""
        )
        log_msg = log_msg.strip() + f": {record.getMessage()}"

        return log_msg


class CsvFormatter(logging.Formatter):
    """Formatter for CSV file logging with PDF document position information.

    This formatter produces CSV-formatted log entries containing information
    primarily related to PDF document positions, making it suitable for user
    review rather than developer debugging.

    Attributes
    ----------
    None
        Inherits all attributes from logging.Formatter
    """

    def format(self, record: logging.LogRecord) -> str:
        """Format the log record as CSV with PDF position information.

        Parameters
        ----------
        record : logging.LogRecord
            The log record to format

        Returns
        -------
        str
            CSV-formatted log entry

        Notes
        -----
        The CSV output includes fields for:
        - Page number
        - Row and column coordinates
        - Log message
        - Report identifier (in batch mode)
        """

        if getattr(record, "vertical_ref", None) is not None:
            vertical_ref = record.vertical_ref.split("[")
            company = vertical_ref[-1].replace("]", "").strip()
            company_match = " ".join(vertical_ref[:-1]).strip()
            if company == "":
                company = None
            if company_match == "":
                company_match = None

        fields = {
            "page": record.page if getattr(record, "page", None) is not None else "",
            "company_match": company_match if company_match is not None else "",
            "company": company if company is not None else "",
            "field_name": record.horizontal_ref
            if getattr(record, "horizontal_ref", None) is not None
            else "",
            "row": record.c1 if getattr(record, "c1", None) is not None else "",
            "col": record.c2 if getattr(record, "c2", None) is not None else "",
            "message": record.getMessage(),
        }

        if getattr(record, "batch_mode", False):
            fields = {
                "report": record.report
                if getattr(record, "report", None) is not None
                else ""
            } | fields

        return (
            pd.DataFrame([fields])
            .to_csv(header=False, index=False)
            .strip()
            .replace("\n", "\\n")
        )


def _set_if_not_exists(a: Any, b: Any, field: str) -> Any:
    """Copy attribute from source to target if it doesn't exist in target.

    If attribute `field` doesn't exist in object `b`, it copies it from object `a`.

    Parameters
    ----------
    a : Any
        Source object to copy attributes from
    b : Any
        Target object to copy attributes to
    field : str
        Name of the attribute to check and copy

    Returns
    -------
    Any
        The target object with potentially updated attributes

    Notes
    -----
    This utility function is used by logging filters to ensure log records
    have all required contextual attributes, copying them from filter state
    if they are missing from the log record.
    """
    try:
        getattr(b, field)
    except AttributeError:
        setattr(b, field, getattr(a, field))
    return b


class AddContextualInfos(logging.Filter):
    """Filter that adds contextual state information to LogRecords.

    This filter maintains state about the processing context and adds this
    information to log records for better traceability and debugging.

    Attributes
    ----------
    mproc : bool
        If True, indicates multiprocess mode (requires PID identification)
    batch_mode : bool
        If True, indicates batch processing mode (requires report identification)
    page : Optional[int]
        The PDF page number being parsed
    report : Optional[str]
        The identifier of the specific PDF report (uses PREFIX_OUT)
    vertical_ref : Optional[str]
        Textual hint for vertical position in page (e.g., company name)
    horizontal_ref : Optional[str]
        Textual hint for horizontal position in page
    c1 : Optional[Union[float, int]]
        First coordinate for precise position identification
    c2 : Optional[Union[float, int]]
        Second coordinate for precise position identification
    """

    mproc: bool = False
    batch_mode: bool = False
    page: Optional[int] = None
    report: Optional[str] = None
    vertical_ref: Optional[str] = None
    horizontal_ref: Optional[str] = None
    c1: Optional[Union[float, int]] = None
    c2: Optional[Union[float, int]] = None

    def filter(self, record: logging.LogRecord) -> logging.LogRecord:
        """Add contextual information from filter state to the LogRecord.

        Parameters
        ----------
        record : logging.LogRecord
            The log record to enrich with contextual information

        Returns
        -------
        logging.LogRecord
            The enriched log record with contextual information
        """
        record.mproc = self.mproc
        record.batch_mode = self.batch_mode
        for field in ["page", "report", "vertical_ref", "horizontal_ref", "c1", "c2"]:
            _set_if_not_exists(self, record, field)
        return record


LOG_CONTEXTUAL_INFOS = AddContextualInfos()


class AdaptStandardInvesmentInfos(logging.Filter):
    """Filter that adds algorithm-specific investment information to LogRecords.

    This filter adds information specific to the standard PDF processing algorithms
    (`standard_pdf_filtering`, `standard_text_extracting`, `standard_deserialization`)
    and converts it to a less algorithm-dependent format suitable for the
    `AddContextualInfos` filter.

    Attributes
    ----------
    company : Optional[str]
        The company being parsed as recognized by the algorithm
    company_match : Optional[str]
        The company being parsed as written in the PDF document
    field : Optional[str]
        The field being parsed
    row : Optional[int]
        The row number in the body table (investment table)
    col : Optional[int]
        The column number in the body table
    """

    company: Optional[str] = None
    company_match: Optional[str] = None
    field: Optional[str] = None
    row: Optional[int] = None
    col: Optional[int] = None

    def filter(self, record: logging.LogRecord) -> logging.LogRecord:
        """Add investment-specific information and convert to contextual format.

        Parameters
        ----------
        record : logging.LogRecord
            The log record to enrich with investment information

        Returns
        -------
        logging.LogRecord
            The enriched log record with investment context
        """
        for field in ["company", "company_match", "field", "row", "col"]:
            _set_if_not_exists(self, record, field)

        company = record.company if getattr(record, "company", None) is not None else ""
        company_match = (
            record.company_match
            if getattr(record, "company_match", None) is not None
            else ""
        )
        record.vertical_ref = (
            f"{company} [{company_match.replace(chr(10), '\\n').strip()}]"
        )
        record.horizontal_ref = record.field
        record.c1 = record.row
        record.c2 = record.col
        return record


LOG_ADAPT_INVESTMENT_INFOS = AdaptStandardInvesmentInfos()

HANDLER_STDERR = logging.StreamHandler()
HANDLER_STDERR.addFilter(LOG_CONTEXTUAL_INFOS)
HANDLER_STDERR.setFormatter(StderrFormatter())

logging.getLogger().addHandler(HANDLER_STDERR)
LOGGING_TABLE = logging.getLogger("logging_table")
LOGGING_STDERR = logging.getLogger("stderr")


def log_config(
    logger: logging.Logger, config: Dict[str, Any], config_location: Dict[str, str]
) -> None:
    """Log with debug priority the configuration provided.

    Parameters
    ----------
    logger : logging.Logger
        the logger that has to log
    config : Dict[str, Any]
        The configuration dictionary to log
    config_location : Dict[str, str]
        Dictionary mapping configuration keys to their source locations
    """
    locations = {"DEFAULT": [], "CONFIG_FILE": [], "ENV_VAR": [], "CMD_ARG": []}
    for k, v in config_location.items():
        if v == "FreeportsDefaultConfig":
            locations["DEFAULT"].append(k)
        elif v == "FreeportsFileConfig":
            locations["CONFIG_FILE"].append(k)
        elif v == "FreeportsEnvConfig":
            locations["ENV_VAR"].append(k)
        elif v == "FreeportsCmdConfig":
            locations["CMD_ARG"].append(k)
        else:
            raise ValueError(_("Unknown config location: {}").format(v))
    logger.debug(_("Resulting config: %s"), config)
    logger.debug(_("Resulting location: %s"), locations)
