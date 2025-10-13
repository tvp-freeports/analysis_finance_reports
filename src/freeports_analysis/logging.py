import logging
import logging.config as config
import pandas as pd


class DevDebugFormatter(logging.Formatter):
    def format(self, log_record):
        debug_msg = f"[{log_record.process}] " if log_record._mproc else ""
        debug_msg += "=" * 70 + "\n"
        debug_msg += f'{log_record.levelname} from "{log_record.funcName}", line {log_record.lineno} of {log_record.pathname}\n'
        report_str = (
            f"Report {log_record.report} " if log_record.report is not None else ""
        )
        page_str = f"page {log_record.page} " if log_record.page is not None else ""
        locate_str = (
            f"in {log_record.horizontal_ref} "
            if log_record.horizontal_ref is not None
            else ""
        )
        locate_str += (
            f"of {log_record.vertical_ref} "
            if log_record.vertical_ref is not None
            else ""
        )
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
        debug_msg += log_record.getMessage()
        return debug_msg


class StderrFormatter(logging.Formatter):
    def format(self, log_record):
        log_msg = f"[{log_record.process}] " if log_record._mproc else ""
        log_msg += f"{log_record.levelname} "
        log_msg += (
            "{" if log_record.report is not None or log_record.page is not None else ""
        )
        log_msg += f"{log_record.report}" if log_record.report is not None else ""
        log_msg += (
            " " if log_record.report is not None and log_record.page is not None else ""
        )
        log_msg += f"pag.{log_record.page}" if log_record.page is not None else ""
        log_msg += (
            "} " if log_record.report is not None or log_record.page is not None else ""
        )
        log_msg += (
            f"in {log_record.horizontal_ref} "
            if log_record.horizontal_ref is not None
            else ""
        )
        log_msg += (
            " "
            if log_record.horizontal_ref is not None
            and log_record.vertical_ref is not None
            else ""
        )
        log_msg += (
            f"of {log_record.vertical_ref} "
            if log_record.vertical_ref is not None
            else ""
        )
        log_msg = log_msg.strip() + f": {log_record.getMessage()}"

        return log_msg


class CsvFormatter(logging.Formatter):
    def format(self, log_record):
        try:
            matched_company = log_record.matched_company
            company = log_record.company
        except AttributeError:
            pass
        try:
            field_name = log_record.field_name
        except AttributeError:
            pass
        company = ""
        company_match = ""
        if log_record.vertical_ref is not None:
            vertical_ref = log_record.vertical_ref.split()
            company = vertical_ref[-1]
            company_match = " ".join(vertical_ref[:-1])
        fields = {
            "page": log_record.page if log_record.page is not None else "",
            "company_match": company_match,
            "company": company,
            "field_name": log_record.horizontal_ref
            if log_record.horizontal_ref is not None
            else "",
            "message": log_record.getMessage(),
        }
        if log_record._batch_mode:
            fields = {
                "report": log_record.report if log_record.report is not None else ""
            } | fields
        return (
            pd.DataFrame([fields])
            .to_csv(header=False, index=False)
            .strip()
            .replace("\n", "\\n")
        )


class AddContextualInfos(logging.Filter):
    mproc = False
    batch_mode = False
    page = None
    report = None
    vertical_ref = None
    horizontal_ref = None
    c1 = None
    c2 = None

    def filter(self, log_record):
        log_record._mproc = self.mproc
        log_record._batch_mode = self.batch_mode

        def _set_if_not_exists(a, b, field):
            try:
                getattr(b, field)
            except AttributeError:
                setattr(b, field, getattr(a, field))

        for field in ["page", "report", "vertical_ref", "horizontal_ref", "c1", "c2"]:
            _set_if_not_exists(self, log_record, field)
        return log_record


LOG_CONTEXTUAL_INFOS = AddContextualInfos()


HANDLER_STDERR = logging.StreamHandler()
HANDLER_STDERR.addFilter(LOG_CONTEXTUAL_INFOS)
HANDLER_STDERR.setFormatter(StderrFormatter())


logging.getLogger().addHandler(HANDLER_STDERR)
LOGGING_TABLE = logging.getLogger("logging_table")
LOGGING_STDERR = logging.getLogger("stderr")
