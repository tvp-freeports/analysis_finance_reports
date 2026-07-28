"""MEDIOLANUM-ES24.Bax"""

from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.utils.pdf_extract import PdfExtractSfdrArticleStandard
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.utils.text_filter import TextFilterSfdrArticleStandard
from freeports_analysis.formats.utils.deserialize import DeserializeSfdrArticleStandard
from freeports_analysis.output import Fund


def deserialize_fund(txt_blk):
    return Fund(name=txt_blk.content)


pipelines = {
    "sfdr_classification": Pipeline(
        pdf_extract=PdfExtractSfdrArticleStandard(
            PdfLineSelection.text(
                "Información periódica de los productos financieros a que se refiere el artículo 9"
            ),
            PdfLineSelection.text(
                "Información periódica de los productos financieros a que se refiere el artículo 8"
            ),
            PdfLineSelection.area_from_bounds(
                x0=PdfLineSelection.text("significa una inversión"),
                x1=1e6,
                y0=PdfLineSelection.text("eglamento (UE) 2020/852"),
                y1=PdfLineSelection.text("dentificador de entidad"),
            )
            & PdfLineSelection(font="calibri"),
        ),
        text_filter=TextFilterSfdrArticleStandard(demand_investment_funds_match=False),
        deserialize=[DeserializeSfdrArticleStandard(), deserialize_fund],
    )
}
