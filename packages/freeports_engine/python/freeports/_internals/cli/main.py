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
from pathlib import Path
from typing import List, Tuple, Optional, Dict, Any
from multiprocessing import Pool, set_start_method
import csv
import pymupdf as pypdf
import pandas as pd
import freeports_engine
from freeports.i18n import _
from freeports._internals.input.companies_db import get_target_companies

transform_to_files_schema = freeports_engine.core.transform_to_files_schema
write_files = freeports_engine.core.write_files
DocumentResults = freeports_engine.core.DocumentResults
PageResults = freeports_engine.core.PageResults
download_pdf = freeports_engine.core.download_pdf
flatten_promise_map = freeports_engine.core.flatten_promise_map
build_promise_multimap = freeports_engine.core.build_promise_multimap
merge_into_multimap = freeports_engine.core.merge_into_multimap
Algorithm = freeports_engine.core.Algorithm

from freeports._internals.output.classes_schema import (
    Investment,
    Fund,
    FundChangeName,
    FundSfdrClassification,
    FundEsgIndicator,
    FundAssets,
    AssetsManager,
)
from freeports._internals.formats.repo.metadata import get_formats
from freeports._internals.cli.conf_parse import (
    DEFAULT_CONFIG,
    DEFAULT_CONFIG_LOCATION,
    FreeportsEnvConfig,
    FreeportsFileConfig,
    FreeportsConfig,
    FreeportsJobConfig,
)
from freeports._internals.core.logging import (
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


def _resolve_documents(job_config: Dict[str, Any]) -> List[Tuple[str, List[dict]]]:
    """Resolve document specs into (report_name, page_dicts) pairs.

    Uses ``_document_specs`` produced by ``pdf_path_validation`` in the config
    model, or falls back to single-document mode for backward compat.
    """
    from freeports._internals.cli.conf_parse import DocumentSpec

    docs: List[DocumentSpec] = job_config["INPUT_REPORTS"]

    result: List[Tuple[str, List[dict]]] = []
    for ds in docs:
        pdf_file = None
        name = None
        page_dicts = None
        if ds["path"] is not None:
            name = ds["name"]
            pdf_file = pypdf.Document(str(ds["path"]))
        elif ds["url"] is not None:
            name = ds["name"]
            save_path = ds["path"]
            pdf_file = pypdf.Document(
                stream=download_pdf(
                    ds["url"], save_path if job_config.get("SAVE_PDF") else None
                )
            )
        page_dicts = [page.get_text("dict") for page in pdf_file]
        result.append((name, page_dicts))
    return result


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
    main_job_config: Dict[str, Any], _n_workers: int
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
    # LOG_CONTEXTUAL_INFOS.report = job_config["PREFIX_OUT"]
    # Unlike `main_config` above, `job_config` HAS been through `FreeportsConfig`'s own
    # `out_path_single_file` validator (via the `FreeportsConfig(**main_job_config)` call right
    # above) — in SINGLE_FILE mode `OUT_PATH` is now a `.csv` *file* path, not a directory, so
    # `.log.csv` has to live in its parent instead (already created by `main()`'s `mkdir()` on the
    # pre-transform `main_config["OUT_PATH"]`, the same directory).
    log_dir = (
        job_config["OUT_PATH"].parent
        if job_config["OUT_PROFILE"].name == "SINGLE_FILE"
        else job_config["OUT_PATH"]
    )
    log_file = log_dir / ".log.csv"
    handler_csv = log.FileHandler(log_file, mode="a")
    csv_formatter = CsvFormatter()
    handler_csv.addFilter(LOG_ADAPT_INVESTMENT_INFOS)
    handler_csv.addFilter(LOG_CONTEXTUAL_INFOS)
    handler_csv.setFormatter(csv_formatter)
    handler_csv.setLevel(log.WARNING)
    format_utils = log.getLogger("freeports._internals.formats.utils")
    format_utils.addHandler(handler_csv)
    LOGGING_TABLE.addHandler(handler_csv)
    logger.debug(_("Starting job with configuration %s"), str(job_config))
    formats_df = get_formats(job_config["FORMATS_REPO_PATH"])
    format_names = formats_df.index.to_list()
    algorithm = Algorithm.load(
        job_config["FORMATS_REPO_PATH"], job_config["FORMAT"], format_names
    )
    documents = _resolve_documents(job_config)
    logger.info(_("Processing %d document(s)..."), len(documents))
    targets = get_target_companies(
        job_config["INPUT_DB_PATH"], job_config["TARGET_LISTS"]
    )
    logger.debug(_("First 5 targets:\n%s"), str(targets[: min(5, len(targets))]))

    promises_resolution_map = build_promise_multimap()
    doc_results_list = []
    results = algorithm(documents, targets)
    doc_results_by_name: Dict[str, DocumentResults] = {}
    seen_pages_by_name: Dict[str, set] = {}
    for doc_name, pn in sorted(results.keys()):
        if doc_name not in doc_results_by_name:
            doc_results_by_name[doc_name] = DocumentResults(
                doc_name, job_config["FORMAT"]
            )
            seen_pages_by_name[doc_name] = set()
            doc_results_list.append(doc_results_by_name[doc_name])
        doc_results = doc_results_by_name[doc_name]
        seen_pages = seen_pages_by_name[doc_name]
        if pn not in seen_pages:
            pr = PageResults()
            pr.page_number = pn
            doc_results.results.append(pr)
            seen_pages.add(pn)
        for r in results.get((doc_name, pn), []):
            if isinstance(r, dict):
                merge_into_multimap(promises_resolution_map, r)
            elif isinstance(r, Investment):
                doc_results.results[-1].investments.append(r)
            elif isinstance(r, AssetsManager):
                doc_results.results[-1].assets_managers.append(r)
            elif isinstance(r, Fund):
                doc_results.results[-1].funds.append(r)
            elif isinstance(r, FundSfdrClassification):
                doc_results.results[-1].funds_sfdr_classification.append(r)
            elif isinstance(r, FundEsgIndicator):
                doc_results.results[-1].funds_esg_indicators.append(r)
            elif isinstance(r, FundAssets):
                doc_results.results[-1].funds_assets.append(r)
            elif isinstance(r, FundChangeName):
                doc_results.results[-1].funds_change_name.append(r)
            else:
                raise TypeError(f"Not recognized type of result {type(r)}")

    promises_resolution_map = flatten_promise_map(promises_resolution_map)
    for dr in doc_results_list:
        dr.fulfill_promises(promises_resolution_map)
    format_utils.removeHandler(handler_csv)
    LOGGING_TABLE.removeHandler(handler_csv)
    LOG_CONTEXTUAL_INFOS.report = None
    return doc_results_list


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
    3. Single job processing
    4. Output file generation
    5. Result transformation and writing

    Batch mode (multiple jobs from a `--batch` CSV, dispatched across worker processes) is no
    longer handled here — Fase E's native Rust binary (`packages/freeports_engine/src/main.rs`)
    owns that now, and is the only live caller of the CLI path that ever set `BATCH_FILE` (`cmd.py`'s
    `cmd()`, retired in the same simplification pass — see `agent-memory/rust-native-binary-plan.md`).
    This function's only remaining live caller, `freeports_dev.pytest_plugin` (which powers
    `freeports-dev test`'s whole fixture suite), always calls it with a single document and
    `BATCH_FILE: None` — confirmed via a workspace-wide grep before removing the batch branch, not
    assumed. The original batch-dispatch body is kept as `_legacy_main` (dead code, this
    migration's usual strangler-fig convention) until the migration is far enough along to delete it.
    """
    main_config["OUT_PATH"].mkdir(exist_ok=True)
    log_file = main_config["OUT_PATH"] / ".log.csv"
    with log_file.open("w", newline="", encoding="utf-8") as csvfile:
        writer = csv.writer(csvfile)
        writer.writerow(
            [
                "Page",
                "Matched Company",
                "Company",
                "Field name",
                "Row",
                "Column",
                "Message",
            ]
        )

    results_documents = _main_job(main_config, 1)
    results = transform_to_files_schema(results_documents, False)
    # `main_config["OUT_PATH"]` is still the raw `--out` directory here (see the comment on the
    # `mkdir()` call above) — `write_files` needs the same `FreeportsConfig`-validated path
    # `_main_job` computes for itself (a `.csv` *file* path in SINGLE_FILE mode, via
    # `conf_parse.py`'s `out_path_single_file` validator), so it's recomputed the same way here
    # rather than reusing the pre-transform value.
    final_out_path = FreeportsConfig(**main_config).model_dump()["OUT_PATH"]
    write_files(
        results,
        final_out_path,
        main_config["OUT_PROFILE"],
        main_config["OUT_FLAGS"],
    )


def _legacy_main(main_config: Dict[str, Any]) -> None:
    """Dead code, superseded by `main()` above (Fase E's final simplification pass removed the
    batch-dispatch branch, now fully handled by the native Rust binary). Kept until the migration
    is far enough along to delete it.
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

    results_documents = []
    if main_config["BATCH_FILE"] is None:
        results_documents = _main_job(main_config, n_workers)
    else:
        LOG_CONTEXTUAL_INFOS.batch_mode = True
        config_jobs = batch_job_confs(main_config)
        args = [(c, 1) for c in config_jobs]
        if n_workers > 1:
            LOG_CONTEXTUAL_INFOS.mproc = True
            with Pool(n_workers) as p:
                batch_results = p.starmap(_main_job, args)
            LOG_CONTEXTUAL_INFOS.mproc = False
        else:
            batch_results = []
            for arg in args:
                batch_results.append(_main_job(*arg))
        for br in batch_results:
            results_documents.extend(br)
    results = transform_to_files_schema(
        results_documents, main_config["BATCH_FILE"] is not None
    )
    final_out_path = FreeportsConfig(**main_config).model_dump()["OUT_PATH"]
    write_files(
        results,
        final_out_path,
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
