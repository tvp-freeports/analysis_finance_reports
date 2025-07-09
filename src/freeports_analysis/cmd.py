"""Contains all the functions related to command line use of the `freeport` script"""

import argparse
import logging as log
from pathlib import Path
from typing import Tuple


from freeports_analysis.consts import PdfFormats, STANDARD_LOG_FORMATTER
from freeports_analysis.i18n import _
from freeports_analysis.conf_parse import (
    PossibleLocationConfig,
    DEFAULT_CONFIG,
    DEFAULT_LOCATION_CONFIG,
    log_config,
    apply_config,
    get_config_file,
)
from freeports_analysis.main import main


logger = log.getLogger()
stderr_log = log.StreamHandler()
stderr_log.setLevel(log.DEBUG)
stderr_log.setFormatter(STANDARD_LOG_FORMATTER)
logger.addHandler(stderr_log)


PROGRAM_DESCRIPTION = _("""Analyze finance reports searching for investing in companies
allegedly involved interantional law violations by third parties
""")


def _create_parser() -> argparse.ArgumentParser:
    """Create and set the parser for command line args

    Returns
    -------
    argparse.ArgumentParser
        class that contains the command line options, retrieve and validate them
    """
    parser = argparse.ArgumentParser(description=PROGRAM_DESCRIPTION)
    # Argomenti obbligatori (stringhe)
    parser.add_argument(
        "--url", "-u", type=str, help=_("URL of the dir where to find the pdf")
    )
    parser.add_argument("--pdf", "-i", type=str, help=_("Name of the file"))
    parser.add_argument(
        "--batch",
        "-b",
        type=str,
        help=_("Activate `BATCH MODE`, path of the batch file"),
    )
    help_str = _(
        "# parallel workers in `BATCH MODE`, if num <= 0, it set to # cpu avalaibles"
    )
    parser.add_argument("--workers", "-j", type=int, help=help_str)
    parser.add_argument(
        "--format", "-f", type=str, choices=PdfFormats.__members__, help=_("PDF format")
    )
    parser.add_argument(
        "--no-download", action="store_true", help=_("Don't save file locally")
    )
    parser.add_argument(
        "--separate-out", action="store_true", help=_("Separate output files")
    )
    parser.add_argument(
        "--config", type=str, help=_("Custom configuration file location")
    )
    out_csv = DEFAULT_CONFIG["OUT_CSV"]
    parser.add_argument(
        "--out",
        "-o",
        type=str,
        help=_("Output file cvs (default path: '{}')").format(out_csv),
    )
    verb = DEFAULT_CONFIG["VERBOSITY"]
    parser.add_argument(
        "-v",
        action="count",
        help=_("Increase verbosity (default level: {})").format(verb),
    )
    parser.add_argument(
        "-q",
        action="count",
        help=_("Decrease verbosity (default level: {})").format(verb),
    )
    return parser


def _validate_args(args):
    if args.v is not None and args.q is not None:
        raise argparse.ArgumentTypeError(_("Cannot increase and decrease verbosity!"))
    return args


def _set_str_arg(
    name_conf: str, value: str, config, config_location, cast_func=lambda x: x
):
    if value is not None:
        config[name_conf] = cast_func(value)
        config_location[name_conf] = PossibleLocationConfig.CMD_ARG
    return config, config_location


def overwrite_with_args(args, config: dict, config_location: dict) -> Tuple[dict, dict]:
    """Overwrite configuration provided and update the dictionary containing
    from where the configuration are loaded from accordingly, using command line arguments

    Parameters
    ----------
    config : dict
        configuration to overwrite
    config_location : dict
        location of configuration to update

    Returns
    -------
    Tuple[dict,dict]
        first `dict` is the new configuration, second is the updated location `dict`
    """
    for name_conf, value, cast_func in [
        ("URL", args.url, str),
        ("PDF", args.pdf, Path),
        ("FORMAT", args.format, lambda x: PdfFormats.__members__[x.strip()]),
        ("OUT_CSV", args.out, Path),
        ("BATCH", args.batch, Path),
        ("N_WORKERS", args.workers, int),
    ]:
        config, config_location = _set_str_arg(
            name_conf, value, config, config_location, cast_func
        )

    if args.no_download:
        config["SAVE_PDF"] = False
        config_location["SAVE_PDF"] = PossibleLocationConfig.CMD_ARG
    increase_verbosity = 0
    if args.v is not None:
        increase_verbosity = args.v
    elif args.q is not None:
        increase_verbosity = -args.q

    if increase_verbosity != 0:
        config["VERBOSITY"] = min(
            max(DEFAULT_CONFIG["VERBOSITY"] + increase_verbosity, 0), 5
        )
        config_location["VERBOSITY"] = PossibleLocationConfig.CMD_ARG

    if args.separate_out:
        config["SEPARATE_OUT_FILES"] = True

    return config, config_location


def cmd():
    """Command called when launching `freeports` from terminal,
    it calls the `main` function.
    """
    config = DEFAULT_CONFIG
    config_location = DEFAULT_LOCATION_CONFIG
    log_level = (5 - config["VERBOSITY"]) * 10
    logger.setLevel(log_level)
    parser = _create_parser()
    args = _validate_args(parser.parse_args())
    config, config_location = get_config_file(config, config_location)
    if args.config is not None:
        config["CONFIG_FILE"] = args.config
        config_location["CONFIG_FILE"] = PossibleLocationConfig.CMD_ARG
    config, config_location = apply_config(config, config_location)
    config, config_location = overwrite_with_args(args, config, config_location)
    log_level = (5 - config["VERBOSITY"]) * 10
    logger.setLevel(log_level)
    log_config(logger, config, config_location)
    main(config)
