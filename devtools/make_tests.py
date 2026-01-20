from test_single_page import get_page
from freeports_analysis.data import get_target_companies, TARGET_LISTS
import freeports_lib
import dill


def create_plk_one_page(
    page_n,
    pdf_filter_func,
    text_extract_func,
    deserialize_func,
    print_financial_data=True,
    print_txt_blks=False,
    print_pdf_blks=False,
):
    page = get_page("report.pdf", page_n)
    blks = pdf_filter_func(page)
    if print_pdf_blks:
        for blk in blks:
            print(blk)
    else:
        print(f"Saved {len(blks)} pdf blocks...")
    with open(f"{page_n}-pdf_blks.pkl", "wb") as f:
        dill.dump(blks, f)
    targets = get_target_companies(TARGET_LISTS)
    targets = (
        freeports_lib.text_extract.matcher.CompanyMatchInfos.compile_from_pandas_df(
            targets
        )
    )
    blks = text_extract_func(blks, targets)
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
