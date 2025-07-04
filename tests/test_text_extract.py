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
def test_pdf_filter(fmt, page, targets):
    pdf_blks = None
    with (data_dir / fmt / f"pdf_blks-{page}.pkl").open("rb") as f:
        pdf_blks = dill.load(f)

    module = importlib.import_module(f"freeports_analysis.formats.{fmt}")
    txt_blks = module.text_extract(pdf_blks, targets)
    # with (data_dir / fmt / f"txt_blks-{page}.pkl").open("wb") as f:
    #     dill.dump(txt_blks,f)
    reference_txt_blks = None
    with (data_dir / fmt / f"txt_blks-{page}.pkl").open("rb") as f:
        reference_txt_blks = dill.load(f)

    assert txt_blks == reference_txt_blks
