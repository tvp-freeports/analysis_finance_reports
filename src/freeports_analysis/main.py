"""This module is the one that contains the function called in order to decode the information
from the pdf and to save the `csv` file. This is also the source code to be launched
(providing the options with a `dotenv` file or with `env variables`) to mimic the behaviour of
the command from commandline (to use the package as a script).

Example:
    ```python main.py```

"""

import os
import re
import importlib
import tarfile
import shutil
from multiprocessing import Pool
import logging as log
from typing import List
import csv
import pymupdf as pypdf
import pandas as pd
from importlib_resources import files
from freeports_analysis import data
from freeports_analysis import download as dw
from freeports_analysis.consts import PDF_Formats
from freeports_analysis.formats import (
    pdf_filter_exec,
    text_extract_exec,
    tabularize_exec,
)
from freeports_analysis.conf_parse import (
    apply_config,
    log_config,
    get_config_file,
    DEFAULT_CONFIG,
    DEFAULT_LOCATION_CONFIG,
    validate_conf,
    schema_job_csv_config,
)

__all__ = ["NoPDFormatDetected", "get_functions"]

logger = log.getLogger(__name__)


class NoPDFormatDetected(Exception):
    """Exception that should rise when the script is not
    capable of detecting a PDF format to use to decode the
    report, and no explicit format is specified
    """


def get_functions(format_pdf: PDF_Formats):
    """Set wrapper functions `PDF_FILTER`, `TEXT_EXTRACT` and `TABULARIZE` to use
    implementation of specific PDF format

    Parameters
    ----------
    format : PDF_Formats
        The format detected used to choose the decoding implementation
    """
    module_name = format_pdf.name
    try:
        module = importlib.import_module(
            f"freeports_analysis.formats.{module_name}", package=__package__
        )
    except ImportError:
        print(f"Errore: modulo {module_name} non trovato")
        raise

    def _pdf_filter(pdf_file):
        return pdf_filter_exec(pdf_file, module.pdf_filter)

    def _text_extract(pdf_blks, targets):
        return text_extract_exec(pdf_blks, targets, module.text_extract)

    def _tabularize(txt_blks):
        return tabularize_exec(txt_blks, module.tabularize)

    return {
        "PDF_FILTER": _pdf_filter,
        "TEXT_EXTRACT": _text_extract,
        "TABULARIZE": _tabularize,
    }


def pipeline(pdf_file: pypdf.Document, targets: List[str], funcs: dict) -> pd.DataFrame:
    """Apply the pipeline of actions in order to get data in `csv`

    Parameters
    ----------
    pdf_file : pypdf.Document
        `pdf` document to process in the format used in the python package `pymupdf`
    targets : List[str]
        the list of relevant companies in the report from which data is relevant
    funcs : dict
        the dictionary containing the functions to use in order to parse the pdf

    Returns
    -------
    pd.DataFrame
        pandas dataframe with extracted data
    """
    logger.info("Extracting relevant blocks of pdf...")
    pdf_blocks = funcs["PDF_FILTER"](pdf_file)
    logger.info("Extracted!")

    logger.info("Filtering relevant blocks of text...")
    filtered_text = funcs["TEXT_EXTRACT"](pdf_blocks, targets)
    logger.info("Filtered!")

    tabular_data = funcs["TABULARIZE"](filtered_text)
    df = pd.DataFrame(tabular_data)
    return df


def batch_job_confs(config: dict) -> List[dict]:
    """Create a list of configurations overwritten after reading
    a batch file with job contextual options

    Parameters
    ----------
    config : dict
        configuration to overwrite

    Returns
    -------
    List[dict]
        list of configurations
    """
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
    format_selected = config["FORMAT"]

    detected_format = None
    if config["URL"] is None or config["PDF"] is not None and config["PDF"].exists():
        logger.debug("PDF: %s", config["PDF"])
        pdf_file = pypdf.Document(config["PDF"])
    else:
        for fmt in PDF_Formats.__members__:
            for reg in PDF_Formats.__members__[fmt].value:
                if bool(re.search(reg, config["URL"])):
                    detected_format = PDF_Formats.__members__[fmt]
                    break
        log_string = "URL: %s/%s [detected %s format]"
        logger.debug(log_string, config["URL"], config["PDF"], detected_format.name)
        pdf_file = pypdf.Document(
            stream=dw.download_pdf(
                config["URL"], config["PDF"] if config["SAVE_PDF"] else None
            )
        )

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
    log_string = str(targets[: min(5, len(targets))])
    logger.debug("First 5 targets: %s", log_string)

    funcs = get_functions(format_pdf)
    df = pipeline(pdf_file, targets, funcs)
    if config["OUT_CSV"].name.endswith(".tar.gz"):
        config["OUT_CSV"] = config["OUT_CSV"].with_suffix("").with_suffix("")
    prefix_csv = config.get("PREFIX_OUT_CSV")
    if prefix_csv is None:
        df.to_csv(config["OUT_CSV"])
    else:
        name_file = f"{format_pdf.name}.csv"
        if prefix_csv is not None and prefix_csv != "":
            name_file = f"{prefix_csv}-{format_pdf.name}.csv"
        df.to_csv(config["OUT_CSV"] / name_file)


def main(config):
    """Main function that expect the configuration to be already provided
    (for example with arguments on command line or with `env variables`)

    Raises
    ------
    NoPDFormatDetected
        if no explicit format is provided and an url is not provided
        or not associated with any format the program cannot choose a way to
        decode the pdf, so it raises this exception
    """
    if config["BATCH"] is None:
        _main_job(config)
    else:
        config_jobs = batch_job_confs(config)
        n_w_set = config["BATCH_WORKERS"]
        n_workers = n_w_set if n_w_set is not None and n_w_set >= 0 else None
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
    current_config = DEFAULT_CONFIG
    config_location = DEFAULT_LOCATION_CONFIG
    LOG_LEVEL = (5 - current_config["VERBOSITY"]) * 10
    log.basicConfig(level=LOG_LEVEL)
    current_config, config_location = get_config_file(current_config, config_location)
    current_config, config_location = apply_config(current_config, config_location)
    LOG_LEVEL = (5 - current_config["VERBOSITY"]) * 10
    log.getLogger().setLevel(LOG_LEVEL)
    log_config(logger, current_config, config_location)
    validate_conf(current_config)
    main(current_config)
