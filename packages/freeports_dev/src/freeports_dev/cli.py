import argparse
import os
import sys
from pathlib import Path


def _resolve_repo(repo_arg=None):
    if repo_arg:
        return Path(repo_arg).resolve()
    env = os.environ.get("FREEPORTS_FORMATS_REPO")
    if env:
        return Path(env).resolve()
    return Path.cwd()


def _cmd_test(args):
    import pytest

    repo = _resolve_repo(args.repo)
    if not (repo / "metadata" / "formats.csv").exists():
        print(
            f"Error: {repo} does not appear to be a formats repository "
            f"(missing metadata/formats.csv)"
        )
        sys.exit(1)

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
    repo = _resolve_repo(args.repo)
    if not (repo / "metadata" / "formats.csv").exists():
        print(
            f"Error: {repo} does not appear to be a formats repository "
            f"(missing metadata/formats.csv)"
        )
        sys.exit(1)

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
        page_type=args.page_type,
        n_page=args.page,
        base_out_path=base_path,
        base_in_path=base_path,
        report_file=args.report,
        filter_data=filter_data,
        noconfirm=args.noconfirm,
        skip_pdf_blks=args.skip_pdf_blks,
        skip_txt_blks=args.skip_txt_blks,
        skip_results=args.skip_results,
        print_results=not args.noprint_results,
        print_txt_blks=args.print_txt_blks,
        print_pdf_blks=args.print_pdf_blks,
    )


def _cmd_inspect_page(args):
    repo = _resolve_repo(args.repo)

    from freeports_dev.create_test_page import (
        get_page_dict,
        print_pdf_line_sets,
        print_pdf_blks_table_MD,
        print_pdf_blks_table_ASCII,
    )
    import freeports_engine

    Algorithm = freeports_engine.core.Algorithm
    from freeports_dev.input_db import get_test_companies as gtc

    base_path = repo / "tests" / "formats"
    report_file = args.report or (base_path / args.format / "report.pdf")

    from freeports._internals.formats.repo.metadata import get_formats

    a = Algorithm.load(repo, args.format, list(get_formats(repo).index))
    page = get_page_dict(str(report_file), args.page)

    if args.mode in ("structured", "semistructured", "unstructured"):
        if not args.strings:
            print("Error: --strings is required for line-set mode")
            sys.exit(1)
        print_pdf_line_sets(page, args.strings, mode=args.mode)
        return

    filter_data = args.filter_data
    if filter_data is None:
        filter_data = gtc(repo)

    if args.mode == "pdf_blks":
        pdf_blks = a.apply_pdf_extract(page, args.page_type)
        for blk in pdf_blks:
            print(blk)
    elif args.mode == "txt_blks":
        txt_blks = a.apply_text_filter(page, filter_data, args.page_type)
        for blk in txt_blks:
            print(blk)
    elif args.mode == "results":
        results = a.apply_deserialize(page, filter_data, args.page_type)
        for r in results:
            print(r)
    elif args.mode == "table_md":
        pdf_blks = a.apply_pdf_extract(page, args.page_type)
        print_pdf_blks_table_MD(pdf_blks)
    elif args.mode == "table_ascii":
        pdf_blks = a.apply_pdf_extract(page, args.page_type)
        print_pdf_blks_table_ASCII(pdf_blks)


def _cmd_inspect_document(args):
    repo = _resolve_repo(args.repo)

    import freeports_engine

    Algorithm = freeports_engine.core.Algorithm
    from freeports._internals.formats.repo.metadata import get_formats
    import pymupdf

    base_path = repo / "tests" / "formats"
    report_file = args.report or (base_path / args.format / "report.pdf")

    a = Algorithm.load(repo, args.format, list(get_formats(repo).index))
    pdf_file = pymupdf.Document(str(report_file))

    if args.page is not None:
        page = pdf_file[args.page - 1].get_text("dict")
        classification = a.classify_pages([page])
        label = classification[0] if classification[0] is not None else "unclassified"
        print(f"Page {args.page}: {label}")
    else:
        pages = [p.get_text("dict") for p in pdf_file]
        classifications = a.classify_pages(pages)
        for i, cls in enumerate(classifications, 1):
            label = cls if cls is not None else "unclassified"
            print(f"Page {i}: {label}")


def _cmd_init_repo(args):
    from freeports_dev.repo_init import init_format_repo

    target = Path(args.path).resolve()
    init_format_repo(target)


def _cmd_setup_input_db(args):
    repo = _resolve_repo(args.repo)
    from freeports_dev.input_db import copy_default_input_db

    copy_default_input_db(repo / "tests")
    print(f"Input DB created at {repo / 'tests' / 'input_db'}")


def main():
    parser = argparse.ArgumentParser(
        prog="freeports-dev",
        description="Development tools for freeports format repositories",
    )
    sub = parser.add_subparsers(dest="command")

    p_test = sub.add_parser("test", help="Run format tests via pytest")
    p_test.add_argument("--repo", "-r", help="Path to formats repository")
    p_test.add_argument(
        "--format", "-f", help="Run tests only for a specific format (e.g. AMUNDI-EN24)"
    )
    p_test.add_argument(
        "pytest_args", nargs=argparse.REMAINDER, help="Arguments forwarded to pytest"
    )

    p_make = sub.add_parser("make-tests", help="Create test fixtures for a format page")
    p_make.add_argument("--repo", "-r", help="Path to formats repository")
    p_make.add_argument(
        "--format", "-f", required=True, help="Format name (e.g. AMUNDI-EN24)"
    )
    p_make.add_argument("--page", "-p", type=int, required=True, help="Page number")
    p_make.add_argument(
        "--page-type", "-t", required=True, help="Page type (e.g. investments)"
    )
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
        "inspect-page", help="Inspect a PDF page for format development"
    )
    p_page.add_argument("--repo", "-r", help="Path to formats repository")
    p_page.add_argument("--format", "-f", required=True, help="Format name")
    p_page.add_argument("--page", "-p", type=int, required=True, help="Page number")
    p_page.add_argument(
        "--page-type",
        "-t",
        default="investments",
        help="Page type for pipeline modes (pdf_blks|txt_blks|results), default: investments",
    )
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
        help="Classify pages of a PDF document to determine their page types",
    )
    p_doc.add_argument("--repo", "-r", help="Path to formats repository")
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

    p_setup = sub.add_parser(
        "setup-input-db", help="Create tests/input_db/ with default TEST list"
    )
    p_setup.add_argument("--repo", "-r", help="Path to formats repository")

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
    elif args.command == "setup-input-db":
        _cmd_setup_input_db(args)
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()
