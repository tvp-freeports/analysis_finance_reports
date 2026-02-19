from freeports_analysis.formats.utils.pdf_filter.pdf_parts import pdfline_from_xml
from freeports_analysis.formats.algorithms import get_pipelines
from freeports_analysis.formats.utils.pdf_filter.select_position import (
    get_table_coordinates,
)
from .conftest import xml_blks, xml_tree, body_blks
import pytest

pdf_filter = get_pipelines("CARNE-EN23")[""][0][0]


@pytest.mark.benchmarks
def test_ExtractedPdfLine(benchmark):
    def init_ExtractedPdfLine(blks):
        return [pdfline_from_xml(blk) for blk in blks]

    result = benchmark(init_ExtractedPdfLine, xml_blks)


@pytest.mark.benchmarks
def test_pdf_filter(benchmark):
    result = benchmark(pdf_filter, xml_tree)


@pytest.mark.benchmarks
def test_get_table_positions(benchmark):
    result = benchmark(get_table_coordinates, body_blks)
