from freeports_dev.create_test_page import (
    get_page,
    get_pdf_blocks,
    get_text_blocks,
    get_results,
    get_page_dict,
)
from freeports_dev.input_db import get_test_companies as gtc
from freeports.core import Algorithm
from freeports.formats_repo import get_formats
from freeports_dev.serialization import (
    dump as json_dump,
    to_serializable,
    from_serializable,
)
from pathlib import Path
import json
import shutil
import os


def user_confirm(question, default=False):
    y_text = "y"
    n_text = "n"
    if default:
        y_text = y_text.upper()
    else:
        n_text = n_text.upper()
    c = input(question + f" [{y_text}/{n_text}]: ")
    reply = None
    r = c.strip().lower()
    if r in ["y", "yes"]:
        reply = True
    elif r in ["n", "no"]:
        reply = False
    elif r == "":
        reply = default
    else:
        raise Exception(f"Input should be yes or no, found: {c}")
    return reply


def add_page_test(
    fmt,
    document,
    page_type,
    n_page,
    base_out_path,
    base_in_path,
    report_file=None,
    filter_data=None,
    noconfirm=False,
    skip_pdf_blks=False,
    skip_txt_blks=False,
    skip_results=False,
    print_results=True,
    print_txt_blks=False,
    print_pdf_blks=False,
):
    filter_data_file = (
        base_in_path
        / fmt
        / ("" if document is None else str(document))
        / "pages"
        / page_type
        / "filter_data.json"
    )
    out_filter_data_file = (
        base_out_path
        / fmt
        / ("" if document is None else str(document))
        / "pages"
        / page_type
        / "filter_data.json"
    )
    repo_root = base_in_path.parent.parent

    if filter_data is None:
        if filter_data_file.exists():
            with open(filter_data_file, "r", encoding="utf-8") as f:
                data = json.load(f)
            if isinstance(data, dict) and "target_lists" in data:
                filter_data = gtc(repo_root, data["target_lists"])
                print(f"Used filter data from {filter_data_file}")
            else:
                filter_data = from_serializable(data)
                print(f"Used custom filter data (filter_data.json present)")
        else:
            filter_data = gtc(repo_root)
        out_filter_data_file = None

    a = Algorithm.load(repo_root, fmt, get_formats(repo_root))
    page = None
    if report_file is not None:
        report_file = Path(report_file)
    if report_file is not None and not report_file.exists():
        print(
            f"Warning, specified a report file that doesn't exist {report_file}, "
            "overwriting with None"
        )
        report_file = None

    if report_file is not None:
        report_file = Path(report_file)
        page = get_page(report_file, n_page)

    pdf_blks = get_pdf_blocks(
        fmt,
        document,
        page_type,
        n_page,
        only_computed=True,
        algorithm=a,
        page=page,
        base_path=base_in_path,
    )
    if print_pdf_blks:
        for blk in pdf_blks:
            print(blk)
    else:
        print(f"Extracted {len(pdf_blks)} pdf blocks...")
    txt_blks = get_text_blocks(
        fmt,
        document,
        page_type,
        n_page,
        filter_data=filter_data,
        only_computed=True,
        algorithm=a,
        page=page,
        base_path=base_in_path,
    )
    if print_txt_blks:
        for blk in txt_blks:
            print(blk)
    else:
        print(f"Filtered {len(txt_blks)} text blocks...")

    results = get_results(
        fmt,
        document,
        page_type,
        n_page,
        filter_data=filter_data,
        only_computed=True,
        algorithm=a,
        page=page,
        base_path=base_in_path,
    )
    if print_results:
        for r in results:
            print(r)
    else:
        print(f"Computed {len(results)} results...")

    format_dir = base_out_path / fmt
    if not format_dir.exists():
        if noconfirm or user_confirm(
            f"Format directory {fmt} not present in {base_out_path}, "
            "do you want to create it?",
            default=True,
        ):
            format_dir.mkdir()
        else:
            print("Without format directory, page test creation cannot continue")
            return None
    else:
        for doc in os.listdir(str(format_dir)):
            if doc in ("pages", "out", "report.pdf") and document is not None:
                raise Exception(
                    f"Specified document variant {document}, but {format_dir} "
                    "seems a single report test layout"
                )

    document_dir = format_dir / ("" if document is None else str(document))
    if not document_dir.exists():
        if noconfirm or user_confirm(
            f"Document variant test directory not present in {format_dir}, "
            "do you want to create it?",
            default=True,
        ):
            document_dir.mkdir()
        else:
            print(
                "Without report variant directory, page test creation cannot continue"
            )
            return None

    report = document_dir / "report.pdf"
    if not report.exists():
        report_to_copy = (
            (
                base_in_path
                / fmt
                / ("" if document is None else str(document))
                / "report.pdf"
            )
            if report_file is None
            else report_file
        )
        if noconfirm or user_confirm(
            f"Report not present in {document_dir}, do you want to copy "
            f"{report_to_copy}?",
            default=True,
        ):
            shutil.copyfile(report_to_copy, report)
            print("Report file copied")
    else:
        if report_file is not None:
            if noconfirm or not user_confirm(
                f"Report present in {document_dir} but report {report_file} is used "
                "for computing results, overwrite?",
                default=False,
            ):
                print("Report file not overwritten")
            else:
                shutil.copyfile(report_file, report)
                print("Report file overwritten")

    pages_dir = document_dir / "pages"
    if not pages_dir.exists():
        if noconfirm or user_confirm(
            f"Pages test directory not present in {document_dir}, "
            "do you want to create it?",
            default=True,
        ):
            pages_dir.mkdir()
        else:
            print("Without pages directory, page test creation cannot continue")
            return None

    pages_type_dir = pages_dir / page_type
    if not pages_type_dir.exists():
        if noconfirm or user_confirm(
            f"Directory for tests of pages of type {page_type} not present in "
            f"{pages_dir}, do you want to create it?",
            default=True,
        ):
            pages_type_dir.mkdir()
        else:
            print("Without results directory, page test creation cannot continue")
            return None

    pdf_blks_file = pages_type_dir / f"{n_page}-pdf_blks.json"
    txt_blks_file = pages_type_dir / f"{n_page}-txt_blks.json"
    results_file = pages_type_dir / f"{n_page}-results.json"

    if not skip_pdf_blks:
        if pdf_blks_file.exists():
            if noconfirm or not user_confirm(
                f"Pdf blocks file for page {n_page} already present, "
                "do you want to overwrite it?",
                default=False,
            ):
                print("Kept original pdf blocks file")
            else:
                with open(pdf_blks_file, "w", encoding="utf-8") as f:
                    json_dump(pdf_blks, f)
                    print(f"Overwritten {pdf_blks_file}...")
        else:
            with open(pdf_blks_file, "w", encoding="utf-8") as f:
                json_dump(pdf_blks, f)
                print(f"Saved {len(pdf_blks)} pdf blocks in {pdf_blks_file}...")
    else:
        print("Skipping creation of pdf blocks file")

    if not skip_txt_blks:
        if txt_blks_file.exists():
            if noconfirm or not user_confirm(
                f"Text block file for page {n_page} already present, "
                "do you want to overwrite it?",
                default=False,
            ):
                print("Kept original text blocks file")
            else:
                with open(txt_blks_file, "w", encoding="utf-8") as f:
                    json_dump(txt_blks, f)
                    print(f"Overwritten {txt_blks_file}...")
        else:
            with open(txt_blks_file, "w", encoding="utf-8") as f:
                json_dump(txt_blks, f)
                print(f"Saved {len(txt_blks)} text blocks in {txt_blks_file}...")
        if out_filter_data_file is not None:
            if out_filter_data_file.exists():
                if noconfirm or not user_confirm(
                    f"Filter data file for page category {page_type} already present, "
                    "do you want to overwrite it?",
                    default=False,
                ):
                    print("Kept original filter data file")
                else:
                    with open(out_filter_data_file, "w", encoding="utf-8") as f:
                        json.dump(
                            to_serializable(filter_data),
                            f,
                            indent=2,
                            ensure_ascii=False,
                        )
                        print(f"Overwritten {out_filter_data_file}...")
            else:
                with open(out_filter_data_file, "w", encoding="utf-8") as f:
                    json.dump(
                        to_serializable(filter_data),
                        f,
                        indent=2,
                        ensure_ascii=False,
                    )
                    print(f"Saved filter data in {out_filter_data_file}...")
    else:
        print("Skipping creation of text blocks file")

    if not skip_results:
        if results_file.exists():
            if noconfirm or not user_confirm(
                f"Results file for page {n_page} already present, "
                "do you want to overwrite it?",
                default=False,
            ):
                print("Kept original results file")
            else:
                with open(results_file, "w", encoding="utf-8") as f:
                    json_dump(results, f)
                    print(f"Overwritten {results_file}...")
        else:
            with open(results_file, "w", encoding="utf-8") as f:
                json_dump(results, f)
                print(f"Saved {len(results)} results in {results_file}...")
    else:
        print("Skipping creation of results file")
