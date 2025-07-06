from conftest import data_dir, single_page_tests
import pytest
from pymupdf import Document
import importlib
import dill
from lxml import etree
import freeports_analysis as fra


@pytest.fixture
def targets():
    return fra.main.get_targets()


@pytest.mark.parametrize(["fmt", "page"], single_page_tests)
def test_deserialize(fmt, page, targets):
    txt_blks = None
    with (data_dir / fmt / f"txt_blks-{page}.pkl").open("rb") as f:
        txt_blks = dill.load(f)

    module = importlib.import_module(f"freeports_analysis.formats.{fmt.lower()}")
    financial_data = [module.deserialize(blk, targets) for blk in txt_blks]
    # with (data_dir / fmt / f"txt_blks-{page}.pkl").open("wb") as f:
    #     dill.dump(txt_blks,f)
    reference_financial_data = None
    with (data_dir / fmt / f"financial_data-{page}.pkl").open("rb") as f:
        reference_financial_data = dill.load(f)

    assert financial_data == reference_financial_data
