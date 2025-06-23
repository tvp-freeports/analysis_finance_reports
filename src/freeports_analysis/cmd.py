"""Contains all the functions related to command line use of the `freeport` script"""

import argparse
import logging as log
import os
from enum import Enum, auto
import yaml

from pathlib import Path
from .consts import PDF_Formats, ENV_PREFIX
from .conf_parse import (
    POSSIBLE_LOCATION_CONFIG,
    DEFAULT_CONFIG,
    RESULTING_CONFIG,
    RESULTING_LOCATION_CONFIG,
    log_resultig_config,
    apply_config,
    get_config_file,
    validate_conf,
)
from .main import main


__all__ = ["create_parser", "cmd"]


logger = log.getLogger(__name__)


def _create_parser() -> argparse.ArgumentParser:
    """Create and set the parser for command line args

    Returns
    -------
    argparse.ArgumentParser
        class that contains the command line options, retrieve and validate them
    """
    parser = argparse.ArgumentParser(
        description="Analyze finance reports searching for investing in companies allegedly involved interantional law violations by third parties"
    )
    # Argomenti obbligatori (stringhe)
    parser.add_argument(
        "--url", "-u", type=str, help="URL of the dir where to find the pdf"
    )
    parser.add_argument("--pdf", "-i", type=str, help="Name of the file")
    parser.add_argument(
        "--batch", "-b", type=str, help="Activate `BATCH MODE`, path of the batch file"
    )
    parser.add_argument(
        "--workers",
        "-j",
        type=int,
        help="# parallel workers in `BATCH MODE`, if num <= 0, it set to # cpu avalaibles",
    )
    parser.add_argument(
        "--format", "-f", type=str, choices=PDF_Formats.__members__, help="PDF format"
    )
    parser.add_argument(
        "--no-download", action="store_true", help="Don't save file locally"
    )
    parser.add_argument("--config", type=str, help="Custom configuration file location")
    parser.add_argument(
        "--out",
        "-o",
        type=str,
        help="Output file cvs (default path: '" + DEFAULT_CONFIG["OUT_CSV"] + "')",
    )
    parser.add_argument(
        "-v",
        action="count",
        help="Increase verbosity (default level: "
        + str(DEFAULT_CONFIG["VERBOSITY"])
        + ")",
    )
    parser.add_argument(
        "-q",
        action="count",
        help="Decrease verbosity (default level: "
        + str(DEFAULT_CONFIG["VERBOSITY"])
        + ")",
    )
    return parser


def _validate_args(args):
    if args.v is not None and args.q is not None:
        raise argparse.ArgumentTypeError("Cannot increase and decrease verbosity!")
    return args


def _set_str_arg(
    name_conf: str, value: str, config, config_location, cast_func=lambda x: x
):
    if value is not None:
        config[name_conf] = cast_func(value)
        config_location[name_conf] = POSSIBLE_LOCATION_CONFIG.CMD_ARG
    return config, config_location


def overwrite_with_args(args, config, config_location):
    for name_conf, value, cast_func in [
        ("URL", args.url, str),
        ("PDF", args.pdf, Path),
        ("FORMAT", args.format, lambda x: PDF_Formats.__members__[x.strip()]),
        ("OUT_CSV", args.out, Path),
        ("BATCH", args.batch, Path),
        ("BATCH_WORKERS", args.workers, int),
    ]:
        config, config_location = _set_str_arg(
            name_conf, value, config, config_location, cast_func
        )

    if args.no_download:
        config["SAVE_PDF"] = False
        config_location["SAVE_PDF"] = POSSIBLE_LOCATION_CONFIG.CMD_ARG
    increase_verbosity = 0
    if args.v is not None:
        increase_verbosity = args.v
    elif args.q is not None:
        increase_verbosity = -args.q

    if increase_verbosity != 0:
        config["VERBOSITY"] = min(
            max(DEFAULT_CONFIG["VERBOSITY"] + increase_verbosity, 0), 5
        )
        config_location["VERBOSITY"] = POSSIBLE_LOCATION_CONFIG.CMD_ARG

    return config, config_location


def cmd():
    """Command called when launching `freeports` from terminal,
    it calls the `main` function.
    """
    global RESULTING_CONFIG
    global RESULTING_LOCATION_CONFIG
    log_level = (5 - RESULTING_CONFIG["VERBOSITY"]) * 10
    log.basicConfig(level=log_level)
    parser = _create_parser()
    args = _validate_args(parser.parse_args())
    RESULTING_CONFIG, RESULTING_LOCATION_CONFIG = get_config_file(
        RESULTING_CONFIG, RESULTING_LOCATION_CONFIG
    )
    if args.config is not None:
        RESULTING_CONFIG["CONFIG_FILE"] = args.config
        RESULTING_LOCATION_CONFIG["CONFIG_FILE"] = POSSIBLE_LOCATION_CONFIG.CMD_ARG
    RESULTING_CONFIG, RESULTING_LOCATION_CONFIG = apply_config(
        RESULTING_CONFIG, RESULTING_LOCATION_CONFIG
    )
    RESULTING_CONFIG, RESULTING_LOCATION_CONFIG = overwrite_with_args(
        args, RESULTING_CONFIG, RESULTING_LOCATION_CONFIG
    )
    log_level = (5 - RESULTING_CONFIG["VERBOSITY"]) * 10
    log.getLogger().setLevel(log_level)
    log_resultig_config(logger)
    validate_conf(RESULTING_CONFIG)
    main(RESULTING_CONFIG)
