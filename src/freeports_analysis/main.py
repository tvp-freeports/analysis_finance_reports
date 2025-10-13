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
from freeports_analysis.i18n import _
from freeports_analysis.data import get_target_companies
from freeports_analysis.output import transform_to_files_schema, write_files, Investment
from freeports_analysis import download as dw
from freeports_analysis.consts import PromisesResolutionContext, flatten_promise_map
from freeports_analysis.formats.algorithms import (
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
    FreeportsJobConfig,
)
from freeports_analysis.logging import (
    LOG_CONTEXTUAL_INFOS,
    LOG_ADAPT_INVESTMENT_INFOS,
    LOGGING_TABLE,
    CsvFormatter,
)


logger = log.getLogger()


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
) -> List[Investment | PromisesResolutionContext]:
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
            _("Filtering relevant blocks of pdf from page %i to %i..."),
            i_page_batch,
            end_page_batch,
        )
        pdf_blocks = pdf_filter_exec(i_page_batch, n_pages, xml_roots, pdf_filter_funcs)
        logger.info(
            _("Extracting relevant blocks of text from page %i to %i..."),
            i_page_batch,
            end_page_batch,
        )
        filtered_text = text_extract_exec(
            i_page_batch, n_pages, pdf_blocks, targets, text_extract_funcs
        )
        results += deserialize_exec(
            i_page_batch, n_pages, filtered_text, deserialize_funcs
        )

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
    with config["BATCH_FILE"].open(newline="", encoding="UTF-8") as csvfile:
        rows = csv.DictReader(csvfile)
        result = []
        for row in rows:
            job_config = FreeportsJobConfig(row)
            result.append(
                job_config.overwrite_config(config, DEFAULT_CONFIG_LOCATION)[0]
            )
    return result


def _get_document(config: dict):
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
    if config["BATCH_FILE"] is not None:
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


def _main_job(config, n_workers: int):
    config = FreeportsConfig(**config).model_dump()
    config["OUT_PATH"].mkdir(exist_ok=True)
    log_file = config["OUT_PATH"] / ".log.csv"
    with log_file.open("w", newline="", encoding="utf-8") as csvfile:
        writer = csv.writer(csvfile)
        header = [
            "Page",
            "Matched Company",
            "Company",
            "Field name",
            "Row",
            "Column",
            "Message",
        ]
        if config["BATCH_FILE"] is not None:
            header = ["Report"] + header
        writer.writerow(header)
    HANDLER_CSV = log.FileHandler(log_file, mode="a")
    CSV_FORMATTER = CsvFormatter()
    HANDLER_CSV.addFilter(LOG_ADAPT_INVESTMENT_INFOS)
    HANDLER_CSV.addFilter(LOG_CONTEXTUAL_INFOS)
    HANDLER_CSV.setFormatter(CSV_FORMATTER)
    HANDLER_CSV.setLevel(log.WARNING)
    format_utils = log.getLogger(__package__ + ".formats.utils")
    format_utils.addHandler(HANDLER_CSV)
    LOGGING_TABLE.addHandler(HANDLER_CSV)
    LOG_CONTEXTUAL_INFOS.report = config["PREFIX_OUT"]
    logger.debug(_("Starting job with configuration %s"), str(config))
    pdf_file = _get_document(config)
    logger.info(_("Starting decoding pdf to xml..."))
    pdf_file_xml = [page.get_text("xml").encode() for page in pdf_file]
    logger.debug(_("End decoding pdf to xml!"))
    targets = get_target_companies(config["TARGET_LISTS"])
    logger.debug(_("First 5 targets:\n%s"), str(targets[: min(5, len(targets))]))
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
        LOG_CONTEXTUAL_INFOS.mproc = True
        with Pool(processes=n_workers) as pool:
            results_batches = pool.starmap(pipeline_batch, batches)
        LOG_CONTEXTUAL_INFOS.mproc = False
    else:
        results_batches = [pipeline_batch(*batches[0])]
    promises_resolution_map = {}
    results = []
    for results_batch in results_batches:
        for results_page in results_batch:
            extracted_data_page = []
            for result in results_page:
                if isinstance(result, PromisesResolutionContext):
                    promises_resolution_map |= result
                else:
                    extracted_data_page.append(result)
            results.append(extracted_data_page)
    flat_promises_map = flatten_promise_map(promises_resolution_map)
    for i, results_page in enumerate(results):
        for j in range(len(results_page)):
            results[i][j].fulfill_promises(flat_promises_map)

    return results, config["FORMAT"], config["PREFIX_OUT"]


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
    results_documents = None
    if config["BATCH_FILE"] is None:
        results_documents = [_main_job(config, n_workers)]
    else:
        LOG_CONTEXTUAL_INFOS.batch_mode = True
        config_jobs = batch_job_confs(config)
        args = [(c, 1) for c in config_jobs]
        if n_workers > 1:
            LOG_CONTEXTUAL_INFOS.mproc = True
            with Pool(n_workers) as p:
                results_documents = p.starmap(_main_job, args)
            LOG_CONTEXTUAL_INFOS.mproc = False
        else:
            results_documents = [_main_job(*args[0])]
    results = transform_to_files_schema(
        results_documents, config["BATCH_FILE"] is not None
    )
    write_files(results, config["OUT_PATH"], config["OUT_PROFILE"], config["OUT_FLAGS"])


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
    if LOG_LEVEL <= log.DEBUG:
        HANDLER_DEVDEBUG = log.FileHandler("freeports.log", "w")
        HANDLER_DEVDEBUG.addFilter(LOG_CONTEXTUAL_INFOS)
        HANDLER_DEVDEBUG.setFormatter(DevDebugFormatter())
        logger.addHandler(HANDLER_DEVDEBUG)
    logger.setLevel(LOG_LEVEL)
    log_config(logger, config, config_location)
    main(config)
