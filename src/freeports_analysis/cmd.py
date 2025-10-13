"""Contains all the functions related to command line use of the `freeport` script"""

import logging as log

from freeports_analysis.i18n import _
from freeports_analysis.conf_parse import (
    DEFAULT_CONFIG_LOCATION,
    DEFAULT_CONFIG,
    FreeportsFileConfig,
    FreeportsEnvConfig,
    FreeportsCmdConfig,
    log_config,
)
from freeports_analysis.logging import (
    HANDLER_STDERR,
    LOG_CONTEXTUAL_INFOS,
    DevDebugFormatter,
)
from freeports_analysis.main import main


def cmd():
    """Command called when launching `freeports` from terminal,
    it calls the `main` function.
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
        HANDLER_DEVDEBUG = log.FileHandler("freeports.log", "w")
        HANDLER_DEVDEBUG.addFilter(LOG_CONTEXTUAL_INFOS)
        HANDLER_DEVDEBUG.setFormatter(DevDebugFormatter())
        rootlogger.addHandler(HANDLER_DEVDEBUG)
    rootlogger.setLevel(log_level)
    log_config(logger, config, config_location)

    logger.removeHandler(HANDLER_STDERR)
    logger.propagate = True
    main(config)
