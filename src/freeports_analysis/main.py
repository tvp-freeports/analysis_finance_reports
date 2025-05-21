import os
import pymupdf as pypdf
from typing import List, Tuple
import freeports_analysis.download as dw
import logging as log
import freeports_analysis.filter_pdf as fl
import freeports_analysis.search as sch
from typing import Optional
from freeports_analysis.consts import ENV_PREFIX, PDF_Formats
import csv
import pandas as pd
from importlib_resources import files
import freeports_analysis.data
from freeports_analysis import extract_data
from freeports_analysis import filter_text
from freeports_analysis import pdf_parts

logger = log.getLogger(__name__)


class NoPDFormatDetected(Exception):
    pass


def get_functions(format: PDF_Formats):
    functions = {"relevant_pdf_parts": None, "filter_text": None, "extract_data": None}
    match format:
        case PDF_Formats.EURIZON:
            functions["relevant_pdf_parts"] = relevant_pdf_parts.EURIZON
            functions["filter_text"] = filter_text.EURIZON
            functions["extract_data"] = extract_data.EURIZON
        case _:
            functions["relevant_pdf_parts"] = relevant_pdf_parts._DEFAULT
            functions["filter_text"] = filter_text._DEFAULT
            functions["extract_data"] = extract_data._DEFAULT
    return functions


def main(save_pdf: bool, format_selected: Optional[PDF_Formats]):
    pdf = os.getenv(f"{ENV_PREFIX}PDF")
    url = os.getenv(f"{ENV_PREFIX}URL")
    detected_format = None
    if url is None or os.path.exists(pdf):
        logger.debug("PDF: %s", pdf)
        pdf_file = pypdf.Document(pdf)
    else:
        for fmt in PDF_Formats.__members__:
            for reg in PDF_Formats.__members__[fmt].value:
                if bool(re.search(reg, url)):
                    detected_format = PDF_Formats.__members__[fmt]
                    break
        logger.debug("URL: %s/%s", url, pdf)
        pdf_file = pypdf.Document(stream=dw.download_pdf(url, pdf, save_pdf))

    targets = []
    with files(freeports_analysis.data).joinpath("target.csv").open("r") as f:
        target_csv = csv.reader(f)
        targets = [row[0] for row in target_csv]
        targets.pop(0)
    logger.debug("First 5 targets: %s", str(targets[: min(5, len(targets))]))
    if detected_format is None and format_selected is None:
        raise NoPDFormatDetected(
            "No format selected and url doesn't match know formats"
        )
    if detected_format is not None and format_selected != detected_format:
        logger.warning(
            "Detected and selected formats don't match [det=%f sel=%s]",
            detected_format.name,
            format_selected.name,
        )
    format_pdf = detected_format if format_selected is None else format_selected

    functions = get_functions(format_pdf)
    logger.info("Extracting relevant parts of pdf...")
    pdf_parts = relevant_pdf_parts.get_relevant_parts(
        pdf_file, functions["relevant_pdf_parts"]
    )
    logger.info("Extracted!")
    logger.info("Filtering relevant parts of text...")
    filtered_text = filter_text.get_text_bits(pdf_parts, functions["filter_text"])
    logger.info("Filtered!")
    tabular_data = extract_data.tabularize(filtered_text, functions["extract_data"])
    df = pd.DataFrame(tabular_data)
    df.to_csv(os.getenv(f"{ENV_PREFIX}OUT_CSV"))


if __name__ == "__main__":
    import dotenv

    dotenv.load_dotenv()
    log_level = (5 - int(os.getenv(f"{ENV_PREFIX}VERBOSITY"))) * 10
    log.basicConfig(level=log_level)
    save_pdf = os.getenv(f"{ENV_PREFIX}SAVE_PDF") is not None
    wanted_format = os.getenv(f"{ENV_PREFIX}PDF_FORMAT")
    format_selected = (
        PDF_Formats.__members__[wanted_format] if wanted_format is not None else None
    )
    main(save_pdf, format_selected)


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
