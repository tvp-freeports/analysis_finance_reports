"""This module is the one that contains the function called in order to decode the information
from the pdf and to save the `csv` file. This is also the source code to be launched
(providing the options with a `dotenv` file or with `env variables`) to mimic the behaviour of
the command from commandline (to use the package as a script).

Example:
    ```python main.py```

"""

import os
import re
import tarfile
import shutil
from lxml import etree
from multiprocess import Pool
import logging as log
from typing import List
import csv
import pymupdf as pypdf
import pandas as pd
from importlib_resources import files
from freeports_analysis import data
from freeports_analysis import download as dw
from freeports_analysis.consts import PdfFormats, _get_module, Equity, Currency
from freeports_analysis.formats import (
    pdf_filter_exec,
    text_extract_exec,
    deserialize_exec,
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


logger = log.getLogger(__name__)


class NoPDFormatDetected(Exception):
    """Exception that should rise when the script is not
    capable of detecting a PDF format to use to decode the
    report, and no explicit format is specified
    """


def pipeline_batch(
    batch_pages: List[str],
    i_page_batch: int,
    n_pages: int,
    targets: List[str],
    module_name: str,
) -> pd.DataFrame:
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
    end_page_batch = i_page_batch + len(batch_pages)
    logger.debug(
        "Starting batch [%i] starting form page %i to %i",
        os.getpid(),
        i_page_batch,
        end_page_batch,
    )
    parser = etree.XMLParser(recover=True)
    xml_roots = [etree.fromstring(page, parser=parser) for page in batch_pages]
    module = _get_module(module_name)
    logger.info(
        "Extracting relevant blocks of pdf from page %i to %i...",
        i_page_batch,
        end_page_batch,
    )
    pdf_blocks = pdf_filter_exec(xml_roots, i_page_batch, n_pages, module.pdf_filter)
    logger.info(
        "Filtering relevant blocks of text from page %i to %i...",
        i_page_batch,
        end_page_batch,
    )
    filtered_text = text_extract_exec(pdf_blocks, targets, module.text_extract)
    financtial_data = deserialize_exec(filtered_text, targets, module.deserialize)
    error_msg = "ERROR, SOMETHING WENT WRONG!!!!"
    df = pd.DataFrame(
        [
            fd.to_dict()
            if fd is not None
            else Equity(
                page=9999,
                targets=[error_msg],
                company=error_msg,
                subfund=None,
                nominal_quantity=None,
                market_value=None,
                perc_net_assets=0.0,
                currency=Currency.EUR,
            ).to_dict()
            for fd in financtial_data
        ]
    )
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


def get_targets() -> List[str]:
    """Read target names from a CSV file and return them as a list.

    Reads the first column of 'target.csv' (excluding header row) and returns
    the values as a list of strings. The file is expected to be in the package's
    data directory.

    Returns
    -------
    List[str]
        list of target names extracted from the CSV file.

    Raises
    ------
    FileNotFoundError
        If 'target.csv' doesn't exist in the data directory.
    IndexError
        If the CSV file is empty or malformed.
    """
    targets = []
    with files(data).joinpath("target.csv").open("r") as f:
        target_csv = csv.reader(f)
        targets = [row[0] for row in target_csv if row]  # Skip empty rows
        targets.pop(0)  # Remove header
    return targets


def _main_job(config, n_workers):
    logger.debug("Starting job [%i] with configuration %s", os.getpid(), str(config))
    format_selected = config["FORMAT"]
    detected_format = None
    if config["URL"] is None or config["PDF"] is not None and config["PDF"].exists():
        logger.debug("PDF: %s", config["PDF"])
        pdf_file = pypdf.Document(config["PDF"])
    else:
        for fmt in PdfFormats.__members__:
            for reg in PdfFormats.__members__[fmt].value:
                if bool(re.search(reg, config["URL"])):
                    detected_format = PdfFormats.__members__[fmt]
                    break
        log_string = "URL: %s/%s [detected %s format]"
        logger.debug(log_string, config["URL"], config["PDF"], detected_format.name)
        pdf_file = pypdf.Document(
            stream=dw.download_pdf(
                config["URL"], config["PDF"] if config["SAVE_PDF"] else None
            )
        )
    logger.debug("Starting decoding pdf to xml...")
    pdf_file_xml = [page.get_text("xml").encode() for page in pdf_file]
    logger.debug("End decoding pdf to xml!")
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
    targets = get_targets()
    log_string = str(targets[: min(5, len(targets))])
    logger.debug("First 5 targets: %s", log_string)

    n_pages = len(pdf_file_xml)
    batch_size = (n_pages + n_workers - 1) // n_workers
    batches = []

    for i in range(n_workers):
        start_idx = i * batch_size
        end_idx = min((i + 1) * batch_size, n_pages)
        batch_pages = pdf_file_xml[start_idx:end_idx]
        batches.append((batch_pages, start_idx + 1, n_pages, targets, format_pdf.name))

    with Pool(processes=n_workers) as pool:
        results = pool.starmap(pipeline_batch, batches)
    df = pd.concat(results, ignore_index=True)

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
    n_w_set = config["N_WORKERS"]
    n_workers = n_w_set if n_w_set is not None and n_w_set >= 0 else None
    if n_workers is None:
        n_workers = os.cpu_count()
    if config["BATCH"] is None:
        _main_job(config, n_workers)
    else:
        config_jobs = batch_job_confs(config)
        args = [(c, 1) for c in config_jobs]
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
            p.starmap(_main_job, args)

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
