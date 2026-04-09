"""This module contains the main function used to extract information from PDF files
and save the results as CSV files. This file also serves as the source code to be
launched (providing options via configuration file or environment variables) to mimic
command line behavior. The logic distinguishes between the main function in this file
and the command line entry point by handling configuration parsing.

Example:
    ```python main.py```

"""

import os
import tarfile
import shutil
import logging as log
from typing import List, Tuple, Optional, Union, Dict, Any
from multiprocessing import Pool, set_start_method
import csv
from lxml import etree
import pymupdf as pypdf
import pandas as pd
from freeports_analysis.i18n import _
from freeports_analysis.data import get_target_companies
from freeports_analysis.output import (
    transform_to_files_schema,
    write_files,
    Investment,
    Equity,
    Bond,
    Fund,
    FundRename,
    FundChangeName,
    FundMerge,
    FundAssets,
    AssetsManager,
    InvestmentsManager,
    DocumentResults,
    PageResults,
)
from freeports_analysis import download as dw
from freeports_analysis.consts import PromisesResolutionContext, flatten_promise_map
from freeports_analysis.formats.algorithms import Algorithm
from freeports_analysis.conf_parse import (
    DEFAULT_CONFIG,
    DEFAULT_CONFIG_LOCATION,
    FreeportsEnvConfig,
    FreeportsFileConfig,
    FreeportsConfig,
    FreeportsJobConfig,
)
from freeports_analysis.logging import (
    log_config,
    LOG_CONTEXTUAL_INFOS,
    LOG_ADAPT_INVESTMENT_INFOS,
    LOGGING_TABLE,
    CsvFormatter,
    DevDebugFormatter,
)

set_start_method("fork")
logger = log.getLogger()


class NoPDFormatDetected(Exception):
    """Exception raised when the script cannot detect a PDF format to decode the report.

    This exception is raised when no explicit format is specified and the program
    cannot automatically determine the appropriate format for decoding the PDF.
    """


def batch_job_confs(job_config: Dict[str, Any]) -> List[Dict[str, Any]]:
    """Create a list of configurations by reading a batch file with job contextual options.

    Parameters
    ----------
    job_config : Dict[str, Any]
        Base configuration to be overwritten with batch file options

    Returns
    -------
    List[Dict[str, Any]]
        List of configurations, one for each row in the batch file

    Raises
    ------
    FileNotFoundError
        If the batch file does not exist
    csv.Error
        If the batch file has invalid CSV format

    Notes
    -----
    The batch file should be a CSV file with columns corresponding to
    configuration keys that can override the base configuration.
    """
    rows = None
    with job_config["BATCH_FILE"].open(newline="", encoding="UTF-8") as csvfile:
        rows = csv.DictReader(csvfile)
        result = []
        for row in rows:
            job_config_instance = FreeportsJobConfig(row)
            result.append(
                job_config_instance.overwrite_config(
                    job_config, DEFAULT_CONFIG_LOCATION
                )[0]
            )
    return result


def _get_document(document_config: Dict[str, Any]) -> pypdf.Document:
    """Get the PDF document from local filesystem or by downloading from URL.

    Parameters
    ----------
    document_config : Dict[str, Any]
        Configuration dictionary containing PDF source information

    Returns
    -------
    pymupdf.Document
        PDF document object

    Raises
    ------
    Exception
        If PDF download fails or file cannot be opened

    Notes
    -----
    The function prioritizes local PDF files over remote URLs if both are specified.
    """
    if document_config["PDF"] is not None:
        log_string = _("Local PDF file used %s [%s format]")
        logger.debug(log_string, document_config["PDF"], document_config["FORMAT"])
        pdf_file = pypdf.Document(document_config["PDF"])
    else:
        log_string = _("Remote URL %s used [%s format]")
        logger.debug(log_string, document_config["URL"], document_config["FORMAT"])
        pdf_file = pypdf.Document(
            stream=dw.download_pdf(
                document_config["URL"],
                document_config["PDF"] if document_config["SAVE_PDF"] else None,
            )
        )
    return pdf_file


def _output_file(
    output_config: Dict[str, Any],
    results: List[Tuple[pd.DataFrame, str, Optional[str]]],
) -> None:
    """Write output files based on configuration and processing results.

    Parameters
    ----------
    output_config : Dict[str, Any]
        Configuration dictionary containing output settings
    results : List[Tuple[pd.DataFrame, str, Optional[str]]]
        List of tuples containing (dataframe, format_name, prefix) for each result

    Notes
    -----
    Handles both single file output and batch processing with optional compression.
    Creates tar.gz archives when compression is enabled and separates files
    when batch processing with separate output files flag.
    """
    out_csv = output_config["OUT_PATH"]
    out_dir = out_csv.parent
    compress = False
    remove_dir = False
    df = None

    if output_config["BATCH_FILE"] is not None:
        if output_config["SEPARATE_OUT_FILES"]:
            out_dir = out_csv
            if out_csv.name.endswith(".tar.gz"):
                compress = True
                out_dir = out_csv.with_suffix("").with_suffix("")
            if not out_dir.exists() and compress:
                remove_dir = True
            out_dir.mkdir(exist_ok=True)
            for df_result, format_pdf, prefix_out in results:
                name_file = f"{format_pdf.name}.csv"
                if prefix_out is not None and prefix_out != "":
                    name_file = f"{prefix_out}-{format_pdf.name}.csv"
                df_result.to_csv(out_dir / name_file, index=False)
        else:
            dataframes = []
            for r, format_pdf, prefix_out in results:
                if prefix_out is not None:
                    r["Report identifier"] = prefix_out
                r["Format"] = format_pdf.name
                dataframes.append(r)
            df = pd.concat(dataframes)
            df.to_csv(output_config["OUT_PATH"], index=False)
    else:
        df = results[0][0]
        df.to_csv(output_config["OUT_PATH"], index=False)

    if compress:
        with tarfile.open(out_csv, "w:gz") as tar:
            tar.add(out_dir, arcname=out_dir.name)
        if remove_dir:
            shutil.rmtree(out_dir)


def _main_job(
    main_job_config: Dict[str, Any], n_workers: int
) -> Tuple[List[List[Investment]], str, Optional[str]]:
    """Execute a single job for PDF processing and data extraction.

    Parameters
    ----------
    main_job_config : Dict[str, Any]
        Configuration dictionary for the job
    n_workers : int
        Number of worker processes to use for parallel processing

    Returns
    -------
    Tuple[List[List[Investment]], str, Optional[str]]
        Tuple containing (results, format_name, prefix) for the processed job
        where:
        - results: List of investment lists (one per page)
        - format_name: Name of the format used for processing
        - prefix: Optional prefix for output file naming

    Notes
    -----
    This function handles the complete PDF processing pipeline including:
    - PDF document retrieval
    - XML conversion
    - Target company filtering
    - Parallel processing of page batches
    - Promise resolution for deferred values
    """
    job_config = FreeportsConfig(**main_job_config).model_dump()
    LOG_CONTEXTUAL_INFOS.report = job_config["PREFIX_OUT"]
    log_file = job_config["OUT_PATH"] / ".log.csv"
    handler_csv = log.FileHandler(log_file, mode="a")
    csv_formatter = CsvFormatter()
    handler_csv.addFilter(LOG_ADAPT_INVESTMENT_INFOS)
    handler_csv.addFilter(LOG_CONTEXTUAL_INFOS)
    handler_csv.setFormatter(csv_formatter)
    handler_csv.setLevel(log.WARNING)
    format_utils = log.getLogger(__package__ + ".formats.utils")
    format_utils.addHandler(handler_csv)
    LOGGING_TABLE.addHandler(handler_csv)
    logger.debug(_("Starting job with configuration %s"), str(job_config))
    alghoritm = Algorithm.load(job_config["FORMAT"])
    pdf_file = _get_document(job_config)
    logger.info(_("Starting decoding pdf to python dict..."))
    pdf_file_dict = [page.get_text("dict") for page in pdf_file]
    logger.debug(_("End decoding pdf to python dict!"))
    targets = get_target_companies(job_config["TARGET_LISTS"])
    logger.debug(_("First 5 targets:\n%s"), str(targets[: min(5, len(targets))]))
    results = alghoritm(pdf_file_dict, targets)
    promises_resolution_map = {}
    doc_results = DocumentResults(job_config["PREFIX_OUT"], job_config["FORMAT"])
    for pn in range(1, len(pdf_file_dict) + 1):
        doc_results.results.append(PageResults())
        for r in results.get(pn, []):
            if isinstance(r, dict):
                promises_resolution_map |= r
            elif isinstance(r, Investment):
                doc_results.results[-1].investments.append(r)
            elif isinstance(r, AssetsManager):
                doc_results.results[-1].assets_managers.append(r)
            elif isinstance(r, Fund):
                doc_results.results[-1].funds.append(r)
            elif isinstance(r, FundAssets):
                doc_results.results[-1].funds_assets.append(r)
            elif isinstance(r, FundChangeName):
                doc_results.results[-1].funds_change_name.append(r)
            else:
                raise Exception(f"Not recognized type of result {type(r)}")
    promises_resolution_map = flatten_promise_map(promises_resolution_map)
    doc_results.fulfill_promises(promises_resolution_map)
    format_utils.removeHandler(handler_csv)
    LOGGING_TABLE.removeHandler(handler_csv)
    LOG_CONTEXTUAL_INFOS.report = None
    return doc_results


def main(main_config: Dict[str, Any]) -> None:
    """Main function for PDF processing and data extraction.

    Expects configuration to be already provided (via command line arguments,
    environment variables, or configuration files).

    Parameters
    ----------
    main_config : Dict[str, Any]
        Configuration dictionary containing all processing parameters

    Raises
    ------
    NoPDFormatDetected
        If no explicit format is provided and the program cannot automatically
        determine the appropriate format for decoding the PDF
    FileNotFoundError
        If required input files or directories are not found
    ValueError
        If configuration contains invalid values

    Notes
    -----
    This function orchestrates the complete PDF processing workflow:
    1. Configuration validation and setup
    2. Log file initialization
    3. Batch or single job processing
    4. Parallel execution with multiprocessing
    5. Output file generation
    6. Result transformation and writing
    """
    n_workers = (
        main_config["N_WORKERS"] if main_config["N_WORKERS"] > 0 else os.cpu_count()
    )
    main_config["OUT_PATH"].mkdir(exist_ok=True)
    log_file = main_config["OUT_PATH"] / ".log.csv"
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
        if main_config["BATCH_FILE"] is not None:
            header = ["Report"] + header
        writer.writerow(header)

    results_documents = None
    if main_config["BATCH_FILE"] is None:
        results_documents = [_main_job(main_config, n_workers)]
    else:
        LOG_CONTEXTUAL_INFOS.batch_mode = True
        config_jobs = batch_job_confs(main_config)
        args = [(c, 1) for c in config_jobs]
        if n_workers > 1:
            LOG_CONTEXTUAL_INFOS.mproc = True
            with Pool(n_workers) as p:
                results_documents = p.starmap(_main_job, args)
            LOG_CONTEXTUAL_INFOS.mproc = False
        else:
            results_documents = []
            for arg in args:
                results_documents.append(_main_job(*arg))
    results = transform_to_files_schema(
        results_documents, main_config["BATCH_FILE"] is not None
    )
    write_files(
        results,
        main_config["OUT_PATH"],
        main_config["OUT_PROFILE"],
        main_config["OUT_FLAGS"],
    )


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
        handler_devdebug = log.FileHandler("freeports.log", "w")
        handler_devdebug.addFilter(LOG_CONTEXTUAL_INFOS)
        handler_devdebug.setFormatter(DevDebugFormatter())
        logger.addHandler(handler_devdebug)
    logger.setLevel(LOG_LEVEL)
    log_config(logger, config, config_location)
    main(config)
