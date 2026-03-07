import pytest
import pandas as pd
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
    run_alghoritm,
    get_output,
)

a = get_algorithm(__file__)
doc = get_pdf_document(__file__)
pages_type = get_pages_with_type(__file__)
expected_pdf_blocks = get_expected_pdf_blocks(__file__, pages_type)
expected_text_blocks = get_expected_text_blocks(__file__, pages_type)
expected_results = get_expected_results(__file__, pages_type)


@pytest.mark.parametrize("page,page_type", pages_type)
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


@pytest.mark.integration_tests
def test_pipeline():
    res = run_alghoritm(__file__)
    expected = get_output(__file__)
    pd.testing.assert_frame_equal(
        res["investments"]
        .sort_values(by=res["investments"].columns.tolist())
        .reset_index(drop=True),
        expected["investments"]
        .sort_values(by=expected["investments"].columns.tolist())
        .reset_index(drop=True),
    )
    assert (
        res["investments additional infos"] == expected["investments additional infos"]
    )
    pd.testing.assert_frame_equal(
        res["log"].sort_values(by=res["log"].columns.tolist()).reset_index(drop=True),
        expected["log"]
        .sort_values(by=expected["log"].columns.tolist())
        .reset_index(drop=True),
    )
