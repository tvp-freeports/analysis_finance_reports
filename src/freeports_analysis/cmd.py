import argparse
from .consts import PDF_Formats, ENV_PREFIX
from .main import main
import logging as log
import os

default_verbosity = 2
default_out_csv = "/dev/stdout"

logger = log.getLogger(__name__)


def create_parser():
    parser = argparse.ArgumentParser(
        description="Analize finance reports searching for investing in companies allegedly involved interantional law violations by third parties"
    )
    # Argomenti obbligatori (stringhe)
    parser.add_argument(
        "--url", "-u", type=str, help="URL of the dir where to find the pdf"
    )
    parser.add_argument("--pdf", "-i", type=str, help="Name of the file")
    parser.add_argument(
        "--format", "-f", type=str, choices=PDF_Formats.__members__, help="PDF format"
    )
    parser.add_argument(
        "--no-download", action="store_true", help="Don't save file locally"
    )
    parser.add_argument(
        "--out",
        "-o",
        type=str,
        help="Output file cvs (default: stdout)",
        default=default_out_csv,
    )
    parser.add_argument("-v", action="count", default=0, help="Verbosity level")
    parser.add_argument("-q", action="count", default=0, help="Quiet level")
    return parser


def validate_args(args):
    if args.v != 0 and args.q != 0:
        raise argparse.ArgumentTypeError("Cannot specify quiet and verbose!")
    return args


def cmd():
    """Command called when launching `freeports` from terminal"""
    parser = create_parser()
    args = validate_args(parser.parse_args())
    if args.url is not None:
        os.environ[f"{ENV_PREFIX}URL"] = args.url
    if args.pdf is not None:
        os.environ[f"{ENV_PREFIX}PDF"] = args.pdf
    if args.format is not None:
        os.environ[f"{ENV_PREFIX}PDF_FORMAT"] = args.format
    if args.no_download:
        os.environ[f"{ENV_PREFIX}SAVE_PDF"] = None
    if args.out != default_out_csv:
        os.environ[f"{ENV_PREFIX}OUT_CSV"] = args.out
    os.environ[f"{ENV_PREFIX}VERBOSITY"] = str(
        min(max(default_verbosity + args.v - args.q, 0), 5)
    )
    log_level = (5 - int(os.getenv(f"{ENV_PREFIX}VERBOSITY"))) * 10
    log.basicConfig(level=log_level)
    save_pdf = os.getenv(f"{ENV_PREFIX}SAVE_PDF") is not None
    wanted_format = os.getenv(f"{ENV_PREFIX}PDF_FORMAT")
    format_selected = (
        PDF_Formats.__members__[wanted_format] if wanted_format is not None else None
    )
    main(save_pdf, format_selected)
