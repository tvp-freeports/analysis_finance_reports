import pyperf
from pathlib import Path
import pymupdf as pypdf
from lxml import etree
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import ExtractedPdfLine


root_dir = Path(__file__).parent
pdf_file = pypdf.Document(root_dir / "report.pdf")
parser = etree.XMLParser(recover=True)
page_doc = pdf_file[100]
xml_str = page_doc.get_text("xml")
xml_tree = etree.fromstring(xml_str.encode(), parser=parser)
xml_blks = xml_tree.xpath("//line")
