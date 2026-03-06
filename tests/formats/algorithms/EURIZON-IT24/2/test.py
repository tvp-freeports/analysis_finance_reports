import pytest
from tests.formats.algorithms import (
    get_algorithm,
    get_pages_with_type,
    apply_pdf_extract,
    get_expected_pdf_blocks,
    get_pdf_document,
    get_expected_text_blocks,
    get_expected_results,
    apply_text_filter,
    apply_deserialize,
    test_companies,
)

a = get_algorithm(__file__)
doc = get_pdf_document(__file__)
pages_type = get_pages_with_type(__file__)
expected_pdf_blocks = get_expected_pdf_blocks(__file__, pages_type)
expected_text_blocks = get_expected_text_blocks(__file__, pages_type)
expected_results = get_expected_results(__file__, pages_type)


@pytest.mark.parametrize("page,ge_type", pages_type)
def test_page_classification(page, page_type):
    assert page_type == a.classify_page(doc, page)


@pytest.mark.parametrize("page,page_type", pages_type)
def test_pdf_extract(page, page_type):
    assert expected_pdf_blocks[page] == apply_pdf_extract(a, doc, page, page_type)


@pytest.mark.parametrize("page,page_type", pages_type)
def test_text_filter(page, page_type):
    assert expected_text_blocks[page] == apply_text_filter(
        a, expected_pdf_blocks[page], test_companies, page_type
    )


@pytest.mark.parametrize("page,page_type", pages_type)
def test_deserialize(page, page_type):
    assert expected_results[page] == apply_deserialize(
        a, expected_text_blocks[page], page_type
    )


# import pytest
# from tests.formats.algorithms import (
#     generic_test_pdf_filter,
#     generic_test_text_extract,
#     generic_test_deserialize,
#     generic_test_pipelines,
#     get_pages,
# )

# pages = get_pages(__file__)


# @pytest.mark.parametrize("page", pages)
# def test_pdf_filter(page):
#     generic_test_pdf_filter(page, __file__)


# @pytest.mark.parametrize("page", pages)
# def test_text_extract(page):
#     generic_test_text_extract(page, __file__)


# @pytest.mark.parametrize("page", pages)
# def test_deserialize(page):
#     generic_test_deserialize(page, __file__)


# @pytest.mark.integration_tests
# def test_pipeline():
#     generic_test_pipelines(__file__)


# @pytest.mark.path(__file__)
# def test_page_classification(page,expected_page_type,pdf_document,algorithm):
#     assert expected_page_type(page) == algorithm.classify_page(pdf_document,page)

# @pytest.mark.path(__file__)
# def test_pdf_extract(page,expected_pdf_blocks,pdf_extract):
#     assert expected_pdf_blocks(page) == pdf_extract(page)

# @pytest.mark.path(__file__)
# def test_text_filter(page,expected_text_blocks,text_filter,test_targets):
#     assert expected_text_blocks(page) == text_filter(page,test_targets)


# @pytest.mark.path(__file__)
# def test_deserialize(page,expected_results,deserialize):
#     assert expected_results(page) == deserialize(page)
