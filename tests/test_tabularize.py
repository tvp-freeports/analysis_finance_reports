from conftest import data_dir, single_page_tests
import pytest
from pymupdf import Document
import importlib
import dill
from lxml import etree
import freeports_analysis as fra


@pytest.mark.parametrize(["fmt", "page"], single_page_tests)
def test_tabularize(fmt, page):
    txt_blks = None
    with (data_dir / fmt / f"txt_blks-{page}.pkl").open("rb") as f:
        txt_blks = dill.load(f)

    module = importlib.import_module(f"freeports_analysis.formats.{fmt}")
    rows = [module.tabularize(blk) for blk in txt_blks]
    # with (data_dir / fmt / f"txt_blks-{page}.pkl").open("wb") as f:
    #     dill.dump(txt_blks,f)
    reference_rows = None
    with (data_dir / fmt / f"rows-{page}.pkl").open("rb") as f:
        reference_rows = dill.load(f)

    assert rows == reference_rows
