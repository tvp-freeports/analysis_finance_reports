from page_layout import get_page
from freeports_analysis.main import get_targets
import dill


def create_plk_one_page(
    page_n,
    pdf_filter_func,
    text_extract_func,
    tabularize_func,
    print_rows=True,
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
    with open(f"pdf_blks-{page_n}.pkl", "wb") as f:
        dill.dump(blks, f)

    blks = text_extract_func(blks, get_targets())
    if print_txt_blks:
        for blk in blks:
            print(blk)
    else:
        print(f"Saved {len(blks)} text blocks...")
    with open(f"txt_blks-{page_n}.pkl", "wb") as f:
        dill.dump(blks, f)

    tab = []
    for blk in blks:
        tab.append(tabularize_func(blk))
    if print_rows:
        for row in tab:
            print(row)
    else:
        print(f"Saved {len(blks)} csv rows...")
    with open(f"rows-{page_n}.pkl", "wb") as f:
        dill.dump(tab, f)
