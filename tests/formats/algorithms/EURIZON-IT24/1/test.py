import pytest
from .. import (
    expected_results,
    expected_page_type,
    expected_pdf_blocks,
    expected_text_blocks,
    pytest_generate_tests,
    classified_pages,
    page_types,
    current_dir,
    pdf_extract,
    text_filter,
    deserialize,
    algorithm,
    format_name,
    pdf_document,
    test_targets,
)


@pytest.mark.path(__file__)
def test_page_classification(page, expected_page_type, pdf_document, algorithm):
    assert expected_page_type(page) == algorithm.classify_page(pdf_document, page)


@pytest.mark.path(__file__)
def test_pdf_extract(page, expected_pdf_blocks, pdf_extract):
    assert expected_pdf_blocks(page) == pdf_extract(page)


@pytest.mark.path(__file__)
def test_text_filter(page, expected_text_blocks, text_filter, test_targets):
    assert expected_text_blocks(page) == text_filter(page, test_targets)


@pytest.mark.path(__file__)
def test_deserialize(page, expected_results, deserialize):
    assert expected_results(page) == deserialize(page)


# @pytest.mark.integration_tests
# def test_pipeline():
#     generic_test_pipelines(__file__)
