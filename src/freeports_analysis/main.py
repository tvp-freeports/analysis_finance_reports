import os
import re
import pymupdf as pypdf
from . import download as dw
import logging as log
from typing import Optional, List
from .consts import ENV_PREFIX, PDF_Formats
import csv
import pandas as pd
from importlib_resources import files
from . import data
from .formats import pdf_filter_exec, text_extract_exec, tabularize_exec
import importlib

logger = log.getLogger(__name__)


class NoPDFormatDetected(Exception):
    pass


PDF_FILTER = None
TEXT_EXTRACT = None
TABULARIZE = None


def get_functions(format: PDF_Formats):
    module_name = format.name
    try:
        module = importlib.import_module(f".formats.{module_name}", package=__package__)
    except ImportError:
        print(f"Errore: modulo {module_name} non trovato")
        raise
    global PDF_FILTER
    global TEXT_EXTRACT
    global TABULARIZE
    PDF_FILTER = lambda pdf_file: pdf_filter_exec(pdf_file, module.pdf_filter)
    TEXT_EXTRACT = lambda pdf_blocks, target: text_extract_exec(
        pdf_blocks, target, module.text_extract
    )
    TABULARIZE = lambda text_blocks: tabularize_exec(text_blocks, module.tabularize)


def pipeline(pdf_file: pypdf.Document, targets: List[str]):
    logger.info("Extracting relevant blocks of pdf...")
    pdf_blocks = PDF_FILTER(pdf_file)
    logger.info("Extracted!")

    logger.info("Filtering relevant blocks of text...")
    filtered_text = TEXT_EXTRACT(pdf_blocks, targets)
    logger.info("Filtered!")

    tabular_data = TABULARIZE(filtered_text)
    df = pd.DataFrame(tabular_data)
    return df


def main(save_pdf: bool, format_selected: Optional[PDF_Formats]):
    pdf = os.getenv(f"{ENV_PREFIX}PDF")
    url = os.getenv(f"{ENV_PREFIX}URL")
    detected_format = None
    if url is None or os.path.exists(pdf):
        logger.debug("PDF: %s", pdf)
        pdf_file = pypdf.Document(pdf)
    else:
        for fmt in PDF_Formats.__members__:
            for reg in PDF_Formats.__members__[fmt].value:
                if bool(re.search(reg, url)):
                    detected_format = PDF_Formats.__members__[fmt]
                    break
        logger.debug("URL: %s/%s", url, pdf)
        pdf_file = pypdf.Document(stream=dw.download_pdf(url, pdf, save_pdf))

    targets = []
    with files(data).joinpath("target.csv").open("r") as f:
        target_csv = csv.reader(f)
        targets = [row[0] for row in target_csv]
        targets.pop(0)
    logger.debug("First 5 targets: %s", str(targets[: min(5, len(targets))]))
    if detected_format is None and format_selected is None:
        raise NoPDFormatDetected(
            "No format selected and url doesn't match know formats"
        )
    if detected_format is not None and format_selected != detected_format:
        logger.warning(
            "Detected and selected formats don't match [det=%f sel=%s]",
            detected_format.name,
            format_selected.name,
        )
    format_pdf = detected_format if format_selected is None else format_selected

    get_functions(format_pdf)
    df = pipeline(pdf_file, targets)
    df.to_csv(os.getenv(f"{ENV_PREFIX}OUT_CSV"))


if __name__ == "__main__":
    import dotenv

    dotenv.load_dotenv()
    log_level = (5 - int(os.getenv(f"{ENV_PREFIX}VERBOSITY"))) * 10
    log.basicConfig(level=log_level)
    save_pdf = os.getenv(f"{ENV_PREFIX}SAVE_PDF") is not None
    wanted_format = os.getenv(f"{ENV_PREFIX}PDF_FORMAT")
    format_selected = (
        PDF_Formats.__members__[wanted_format] if wanted_format is not None else None
    )
    main(save_pdf, format_selected)
