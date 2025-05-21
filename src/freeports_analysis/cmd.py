import argparse
from freeports_analysis.consts import PDF_Formats, ENV_PREFIX
from freeports_analysis.main import main
import os
def create_parser():
    parser = argparse.ArgumentParser(description="Analize finance reports searching for investing in illegals companies")
    # Argomenti obbligatori (stringhe)
    parser.add_argument("--url",'-u', type=str, help="URL of the dir where to find the pdf")
    parser.add_argument("--pdf",'-i', type=str, help="Name of the file")
    parser.add_argument("--format",'-f', type=str, choices=PDF_Formats.__members__, help="PDF format")
    parser.add_argument("--no-download", action="store_true",help="Don't save file locally")
    parser.add_argument("--out",'-o', type=str, help="Output file cvs (default: stdout)",default='/dev/stdout')
    parser.add_argument("-v", action="count", default=0, help="Verbosity level")
    parser.add_argument("-q", action="count", default=0, help="Quiet level")
    return parser

def validate_args(args):
    if args.v !=0 and args.q != 0:
        raise argparse.ArgumentTypeError("Cannot specify quiet and verbose!")
    return args

def cmd():
    parser = create_parser()
    args = validate_args(parser.parse_args())
    if args.url is not None:
        os.environ[f"{ENV_PREFIX}URL"]=args.url
    if args.pdf is not None:
        os.environ[f"{ENV_PREFIX}PDF"]=args.pdf
    if args.format is not None:
        os.environ[f"{ENV_PREFIX}PDF_FORMAT"]=args.format
    if args.no_download:
        os.environ[f"{ENV_PREFIX}SAVE_PDF"]=None
    if args.out:
        os.environ[f"{ENV_PREFIX}OUT_CSV"]=args.out
    elif os.getenv(f"{ENV_PREFIX}OUT_CSV") is None:
        os.environ[f"{ENV_PREFIX}OUT_CSV"]='report.csv'
    if args.v!=0 or args.q!=0:
        os.environ[f"{ENV_PREFIX}VERBOSITY"]=str(min(max(args.v-args.q,0),5))
    else:
        os.environ[f"{ENV_PREFIX}VERBOSITY"]="1"
    save_pdf=os.getenv(f"{ENV_PREFIX}SAVE_PDF") is not None
    wanted_format=os.getenv(f"{ENV_PREFIX}PDF_FORMAT")
    format_selected=PDF_Formats.__members__[wanted_format] if wanted_format is not None else None
    main(save_pdf,format_selected)