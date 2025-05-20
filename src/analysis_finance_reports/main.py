import os
import xml.etree.ElementTree as ET
import dotenv
import pymupdf as pypdf
import download as dw
import logging as log
import filter_pdf as fl
import search as sch
from typing import Optional
from consts import ENV_PREFIX, DATA_FILE_TARGET, PDF_Formats
import csv
import re

logger=log.getLogger(__name__)

class NoPDFormatDetected(Exception):
    pass

def lines_to_dict()
    pass

def save_cvs()
    pass

def main(save_pdf: bool,format_selected: Optional[PDF_Formats] ):
    pdf=os.getenv(f"{ENV_PREFIX}PDF")
    url=os.getenv(f"{ENV_PREFIX}URL")
    detected_format=None
    if url is None or os.path.exists(pdf):
        logger.debug("PDF: %s",pdf)
        pdf_file = pypdf.Document(pdf)
        for fmt in PDF_Formats.__members__:
            for reg in fmt.value:
                if bool(re.search(reg,url)):
                    detected_format=fmt
                    break
    else:
        logger.debug("URL: %s/%s",url,pdf)
        pdf_file=pypdf.Document(stream=dw.download_pdf(url,pdf,save_pdf))
    
    targets=[]
    with open(DATA_FILE_TARGET,'r',encoding='utf-8') as f:
        target_csv=csv.reader(f)
        targets=[row[0] for row in target_csv]
        targets.pop(0)
    logger.debug("First 5 targets: %s",str(targets[:min(5,len(targets))]))
    if detected_format is None and format_selected is None:
        raise NoPDFormatDetected("No format selected and url doesn't match know formats")
    if detected_format is not None and format_selected!=detected_format:
        logger.warning("Detected and selected formats don't match [det=%f sel=%s]",detected_format.name,format_selected.name)

    pdf_text=fl.extract_text(format_pdf,pdf_file)
    lines=sch.get_relevant_lines()







if __name__=="__main__":
    dotenv.load_dotenv()
    log_level=(5-int(os.getenv(f"{ENV_PREFIX}VERBOSITY")))*10
    log.basicConfig(level=log_level)
    save_pdf=os.getenv(f"{ENV_PREFIX}SAVE_PDF") is not None
    wanted_format=os.getenv(f"{ENV_PREFIX}PDF_FORMAT")
    format_selected=PDF_Formats.__members__[wanted_format] if wanted_format is not None else None
    main(save_pdf,format_selected)





# print(PDF_DOC)
# doc = pdf.open(PDF_DOC)
# print("prova")
# print(doc.get_toc()[2:12])
# print(doc.metadata)
# d=doc[0].get_text("xml")
# d=ET.fromstring(d)
# for l in d.findall('.//line'):
#     for e in l.findall('.//char'):
#         print(e.get('c'),end='')
#     print('')
#     #print(page)
#     #print(page.get_text())

# def print_all_tags(element, depth=0):
#     """Recursively prints all element tags with indentation."""
#     print('  ' * depth + element.tag + ' ' + str(element.attrib) + ' --> '+ str(element.text) + ' __tail: ' + str(element.tail))
#     for child in element:
#         print_all_tags(child, depth + 1)

# #print_all_tags(d)