import pytest
from tests.formats.algorithms import (
    generic_test_pdf_filter,
    generic_test_text_extract,
    generic_test_deserialize,
    generic_test_pipelines,
    get_pages,
)

pages = get_pages(__file__)


@pytest.mark.parametrize("page", pages)
def test_page_classification(page):
    generic_test_page_classification(page, __file__)


@pytest.mark.parametrize("page", pages)
def test_pdf_filter(page):
    generic_test_pdf_filter(page, __file__)


@pytest.mark.parametrize("page", pages)
def test_text_extract(page):
    generic_test_text_extract(page, __file__)


@pytest.mark.parametrize("page", pages)
def test_deserialize(page):
    generic_test_deserialize(page, __file__)


@pytest.mark.integration_tests
def test_pipeline():
    generic_test_pipelines(__file__)
