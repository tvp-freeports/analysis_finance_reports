"""The `freeports-dev` command line.

Every subcommand takes its settings from a :class:`~freeports_dev.config.DevConfig` rather than
reading the environment itself, so that one setting resolves the same way whichever subcommand asked
for it. The options shared with the engine — the repository, the input database, the configuration
file, the target lists — are declared once, on the parent parsers below, and carry the same names and
short letters `freeports` uses.
"""

import argparse
import sys
from pathlib import Path

from freeports_dev.config import DevConfig, set_active


def _repo_or_exit(config):
    """The formats repository, checked to actually be one.

    The same guard three subcommands had inline, in one place: pointing any of them at a directory
    that is not a formats repository fails the same way, with the same message.
    """
    repo = config.formats_repo
    if not (repo / "metadata" / "formats.csv").exists():
        print(
            f"Error: {repo} does not appear to be a formats repository "
            f"(missing metadata/formats.csv)"
        )
        sys.exit(1)
    return repo


def _cmd_test(args):
    import pytest

    config = DevConfig(args)
    repo = _repo_or_exit(config)
    set_active(config)

    test_dir = (
        repo / "tests" / "formats" / args.format if args.format else repo / "tests"
    )
    pytest_args = [str(test_dir), "--rootdir", str(repo)]
    extra = args.pytest_args
    if extra and extra[0] == "--":
        extra = extra[1:]
    pytest_args.extend(extra)
    sys.exit(pytest.main(pytest_args))


def _cmd_make_tests(args):
    config = DevConfig(args)
    repo = _repo_or_exit(config)

    from freeports_dev.make_tests import add_page_test

    base_path = repo / "tests" / "formats"
    filter_data = None
    if args.filter_data:
        import dill

        with open(args.filter_data, "rb") as f:
            filter_data = dill.load(f)

    add_page_test(
        fmt=args.format,
        document=args.document,
        page_type=config.page_type,
        n_page=args.page,
        base_out_path=base_path,
        base_in_path=base_path,
        report_file=args.report,
        filter_data=filter_data,
        noconfirm=config.noconfirm,
        skip_pdf_blks=args.skip_pdf_blks,
        skip_txt_blks=args.skip_txt_blks,
        skip_results=args.skip_results,
        print_results=not args.noprint_results,
        print_txt_blks=args.print_txt_blks,
        print_pdf_blks=args.print_pdf_blks,
    )


def _cmd_inspect_page(args):
    config = DevConfig(args)
    repo = config.formats_repo

    from freeports_dev.create_test_page import (
        get_page_dict,
        print_pdf_line_sets,
        print_pdf_blks_table_MD,
        print_pdf_blks_table_ASCII,
    )
    from freeports.core import Algorithm
    from freeports_dev.input_db import get_test_companies as gtc

    base_path = repo / "tests" / "formats"
    report_file = args.report or (base_path / args.format / "report.pdf")

    from freeports.formats_repo import get_formats

    a = Algorithm.load(repo, args.format, get_formats(repo))
    page = get_page_dict(str(report_file), args.page)

    if args.mode in ("structured", "semistructured", "unstructured"):
        if not args.strings:
            print("Error: --strings is required for line-set mode")
            sys.exit(1)
        print_pdf_line_sets(page, args.strings, mode=args.mode)
        return

    filter_data = args.filter_data
    if filter_data is None:
        filter_data = gtc(repo, config.target_lists, config)

    page_type = config.page_type
    if args.mode == "pdf_blks":
        pdf_blks = a.apply_pdf_extract(page, page_type)
        for blk in pdf_blks:
            print(blk)
    elif args.mode == "txt_blks":
        txt_blks = a.apply_text_filter(page, filter_data, page_type)
        for blk in txt_blks:
            print(blk)
    elif args.mode == "results":
        results = a.apply_deserialize(page, filter_data, page_type)
        for r in results:
            print(r)
    elif args.mode == "table_md":
        pdf_blks = a.apply_pdf_extract(page, page_type)
        print_pdf_blks_table_MD(pdf_blks)
    elif args.mode == "table_ascii":
        pdf_blks = a.apply_pdf_extract(page, page_type)
        print_pdf_blks_table_ASCII(pdf_blks)


def _cmd_inspect_document(args):
    repo = DevConfig(args).formats_repo

    from freeports.core import Algorithm
    from freeports.formats_repo import get_formats
    import pymupdf

    base_path = repo / "tests" / "formats"
    report_file = args.report or (base_path / args.format / "report.pdf")

    a = Algorithm.load(repo, args.format, get_formats(repo))
    pdf_file = pymupdf.Document(str(report_file))

    # The whole document is classified even when only one page is asked about, and that is not
    # waste. A format may supply a finalizer that rewrites the raw per-page answers looking at all
    # of them at once -- "every page after the holdings header is holdings" -- so a page classified
    # on its own can get a different answer from the same page classified in its document. The
    # isolated answer is the wrong one, and it is the one somebody would act on.
    pages = [p.get_text("dict") for p in pdf_file]
    classifications = a.classify_pages(pages)

    if args.page is not None:
        if not 1 <= args.page <= len(classifications):
            print(
                f"Error: page {args.page} is outside {report_file}, which has {len(classifications)}"
            )
            sys.exit(1)
        numbered = [(args.page, classifications[args.page - 1])]
    else:
        numbered = list(enumerate(classifications, 1))

    for number, page_class in numbered:
        print(
            f"Page {number}: {page_class if page_class is not None else 'unclassified'}"
        )


def _cmd_init_repo(args):
    from freeports_dev.repo_init import init_format_repo

    target = Path(args.path).resolve()
    init_format_repo(target)


def _cmd_init_input_db(args):
    from freeports_dev.repo_init import init_input_db

    init_input_db(Path(args.path).resolve(), sample=args.sample)


def _cmd_setup_input_db(args):
    repo = DevConfig(args).formats_repo
    from freeports_dev.input_db import copy_default_input_db

    copy_default_input_db(repo / "tests")
    print(f"Input DB created at {repo / 'tests' / 'input_db'}")


def _common_parser():
    """The options every subcommand that works on an existing repository accepts.

    Declared once and inherited, so the repository is named the same way whichever subcommand is
    being run. The long names and short letters are the engine's: `freeports` accepts
    `--formats-directory`/`--repo`/`-F`/`-r` and `--db-directory`/`-I` for the same two things, and a
    format author should not have to remember which command wanted which spelling.
    """
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument(
        "--repo",
        "-r",
        "--formats-directory",
        "-F",
        dest="repo",
        metavar="PATH",
        help="Formats repository [default: $FREEPORTS_FORMATS_REPO_PATH, then `formats_repo` in the "
        "configuration file, then the working directory]",
    )
    parser.add_argument(
        "--db-directory",
        "-I",
        dest="db_directory",
        metavar="PATH",
        help="Input database, overriding the repository's own tests/input_db "
        "[default: $FREEPORTS_INPUT_DB_PATH]",
    )
    parser.add_argument(
        "--config",
        metavar="PATH",
        help="Configuration file to read [default: $FREEPORTS_CONFIG_FILE, then the file the engine "
        "would find in the working, user and system tiers]",
    )
    return parser


def _targets_parser():
    """The target lists, for the subcommands that actually filter by company."""
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument(
        "--target-list",
        "-T",
        dest="target_list",
        nargs="+",
        metavar="NAME",
        help="Lists to search [default: $FREEPORTS_DEV_TARGET_LIST, then `dev.target_lists` in the "
        "configuration file, then TEST]",
    )
    return parser


def _page_type_parser():
    """The page type, defaulted through the configuration rather than hard-coded in the flag."""
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument(
        "--page-type",
        "-t",
        dest="page_type",
        metavar="TYPE",
        help="Page type [default: $FREEPORTS_DEV_PAGE_TYPE, then `dev.page_type` in the "
        "configuration file, then investments]",
    )
    return parser


def main():
    parser = argparse.ArgumentParser(
        prog="freeports-dev",
        description="Development tools for freeports format repositories",
    )
    sub = parser.add_subparsers(dest="command")
    common = _common_parser()
    targets = _targets_parser()
    page_type = _page_type_parser()

    p_test = sub.add_parser(
        "test", parents=[common, targets], help="Run format tests via pytest"
    )
    p_test.add_argument(
        "--format", "-f", help="Run tests only for a specific format (e.g. AMUNDI-EN24)"
    )
    p_test.add_argument(
        "pytest_args", nargs=argparse.REMAINDER, help="Arguments forwarded to pytest"
    )

    p_make = sub.add_parser(
        "make-tests",
        parents=[common, targets, page_type],
        help="Create test fixtures for a format page",
    )
    p_make.add_argument(
        "--format", "-f", required=True, help="Format name (e.g. AMUNDI-EN24)"
    )
    p_make.add_argument("--page", "-p", type=int, required=True, help="Page number")
    p_make.add_argument(
        "--document", "-d", help="Document variant (for multi-document formats)"
    )
    p_make.add_argument("--report", help="Path to report PDF (defaults to test dir)")
    p_make.add_argument("--filter-data", help="Path to filter data .pkl file")
    p_make.add_argument(
        "--noconfirm", action="store_true", help="Skip confirmation prompts"
    )
    p_make.add_argument(
        "--noprint_results", action="store_true", help="Suppress result printing"
    )
    p_make.add_argument(
        "--print_txt_blks", action="store_true", help="Activate text blocks printing"
    )
    p_make.add_argument(
        "--print_pdf_blks", action="store_true", help="Activate pdf blocks printing"
    )
    p_make.add_argument("--skip-pdf-blks", action="store_true")
    p_make.add_argument("--skip-txt-blks", action="store_true")
    p_make.add_argument("--skip-results", action="store_true")

    p_page = sub.add_parser(
        "inspect-page",
        parents=[common, targets, page_type],
        help="Inspect a PDF page for format development",
    )
    p_page.add_argument("--format", "-f", required=True, help="Format name")
    p_page.add_argument("--page", "-p", type=int, required=True, help="Page number")
    p_page.add_argument(
        "--mode",
        "-m",
        default="results",
        choices=[
            "structured",
            "semistructured",
            "unstructured",
            "pdf_blks",
            "txt_blks",
            "results",
            "table_md",
            "table_ascii",
        ],
        help=(
            "Inspection mode. "
            "Line-set modes (structured|semistructured|unstructured): "
            "search --strings on the page and print PdfLineSelection matching lines. "
            "Pipeline modes (pdf_blks|txt_blks|results): "
            "print output from the pdf_extract, text_filter, or full pipeline stage. "
            "Table modes (table_md|table_ascii): "
            "render pdf_blks with table-row and col-row metadata as a table"
        ),
    )
    p_page.add_argument(
        "--strings",
        nargs="*",
        help="Strings to search in the page (required for structured|semistructured|unstructured modes)",
    )
    p_page.add_argument("--report", help="Path to report PDF")
    p_page.add_argument(
        "--filter-data",
        help="Path to filter data .pkl file (for txt_blks and results modes)",
    )

    p_doc = sub.add_parser(
        "inspect-document",
        parents=[common],
        help="Classify pages of a PDF document to determine their page types",
    )
    p_doc.add_argument("--format", "-f", required=True, help="Format name")
    p_doc.add_argument(
        "--page",
        "-p",
        type=int,
        help="Specific page to classify (default: classify all pages)",
    )
    p_doc.add_argument("--report", help="Path to report PDF")

    p_init = sub.add_parser(
        "init-format-repo", help="Initialize a new format repository"
    )
    p_init.add_argument("path", help="Path for the new repository")

    p_init_db = sub.add_parser("init-input-db", help="Initialize a new input database")
    p_init_db.add_argument("path", help="Path for the new input database")
    p_init_db.add_argument(
        "--sample",
        action="store_true",
        help="Fill the tables with the packaged example database (list TEST) instead of leaving "
        "them empty",
    )

    sub.add_parser(
        "setup-input-db",
        parents=[common],
        help="Create tests/input_db/ with default TEST list",
    )

    args = parser.parse_args()

    if args.command == "test":
        _cmd_test(args)
    elif args.command == "make-tests":
        _cmd_make_tests(args)
    elif args.command == "inspect-page":
        _cmd_inspect_page(args)
    elif args.command == "inspect-document":
        _cmd_inspect_document(args)
    elif args.command == "init-format-repo":
        _cmd_init_repo(args)
    elif args.command == "init-input-db":
        _cmd_init_input_db(args)
    elif args.command == "setup-input-db":
        _cmd_setup_input_db(args)
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()
