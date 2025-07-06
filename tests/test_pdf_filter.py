from conftest import data_dir, single_page_tests
import pytest
from pymupdf import Document
import importlib
import dill
from lxml import etree


@pytest.fixture
def xml_parser():
    return etree.XMLParser(recover=True)


@pytest.mark.parametrize(["fmt", "page"], single_page_tests)
def test_pdf_filter(fmt, page, xml_parser):
    pdf = Document(data_dir / fmt / "report.pdf")
    xml_str = pdf[page].get_text("xml")
    xml_tree = etree.fromstring(xml_str.encode(), parser=xml_parser)
    module = importlib.import_module(f"freeports_analysis.formats.{fmt.lower()}")
    pdf_blks = module.pdf_filter(xml_tree, page)
    # with (data_dir / fmt / f"pdf_blks-{page}.pkl").open("wb") as f:
    #     dill.dump(pdf_blks,f)
    reference_pdf_blks = None
    with (data_dir / fmt / f"pdf_blks-{page}.pkl").open("rb") as f:
        reference_pdf_blks = dill.load(f)

    assert pdf_blks == reference_pdf_blks
