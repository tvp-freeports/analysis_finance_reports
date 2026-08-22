"""Archived dead code, moved out of `python/freeports/_internals/cli/cmd.py` during the
maturin-idiomatic restructure session (2026-08-21) — see
`analysis_finance_reports/agent-memory/maturin-idiomatic-restructure-plan.md`, §6b. Reference-only,
never packaged (see this directory's own `reference_legacy/README.md`). Docstring below is
preserved verbatim from the live tree.

``_legacy_cmd`` was the original Python CLI entry point (argv parsing + config resolution +
orchestration), superseded by the Rust implementation `cmd()` execs into (see `src/main.rs`). Its
only caller in `cmd.py` was itself (via the module-level `main as _legacy_main` import, which
moved away with it — the still-live `_internals.cli.main.main` is unaffected).
"""

import logging

from freeports._internals.core.logging import (
    HANDLER_STDERR,
    LOG_CONTEXTUAL_INFOS,
    DevDebugFormatter,
    log_config,
)
from freeports._internals.cli.conf_parse import (
    DEFAULT_CONFIG_LOCATION,
    DEFAULT_CONFIG,
    FreeportsFileConfig,
    FreeportsEnvConfig,
    FreeportsCmdConfig,
)
from freeports._internals.cli.main import main as _legacy_main


def _legacy_cmd() -> None:
    """Dead code, superseded by the Rust implementation `cmd()` execs into. Kept until the
    migration is far enough along to delete it.

    Notes
    -----
    The configuration is loaded in the following order of precedence:
    1. Command line arguments (highest priority)
    2. Environment variables
    3. Configuration files
    4. Default values (lowest priority)
    """
    rootlogger = logging.getLogger()
    logger = logging.getLogger(__package__ + ".cmd")
    logger.addHandler(HANDLER_STDERR)
    logger.propagate = False

    config = DEFAULT_CONFIG
    config_location = DEFAULT_CONFIG_LOCATION
    log_level = (5 - config["VERBOSITY"]) * 10
    logger.setLevel(log_level)

    parser = FreeportsCmdConfig.create_parser()
    config_cmd = FreeportsCmdConfig(parser.parse_args(), DEFAULT_CONFIG["VERBOSITY"])
    config_env = FreeportsEnvConfig()
    tmp_config, tmp_config_location = config_env.overwrite_config(
        DEFAULT_CONFIG, DEFAULT_CONFIG_LOCATION
    )
    tmp_config, tmp_config_location = config_cmd.overwrite_config(
        tmp_config, tmp_config_location
    )
    config_file_path = tmp_config["CONFIG_FILE"]
    config_file = FreeportsFileConfig(config_file_path)
    config, config_location = config_file.overwrite_config(
        DEFAULT_CONFIG, DEFAULT_CONFIG_LOCATION
    )
    config, config_location = config_env.overwrite_config(config, config_location)
    config, config_location = config_cmd.overwrite_config(config, config_location)
    log_level = (5 - config["VERBOSITY"]) * 10
    if log_level <= logging.DEBUG:
        handler_devdebug = logging.FileHandler("freeports.log", "w")
        handler_devdebug.addFilter(LOG_CONTEXTUAL_INFOS)
        handler_devdebug.setFormatter(DevDebugFormatter())
        rootlogger.addHandler(handler_devdebug)
    rootlogger.setLevel(log_level)
    log_config(logger, config, config_location)

    logger.removeHandler(HANDLER_STDERR)
    logger.propagate = True
    _legacy_main(config)
