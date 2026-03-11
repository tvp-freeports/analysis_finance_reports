from test_single_page import get_page, get_pdf_blocks, get_text_blocks, get_results
from freeports_analysis.data import get_target_companies
from freeports_analysis.formats.algorithms import Algorithm
import freeports_lib
from pathlib import Path
import dill


def overwrite_pkl_one_page(
    fmt,
    document,
    page_type,
    n_page,
    filter_data=None,
    pdf_blks=True,
    txt_blks=True,
    results=True,
    print_financial_data=True,
    print_txt_blks=False,
    print_pdf_blks=False,
):
    if filter_data is None:
        targets = get_target_companies(["TEST"])
        filter_data = (
            freeports_lib.text_filter.matcher.CompanyMatchInfos.compile_from_pandas_df(
                targets
            )
        )
    selected_dir = (
        Path("..")
        / "tests"
        / "formats"
        / "algorithms"
        / fmt
        / ("" if document is None else document)
        / "pages"
        / page_type
    )
    blks = get_pdf_blocks(fmt, document, page_type, n_page, only_computed=True)
    if print_pdf_blks:
        for blk in blks:
            print(blk)
    elif pdf_blks:
        print(f"Saved {len(blks)} pdf blocks...")
    if pdf_blks:
        with open(selected_dir / f"{n_page}-pdf_blks.pkl", "wb") as f:
            dill.dump(blks, f)
    blks = get_text_blocks(
        fmt, document, page_type, n_page, filter_data, pdf_blks=blks, only_computed=True
    )
    if print_txt_blks:
        for blk in blks:
            print(blk)
    elif txt_blks:
        print(f"Saved {len(blks)} text blocks...")
    if txt_blks:
        with open(selected_dir / f"{n_page}-txt_blks.pkl", "wb") as f:
            dill.dump(blks, f)
    r = get_results(fmt, document, page_type, n_page, txt_blks=blks, only_computed=True)
    if print_financial_data:
        for res in r:
            print(res)
    elif results:
        print(f"Saved {len(r)} financial data...")
    if results:
        with open(selected_dir / f"{n_page}-results.pkl", "wb") as f:
            dill.dump(r, f)


def create_plk_one_page(
    page_n,
    pdf_extract_func,
    text_filter_func,
    deserialize_func,
    print_financial_data=True,
    print_txt_blks=False,
    print_pdf_blks=False,
):
    page = get_page("report.pdf", page_n)
    blks = pdf_extract_func(page)
    if print_pdf_blks:
        for blk in blks:
            print(blk)
    else:
        print(f"Saved {len(blks)} pdf blocks...")
    with open(f"{page_n}-pdf_blks.pkl", "wb") as f:
        dill.dump(blks, f)
    targets = get_target_companies(TARGET_LISTS)
    targets = (
        freeports_lib.text_filter.matcher.CompanyMatchInfos.compile_from_pandas_df(
            targets
        )
    )
    blks = text_filter_func(blks, targets)
    if print_txt_blks:
        for blk in blks:
            print(blk)
    else:
        print(f"Saved {len(blks)} text blocks...")
    with open(f"{page_n}-txt_blks.pkl", "wb") as f:
        dill.dump(blks, f)

    tab = []
    for blk in blks:
        tab.append(deserialize_func(blk))
    if print_financial_data:
        for row in tab:
            print(row)
    else:
        print(f"Saved {len(blks)} financial data...")
    with open(f"{page_n}-results.pkl", "wb") as f:
        dill.dump(tab, f)
