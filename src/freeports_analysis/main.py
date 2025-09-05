"""This module is the one that contains the function called in order to decode the information
from the pdf and to save the `csv` file. This file is also the source code to be launched
(providing the options with a configuration file or with `env variables`) to mimic the behaviour of
from the pdf and to save the `csv` file. This file is also the source code to be launched
(providing the options with a configuration file or with `env variables`) to mimic the behaviour of
the command from commandline (to use the package as a script).

Example:
    ```python main.py```

"""

import os
import re
import tarfile
import shutil
import logging as log
from typing import List
from multiprocessing import Pool
import csv
from lxml import etree
import pymupdf as pypdf
import pandas as pd
from importlib_resources import files
from freeports_analysis.i18n import _
from freeports_analysis import data
from freeports_analysis.data import get_target_companies
from freeports_analysis import download as dw
from freeports_analysis.consts import (
    PdfFormats,
    Equity,
    Currency,
    FinancialData,
    PromisesResolutionContext,
    flatten_promise_map,
    STANDARD_LOG_FORMATTER,
    STANDARD_LOG_FORMATTER_MP,
)
from freeports_analysis.formats import (
    pdf_filter_exec,
    text_extract_exec,
    deserialize_exec,
    get_pipelines,
)
from freeports_analysis.conf_parse import (
    log_config,
    DEFAULT_CONFIG,
    DEFAULT_CONFIG_LOCATION,
    FreeportsEnvConfig,
    FreeportsFileConfig,
    FreeportsConfig,
    schema_job_csv_config,
)
from freeports_analysis.formats_data import url_to_format


logger = log.getLogger(__package__)
logger.propagate = False
stderr_log = log.StreamHandler()
stderr_log.setFormatter(STANDARD_LOG_FORMATTER)
logger.addHandler(stderr_log)


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
    format_name: str,
) -> List[FinancialData | PromisesResolutionContext]:
    """Apply the pipeline of actions in order to get financial data from PDF pages

    Parameters
    ----------
    batch_pages : List[str]
        List of XML strings representing PDF pages to process
    i_page_batch : int
        Starting page number of this batch (1-based index)
    n_pages : int
        Total number of pages in the document
    targets : List[str]
        List of relevant company names to extract from the report
    format_name : str
        Name of the format containing format-specific parsing functions

    Returns
    -------
    List[FinancialData | PromisesResolutionContext]
        List of extracted financial data objects or promise resolution contexts
    """
    end_page_batch = i_page_batch + len(batch_pages)
    logger.info(
        _("Starting batch form page %i to %i"),
        i_page_batch,
        end_page_batch,
    )
    parser = etree.XMLParser(recover=True)
    xml_roots = [etree.fromstring(page, parser=parser) for page in batch_pages]
    pipelines = get_pipelines(format_name)

    results = []
    for pipeline_name, pipeline in pipelines.items():
        (pdf_filter_funcs, text_extract_funcs, deserialize_funcs) = pipeline
        if pipeline_name != "":
            logger.info(_("Selected named pipeline ({})").format(pipeline_name))
        logger.info(
            _("Extracting relevant blocks of pdf from page %i to %i..."),
            i_page_batch,
            end_page_batch,
        )
        pdf_blocks = pdf_filter_exec(xml_roots, i_page_batch, n_pages, pdf_filter_funcs)
        logger.info(
            _("Filtering relevant blocks of text from page %i to %i..."),
            i_page_batch,
            end_page_batch,
        )
        filtered_text = text_extract_exec(pdf_blocks, targets, text_extract_funcs)
        results += deserialize_exec(filtered_text, deserialize_funcs)
        print(results)
    return results


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
    with files(data).joinpath("companies.csv").open("r") as f:
        target_csv = csv.reader(f)
        targets = [row[0] for row in target_csv if row]  # Skip empty rows
        targets.pop(0)  # Remove header
    return targets


def _get_document(config):
    if config["PDF"] is not None:
        log_string = _("Local PDF file used %s [%s format]")
        logger.debug(log_string, config["PDF"], config["FORMAT"])
        pdf_file = pypdf.Document(config["PDF"])
    else:
        log_string = _("Remote URL %s used [%s format]")
        logger.debug(log_string, config["URL"], config["FORMAT"])
        pdf_file = pypdf.Document(
            stream=dw.download_pdf(
                config["URL"], config["PDF"] if config["SAVE_PDF"] else None
            )
        )
    return pdf_file


def _output_file(config, results):
    out_csv = config["OUT_PATH"]
    out_dir = out_csv.parent
    compress = False
    remove_dir = False
    df = None
    if config["BATCH"] is not None:
        if config["SEPARATE_OUT_FILES"]:
            out_dir = out_csv
            if out_csv.name.endswith(".tar.gz"):
                compress = True
                out_dir = out_csv.with_suffix("").with_suffix("")
            if not out_dir.exists() and compress:
                remove_dir = True
            out_dir.mkdir(exist_ok=True)
        else:
            dataframes = []
            for r, format_pdf, prefix_out in results:
                if prefix_out is not None:
                    r["Report identifier"] = prefix_out
                r["Format"] = format_pdf.name
                dataframes.append(r)
            df = pd.concat(dataframes)
    else:
        df = results[0][0]

    if df is None:
        for df, format_pdf, prefix_out in results:
            name_file = f"{format_pdf.name}.csv"
            if prefix_out is not None and prefix_out != "":
                name_file = f"{prefix_out}-{format_pdf.name}.csv"
            df.to_csv(out_dir / name_file, index=False)
    else:
        df.to_csv(config["OUT_PATH"], index=False)

    if compress:
        with tarfile.open(out_csv, "w:gz") as tar:
            tar.add(out_dir, arcname=out_dir.name)
        if remove_dir:
            shutil.rmtree(out_dir)


def _main_job(config, n_workers):
    config = FreeportsConfig(**config).model_dump()
    logger.debug(_("Starting job with configuration %s"), str(config))
    pdf_file = _get_document(config)
    prefix_out = config["PREFIX_OUT"]
    logger.debug(_("Starting decoding pdf to xml..."))
    pdf_file_xml = [page.get_text("xml").encode() for page in pdf_file]
    logger.debug(_("End decoding pdf to xml!"))
    targets = get_target_companies(config["TARGET_LISTS"])
    logger.debug(_("First 5 targets: %s"), str(targets[: min(5, len(targets))]))
    n_pages = len(pdf_file_xml)
    batch_size = (n_pages + n_workers - 1) // n_workers
    batches = []
    for i in range(n_workers):
        start_idx = i * batch_size
        end_idx = min((i + 1) * batch_size, n_pages)
        batch_pages = pdf_file_xml[start_idx:end_idx]
        batches.append((batch_pages, start_idx + 1, n_pages, targets, config["FORMAT"]))

    results_batches = None
    if n_workers > 1:
        stderr_log.setFormatter(STANDARD_LOG_FORMATTER_MP)
        with Pool(processes=n_workers) as pool:
            results_batches = pool.starmap(pipeline_batch, batches)
        stderr_log.setFormatter(STANDARD_LOG_FORMATTER)
    else:
        results_batches = [pipeline_batch(*batches[0])]
    promises_resolution_map = {}
    results = []
    for batch in results_batches:
        for result in batch:
            if isinstance(result, PromisesResolutionContext):
                promises_resolution_map |= result
            else:
                results.append(result)

    flat_promises_map = flatten_promise_map(promises_resolution_map)
    dict_results = []
    error_msg = _("ERROR, SOMETHING WENT WRONG!!!!")
    for result in results:
        if result is not None:
            result.fulfill_promises(flat_promises_map)
            dict_results.append(result.to_dict())
        else:
            dict_results.append(
                Equity(
                    page=9999,
                    targets=[error_msg],
                    company=error_msg,
                    company_match="",
                    subfund=None,
                    nominal_quantity=None,
                    market_value=None,
                    perc_net_assets=0.0,
                    currency=Currency.EUR,
                ).to_dict()
            )

    df = pd.DataFrame(dict_results)
    return df, format_pdf, prefix_out


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
    n_workers = config["N_WORKERS"] if config["N_WORKERS"] > 0 else os.cpu_count()
    results = None
    if config["BATCH_FILE"] is None:
        results = [_main_job(config, n_workers)]
    else:
        config_jobs = batch_job_confs(config)
        args = [(c, 1) for c in config_jobs]
        if n_workers > 1:
            stderr_log.setFormatter(STANDARD_LOG_FORMATTER_MP)
            with Pool(n_workers) as p:
                results = p.starmap(_main_job, args)
            stderr_log.setFormatter(STANDARD_LOG_FORMATTER)
        else:
            results = [_main_job(*args[0])]

    _output_file(config, results)


if __name__ == "__main__":
    config = DEFAULT_CONFIG
    config_location = DEFAULT_CONFIG_LOCATION
    LOG_LEVEL = (5 - config["VERBOSITY"]) * 10
    log.basicConfig(level=LOG_LEVEL)
    config_env = FreeportsEnvConfig()
    tmp_config, tmp_config_location = config_env.overwrite_config(
        DEFAULT_CONFIG, DEFAULT_CONFIG_LOCATION
    )
    config_file_path = tmp_config["CONFIG_FILE"]
    config_file = FreeportsFileConfig(config_file_path)
    config, config_location = config_file.overwrite_config(
        DEFAULT_CONFIG, DEFAULT_CONFIG_LOCATION
    )
    config, config_location = config_env.overwrite_config(config, config_location)

    LOG_LEVEL = (5 - config["VERBOSITY"]) * 10
    log.getLogger().setLevel(LOG_LEVEL)
    log_config(logger, config, config_location)
    main(config)
