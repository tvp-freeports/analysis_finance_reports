"""This module is the one that contains the function called in order to decode the information
from the pdf and to save the `csv` file. This is also the source code to be launched
(providing the options with a `dotenv` file or with `env variables`) to mimic the behaviour of
the command from commandline (to use the package as a script).

Example:
    ```python main.py```

"""

import os
import re
from multiprocessing import Pool
import pymupdf as pypdf
from freeports_analysis import download as dw
import logging as log
from typing import Optional, List
from freeports_analysis.consts import ENV_PREFIX, PDF_Formats
import csv
from pathlib import Path
import pandas as pd
import tarfile
import shutil
from importlib_resources import files
from freeports_analysis import data
from freeports_analysis.formats import (
    pdf_filter_exec,
    text_extract_exec,
    tabularize_exec,
)
from freeports_analysis.conf_parse import (
    apply_config,
    log_resultig_config,
    get_config_file,
    RESULTING_CONFIG,
    RESULTING_LOCATION_CONFIG,
    validate_conf,
    schema_job_csv_config,
)
import importlib

__all__ = ["NoPDFormatDetected", "get_functions"]

logger = log.getLogger(__name__)


class NoPDFormatDetected(Exception):
    """Exception that should rise when the script is not
    capable of detecting a PDF format to use to decode the
    report, and no explicit format is specified
    """

    pass


PDF_FILTER = None
TEXT_EXTRACT = None
TABULARIZE = None


def get_functions(format: PDF_Formats):
    """Set wrapper functions `PDF_FILTER`, `TEXT_EXTRACT` and `TABULARIZE` to use
    implementation of specific PDF format

    Parameters
    ----------
    format : PDF_Formats
        The format detected used to choose the decoding implementation
    """
    module_name = format.name
    try:
        module = importlib.import_module(
            f"freeports_analysis.formats.{module_name}", package=__package__
        )
    except ImportError:
        print(f"Errore: modulo {module_name} non trovato")
        raise

    global PDF_FILTER
    global TEXT_EXTRACT
    global TABULARIZE

    def PDF_FILTER(pdf_file):
        return pdf_filter_exec(pdf_file, module.pdf_filter)

    def TEXT_EXTRACT(pdf_blocks, target):
        return text_extract_exec(pdf_blocks, target, module.text_extract)

    def TABULARIZE(text_blocks):
        return tabularize_exec(text_blocks, module.tabularize)


def pipeline(pdf_file: pypdf.Document, targets: List[str]) -> pd.DataFrame:
    """Apply the pipeline of actions in order to get data in `csv`

    Parameters
    ----------
    pdf_file : pypdf.Document
        `pdf` document to process in the format used in the python package `pymupdf`
    targets : List[str]
        the list of relevant companies in the report from which data is relevant

    Returns
    -------
    pd.DataFrame
        pandas dataframe with extracted data
    """
    logger.info("Extracting relevant blocks of pdf...")
    pdf_blocks = PDF_FILTER(pdf_file)
    logger.info("Extracted!")

    logger.info("Filtering relevant blocks of text...")
    filtered_text = TEXT_EXTRACT(pdf_blocks, targets)
    logger.info("Filtered!")

    tabular_data = TABULARIZE(filtered_text)
    df = pd.DataFrame(tabular_data)
    return df


def batch_job_confs(config):
    rows = None
    with config["BATCH"].open(newline="", encoding="UTF-8") as csvfile:
        rows = csv.DictReader(csvfile)
        result = [
            config
            | {
                k: cast(v)
                for h, v in r.items()
                for k, cast in [schema_job_csv_config[h.strip().lower()]]
            }
            for r in rows
        ]
    return result


def _main_job(config):
    logger.debug("Starting job [%i] with configuration %s", os.getpid(), str(config))
    save_pdf = config["SAVE_PDF"]
    format_selected = config["FORMAT"]
    pdf = config["PDF"]
    url = config["URL"]
    out_csv = config["OUT_CSV"]

    detected_format = None
    if url is None or pdf.exists():
        logger.debug("PDF: %s", pdf)
        pdf_file = pypdf.Document(pdf)
    else:
        for fmt in PDF_Formats.__members__:
            for reg in PDF_Formats.__members__[fmt].value:
                if bool(re.search(reg, url)):
                    detected_format = PDF_Formats.__members__[fmt]
                    break
        logger.debug("URL: %s/%s [detected %s format]", url, pdf, detected_format.name)
        pdf_file = pypdf.Document(stream=dw.download_pdf(url, pdf, save_pdf))

    if detected_format is None and format_selected is None:
        raise NoPDFormatDetected(
            "No format selected and url doesn't match know formats"
        )
    if (
        detected_format is not None
        and format_selected
        and format_selected is not None
        and format_selected != detected_format
    ):
        logger.warning(
            "Detected and selected formats don't match [det=%f sel=%s]",
            detected_format.name,
            format_selected.name,
        )

    format_pdf = detected_format if format_selected is None else format_selected
    logger.debug("Using %s format", format_pdf.name)
    targets = []
    with files(data).joinpath("target.csv").open("r") as f:
        target_csv = csv.reader(f)
        targets = [row[0] for row in target_csv]
        targets.pop(0)
    logger.debug("First 5 targets: %s", str(targets[: min(5, len(targets))]))

    get_functions(format_pdf)
    df = pipeline(pdf_file, targets)
    if out_csv.name.endswith(".tar.gz"):
        out_csv = out_csv.with_suffix("").with_suffix("")
    prefix_csv = config.get("PREFIX_OUT_CSV")
    if prefix_csv is None:
        df.to_csv(out_csv)
    else:
        df.to_csv(out_csv / f"{prefix_csv}-{format_pdf.name}.csv")


def main(config):
    """Main function that expect the configuration to be already provided
    (for example with arguments on command line or with `env variables`)

    Raises
    ------
    NoPDFormatDetected
        if no explicit format is provided through the command line or `env variables` or other methods
        and an url is not provided or not associated with any format the program cannot choose a way to
        decode the pdf, so it raise this exception
    """
    if config["BATCH"] is None:
        _main_job(config)
    else:
        config_jobs = batch_job_confs(config)
        n_workers = (
            config["BATCH_WORKERS"]
            if config["BATCH_WORKERS"] is not None and config["BATCH_WORKERS"] >= 0
            else None
        )
        out_csv = config["OUT_CSV"]
        out_dir = out_csv
        compress = False
        remove_dir = False
        if out_csv.name.endswith(".tar.gz"):
            compress = True
            out_dir = out_csv.with_suffix("").with_suffix("")
        if not out_dir.exists() and compress:
            remove_dir = True

        out_dir.mkdir(exist_ok=True)
        with Pool(n_workers) as p:
            p.map(_main_job, config_jobs)

        if compress:
            with tarfile.open(out_csv, "w:gz") as tar:
                tar.add(out_dir, arcname=out_dir.name)
            if remove_dir:
                shutil.rmtree(out_dir)


if __name__ == "__main__":
    global RESULTING_CONFIG
    global RESULTING_LOCATION_CONFIG
    log_level = (5 - RESULTING_CONFIG["VERBOSITY"]) * 10
    log.basicConfig(level=log_level)
    RESULTING_CONFIG, RESULTING_LOCATION_CONFIG = get_config_file(
        RESULTING_CONFIG, RESULTING_LOCATION_CONFIG
    )
    RESULTING_CONFIG, RESULTING_LOCATION_CONFIG = apply_config(
        RESULTING_CONFIG, RESULTING_LOCATION_CONFIG
    )
    log_level = (5 - RESULTING_CONFIG["VERBOSITY"]) * 10
    log.getLogger().setLevel(log_level)
    log_resultig_config(logger)
    validate_conf(RESULTING_CONFIG)
    main(RESULTING_CONFIG, RESULTING_LOCATION_CONFIG)
