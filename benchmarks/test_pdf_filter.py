from freeports_analysis.formats.utils.pdf_filter.pdf_parts import pdflines_from_pagedict
from freeports_analysis.formats.algorithms import get_pipelines
from freeports_analysis.formats.utils.pdf_filter.select_position import (
    get_table_coordinates,
)
from .conftest import root_dict, body_blks
import pytest

pdf_filter = get_pipelines("CARNE-EN23")[""].pdf_extract


@pytest.mark.benchmarks
def test_ExtractedPdfLine(benchmark):
    def init_ExtractedPdfLine(page):
        return pdflines_from_pagedict(page)

    result = benchmark(init_ExtractedPdfLine, root_dict)


@pytest.mark.benchmarks
def test_pdf_filter(benchmark):
    result = benchmark(pdf_filter, root_dict)


@pytest.mark.benchmarks
def test_get_table_positions(benchmark):
    result = benchmark(get_table_coordinates, body_blks)
