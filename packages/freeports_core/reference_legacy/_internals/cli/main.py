"""Archived dead code, moved out of `python/freeports/_internals/cli/main.py` during the
maturin-idiomatic restructure session (2026-08-21) — see
`analysis_finance_reports/agent-memory/maturin-idiomatic-restructure-plan.md`, §6c. Reference-only,
never packaged (see this directory's own `reference_legacy/README.md`). Docstrings below are
preserved verbatim from the live tree.

Per-symbol treatment (not a whole-file move — `main.py` itself is still very much live):

- `NoPDFormatDetected` — never raised or caught anywhere in the workspace (only mentioned in
  `main()`'s own docstring "Raises" section, describing an exception that function never actually
  raises). Not `_legacy_`-prefixed in the source, but confirmed dead via a workspace-wide grep.
- `batch_job_confs` — confirmed zero callers anywhere except `_legacy_main` itself; dead once
  `_legacy_main` moved here too.
- `_output_file` — confirmed zero callers anywhere, not even from `_legacy_main`.
- `_legacy_main` — explicitly marked dead in its own docstring, superseded by the live `main()`.
- The `if __name__ == "__main__":` block — confirmed dead: no consumer anywhere in the workspace
  runs `python -m freeports._internals.cli.main` or otherwise executes this file as a script;
  `main.py`'s only real caller (`freeports_dev.pytest_plugin`) always imports `main` as a function.

`_resolve_documents`, `_main_job`, and `main()` itself stay live in `python/freeports/_internals/
cli/main.py` (real callers: `_main_job`/`main()` call each other, and `freeports_dev.pytest_plugin`
calls `main()` directly) — imported from there below where this archived code still calls them, so
this file reads the same as it did in place.
"""

import os
import csv
import logging as log
from typing import List, Dict, Any, Tuple, Optional
from multiprocessing import Pool

import pandas as pd
import tarfile
import shutil

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
    DevDebugFormatter,
)
from freeports._internals.cli.main import (
    logger,
    _main_job,
    main,
    transform_to_files_schema,
    write_files,
)

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
