"""Custom pdf filter for EURIZON-IT24"""

from freeports_analysis.formats.utils.pdf_extract import PdfExtractInvestmentsStandard
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.consts import Currency

subfund_set = PdfLineSelection(
    font="TrebuchetMSItalic", font_size=(4, 6.5), area=(270, 700, 595, 805)
)

currency_set = Currency.EUR

body_set = PdfLineSelection.font("TrebuchetMS")

deselection_list = [
    PdfLineSelection(font="TrebuchetMS", text="Totale"),
    PdfLineSelection(font="TrebuchetMS", text="Altri strumenti finanziari"),
]

pipelines = {
    "investments": Pipeline(
        pdf_extract=PdfExtractInvestmentsStandard(
            subfund_set=subfund_set,
            body_set=body_set,
            currency_set=currency_set,
            deselection_list=deselection_list,
        )
    )
}
