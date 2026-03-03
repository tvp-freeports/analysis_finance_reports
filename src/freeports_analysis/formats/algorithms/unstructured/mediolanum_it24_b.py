"""MEDIOLANUM_IT24_B format submodule"""

from freeports_analysis.formats.utils.pdf_filter import (
    PdfExtractInvestmentsStandard,
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.consts import Currency


def pdf_extract(dict_root):
    """This pdf filter calculate dynamically the sixe of the table using some bound text"""
    lines = pdflines_from_pagedict(dict_root)
    next_table = PdfLineSelection(
        font="Helvetica-Bold", text="Strumenti finanziari quotati"
    ).select(lines)

    if len(next_table) == 0:
        next_table = PdfLineSelection(
            font="Helvetica-Bold", text="STRUMENTI FINANZIARI QUOTATI"
        ).select(lines)

    body_low_limit = 700 if len(next_table) == 0 else next_table.bbox[3]
    std = PdfExtractInvestmentsStandard(
        subfund_set=PdfLineSelection(
            font="Helvetica",
            area=(150, 67, 1e6, 76),
        ),
        body_set=PdfLineSelection(
            font="Helvetica",
            area=(0.0, 100.0, 1e6, body_low_limit),
        ),
        currency_set=Currency.EUR,
        deselection_list=[PdfLineSelection.text("^ ")],
    )

    return std(dict_root)


pipelines = {"investments": Pipeline(pdf_extract=pdf_extract)}
