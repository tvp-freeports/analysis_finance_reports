import freeports_lib
from freeports_analysis.formats.algorithms import get_pipelines
from .conftest import pdf_blks, target_companies
import pytest


text_extract = get_pipelines("CARNE-EN23")[""].text_filter


@pytest.mark.benchmarks
def test_text_extract(benchmark):
    result = benchmark(text_extract, pdf_blks, target_companies)


@pytest.mark.benchmarks
def test_match_company(benchmark):
    result = benchmark(
        freeports_lib.text_extract.matcher.match_company,
        "fjdlsajfals",
        target_companies,
    )
