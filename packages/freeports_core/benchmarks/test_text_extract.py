# import freeports_lib
# from freeports_analysis.formats.algorithms import get_pipelines
# from .conftest import pdf_blks, target_companies
# import pytest


# text_filter = get_pipelines("CARNE-EN23")[""].text_filter


# @pytest.mark.benchmarks
# def test_text_filter(benchmark):
#     result = benchmark(text_filter, pdf_blks, target_companies)


# @pytest.mark.benchmarks
# def test_match_company(benchmark):
#     result = benchmark(
#         freeports_lib.text_filter.matcher.match_company, "fjdlsajfals", target_companies
#     )
