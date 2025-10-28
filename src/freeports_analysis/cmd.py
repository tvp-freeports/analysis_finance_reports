"""Contains all functions related to command line use of the `freeport` script."""

import logging as log

from freeports_analysis.conf_parse import (
    DEFAULT_CONFIG_LOCATION,
    DEFAULT_CONFIG,
    FreeportsFileConfig,
    FreeportsEnvConfig,
    FreeportsCmdConfig,
)
from freeports_analysis.logging import (
    HANDLER_STDERR,
    LOG_CONTEXTUAL_INFOS,
    DevDebugFormatter,
    log_config,
)
from freeports_analysis.main import main


def cmd() -> None:
    """Command line entry point for the freeports script.

    This function is called when launching `freeports` from the terminal.
    It handles configuration parsing from multiple sources (command line,
    environment variables, configuration files) and calls the main function.

    Raises
    ------
    argparse.ArgumentError
        If command line arguments are invalid or conflicting
    FileNotFoundError
        If specified configuration files are not found
    ValueError
        If configuration values are invalid

    Notes
    -----
    The configuration is loaded in the following order of precedence:
    1. Command line arguments (highest priority)
    2. Environment variables
    3. Configuration files
    4. Default values (lowest priority)
    """
    rootlogger = log.getLogger()
    logger = log.getLogger(__package__ + ".cmd")
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
    if log_level <= log.DEBUG:
        handler_devdebug = log.FileHandler("freeports.log", "w")
        handler_devdebug.addFilter(LOG_CONTEXTUAL_INFOS)
        handler_devdebug.setFormatter(DevDebugFormatter())
        rootlogger.addHandler(handler_devdebug)
    rootlogger.setLevel(log_level)
    log_config(logger, config, config_location)

    logger.removeHandler(HANDLER_STDERR)
    logger.propagate = True
    main(config)
