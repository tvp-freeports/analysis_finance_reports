"""Contains all the functions related to command line use of the `freeport` script"""

import argparse
import logging as log
from pathlib import Path
from typing import Tuple


from freeports_analysis.consts import PdfFormats, STANDARD_LOG_FORMATTER
from freeports_analysis.i18n import _
from freeports_analysis.conf_parse import (
    DEFAULT_CONFIG_LOCATION,
    DEFAULT_CONFIG,
    FreeportsFileConfig,
    FreeportsEnvConfig,
    FreeportsCmdConfig,
    log_config,
)
from freeports_analysis.main import main


logger = log.getLogger()
stderr_log = log.StreamHandler()
stderr_log.setLevel(log.DEBUG)
stderr_log.setFormatter(STANDARD_LOG_FORMATTER)
logger.addHandler(stderr_log)


def cmd():
    """Command called when launching `freeports` from terminal,
    it calls the `main` function.
    """
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
    logger.setLevel(log_level)
    log_config(logger, config, config_location)
    main(config)
