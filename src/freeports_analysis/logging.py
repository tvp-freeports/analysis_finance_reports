import logging
import logging.config as config
import pandas as pd

_basefmt = '{pid_str}%(levelname)s from "%(funcName)s" line %(lineno)d (%(pathname)s):\n{pid_str}%(message)s'
FORMAT_SP = _basefmt.format(pid_str="")
FORMAT_MP = _basefmt.format(pid_str="[%(process)d] ")
config.dictConfig(
    {
        "version": 1,
        "formatters": {"source": {"format": FORMAT_SP}},
        "handlers": {"stderr": {"class": logging.StreamHandler, "formatter": "source"}},
        "loggers": {
            "freeports_analysis.logging_table": {},
            "freeports_analysis": {"propagate": False, "handlers": ["stderr"]},
        },
    }
)


HANDLER_STDERR = logging.getHandlerByName("stderr")
FORMATTER_SOURCE = HANDLER_STDERR.formatter
FORMATTER_SOURCE_MP = logging.Formatter(fmt=FORMAT_MP)


class DevDebugFormatter(logging.Formatter):
    mproc = None

    def __init__(self, batch_mode: bool, multi_process: bool):
        super().__init__()
        self._batch_mode = batch_mode
        self.mproc = multi_process

    def format(self, log_record):
        debug_msg = "[%(process)d] " if self.mproc else ""
        debug_msg += "=" * 30 + "\n"
        debug_msg += (
            '%(levelname)s from "%(funcName)s", line %(lineno)d of %(pathname)s\n'
        )
        report_str = "Report %(report)s " if log_record.report != "" else ""
        page_str = "page %(page)d " if log_record.page != 0 else ""
        locate_str = "in %(horizontal_ref)s " if log_record.horizontal_ref != "" else ""
        locate_str += "of %(vertical_ref)s " if log_record.vertical_ref != "" else ""
        coordinates = (
            "\t[" if log_record.c2 is not None or log_record.c1 is not None else ""
        )
        coordinates += "c1=%(c1)d" if log_record.c1 is not None else ""
        coordinates += (
            "," if log_record.c2 is not None and log_record.c1 is not None else ""
        )
        coordinates += "c2=%(c2)d" if log_record.c2 is not None else ""
        coordinates += (
            "]" if log_record.c2 is not None or log_record.c1 is not None else ""
        )
        line_location = report_str + page_str + locate_str + coordinates
        debug_msg += line_location + "\n" if line_location != "" else ""
        debug_msg += "%(message)s"
        return debug_msg


class StderrFormatter(logging.Formatter):
    mproc = None

    def __init__(self, batch_mode: bool, multi_process: bool):
        super().__init__()
        self._batch_mode = batch_mode
        self.mproc = multi_process

    def format(self, log_record):
        debug_msg = ""
        debug_msg += "[%(report)s] " if log_record.report is not None else ""
        debug_msg += "%(levelname)s "
        debug_msg += "{pag.%(page)d} " if log_record.page is not 0 else ""
        debug_msg += (
            "in %(horizontal_ref)s " if log_record.horizontal_ref is not None else ""
        )
        debug_msg += (
            "of %(vertical_ref)s" if log_record.vertical_ref is not None else ""
        )


class CsvFormatter(logging.Formatter):
    def __init__(self, batch_mode: bool):
        super().__init__()
        self._batch_mode = batch_mode

    def format(self, log_record):
        field_name = ""
        matched_company = ""
        company = ""
        try:
            matched_company = log_record.matched_company
            company = log_record.company
        except AttributeError:
            pass
        try:
            field_name = log_record.field_name
        except AttributeError:
            pass

        fields = {
            "page": log_record.page,
            "matched_company": matched_company,
            "company": company,
            "field_name": field_name,
            "message": log_record.msg,
        }
        if self._batch_mode:
            fields = {"report": log_record.report} | fields
        return (
            pd.DataFrame([fields])
            .to_csv(header=False, index=False)
            .strip()
            .replace("\n", "\\n")
        )


class AddPageFilter(logging.Filter):
    page = 0

    def filter(self, log_record):
        log_record.page = self.page
        return log_record


class AddReportFilter(logging.Filter):
    report = ""

    def filter(self, log_record):
        log_record.report = self.report
        return log_record


PAGE_FILTER = AddPageFilter()
