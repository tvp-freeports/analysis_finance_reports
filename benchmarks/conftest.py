import pyperf
from pathlib import Path
import pymupdf as pypdf
from lxml import etree
import freeports_lib
from freeports_analysis.formats.algorithms import get_pipelines
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import ExtractedPdfLine
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import PdfLineSet
from freeports_analysis.data import get_target_companies

pdf_filter = get_pipelines("CARNE-EN23")[""][0][0]

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

df_target_companies = get_target_companies("TEST")
target_companies = (
    freeports_lib.text_extract.matcher.CompanyMatchInfos.compile_from_pandas_df(
        df_target_companies
    )
)
pdf_blks = pdf_filter(xml_tree)
