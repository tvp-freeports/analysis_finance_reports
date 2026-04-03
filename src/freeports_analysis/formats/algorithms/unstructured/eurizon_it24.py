"""Custom pdf filter for EURIZON-IT24"""

from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractCurrencyConstant,
    PdfExtractFundStandard,
    ExtractTextBlockOrFailPage,
    OnePdfBlockType,
)
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.utils.text_filter.match import MatchFund
from freeports_analysis.formats.utils.text_filter import (
    StandardManagmentCompanyTextBlock,
)
from freeports_analysis.formats.utils.deserialize import (
    DeserializerManagmentCompanyStandard,
)
from freeports_analysis.consts import Currency
from freeports_analysis.formats import PageParseFail
from freeports_analysis.output import Fund
import re

fund_set = PdfLineSelection(
    font="TrebuchetMSItalic", font_size=(4, 6.5), area=(270, 700, 595, 805)
)

body_set = PdfLineSelection.font("TrebuchetMS")

pdf_filter_manco = ExtractTextBlockOrFailPage(
    PdfLineSelection.text("^La società di gestione"),
    "managment company",
    OnePdfBlockType.RELEVANT_BLOCK,
)

manco_regex = re.compile("gestione ([^,]+)")


def text_filter_manco(pdf_blks, filter_data):
    funds = set(
        map(
            lambda x: MatchFund(x.name),
            filter(lambda x: isinstance(x, Fund), filter_data),
        )
    )
    m = manco_regex.search(pdf_blks[0].content)
    found = None
    if m:
        found = m.group(1).strip()
    else:
        raise PageParseFail("Managment regex didn't matched anything")
    return [StandardManagmentCompanyTextBlock.from_name(found, funds)]


deserialize_manco = DeserializerManagmentCompanyStandard()

deselection_list = [
    PdfLineSelection(font="TrebuchetMS", text="Totale"),
    PdfLineSelection(font="TrebuchetMS", text="Altri strumenti finanziari"),
]


pipelines = {
    "manco": Pipeline(
        pdf_extract=pdf_filter_manco,
        text_filter=text_filter_manco,
        deserialize=deserialize_manco,
    ),
    "investments": Pipeline(
        pdf_extract=(
            PdfExtractInvestmentsStandard(
                body_set=body_set,
                deselection_list=deselection_list,
            ),
            PdfExtractFundStandard(fund_set),
            PdfExtractCurrencyConstant(Currency.EUR),
        )
    ),
}
