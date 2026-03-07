import pyperf
from pathlib import Path
import pymupdf as pypdf
from lxml import etree
import freeports_lib
from freeports_analysis.formats.algorithms import get_pipelines
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import pdflines_from_pagedict
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import (
    pdfline_selection_from_str,
)
from freeports_analysis.data import get_target_companies

pdf_filter = get_pipelines("CARNE-EN23")[""].pdf_extract

root_dir = Path(__file__).parent
pdf_file = pypdf.Document(root_dir / "report.pdf")
page_doc = pdf_file[25]
root_dict = page_doc.get_text("dict")
lines = pdflines_from_pagedict(root_dict)
body_blks = pdfline_selection_from_str("ArialMT[6.96](160:786)").select(lines)

df_target_companies = get_target_companies("TEST")
target_companies = (
    freeports_lib.text_extract.matcher.CompanyMatchInfos.compile_from_pandas_df(
        df_target_companies
    )
)
pdf_blks = pdf_filter(xml_tree)
