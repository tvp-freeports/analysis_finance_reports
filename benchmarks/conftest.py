import pyperf
from pathlib import Path
import pymupdf as pypdf
from lxml import etree
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import ExtractedPdfLine
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet


root_dir = Path(__file__).parent
pdf_file = pypdf.Document(root_dir / "report.pdf")
parser = etree.XMLParser(recover=True)
page_doc = pdf_file[25]
xml_str = page_doc.get_text("xml")
xml_tree = etree.fromstring(xml_str.encode(), parser=parser)
xml_blks = xml_tree.xpath("//line")
body_blks = [
    body_blk
    for body_blk in [ExtractedPdfLine(blk) for blk in xml_blks]
    if body_blk in PdfLineSet.from_str("ArialMT[6.96](160:786)")
]
