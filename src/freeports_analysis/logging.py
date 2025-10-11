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
        return pd.DataFrame([fields]).to_csv(header=False, index=False).strip()


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
