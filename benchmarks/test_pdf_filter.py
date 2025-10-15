from freeports_analysis.formats.utils.pdf_filter.pdf_parts import ExtractedPdfLine
from .conftest import xml_blks


def test_ExtractedPdfLine(benchmark):
    def init_ExtractedPdfLine():
        return [ExtractedPdfLine(blk) for blk in xml_blks]

    result = benchmark(init_ExtractedPdfLine)
