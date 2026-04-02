from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractManagmentCompanyStandard,
)
from freeports_analysis.formats.utils.text_filter import (
    TextFilterManagmentCompanyStandard,
)
from freeports_analysis.formats.utils.deserialize import (
    DeserializerManagmentCompanyStandard,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import PdfLineSelection
from freeports_analysis.formats.algorithms.commons import Pipeline

pipelines = {
    "manco": Pipeline(
        pdf_extract=PdfExtractManagmentCompanyStandard(
            PdfLineSelection.area_from_movewindow(
                PdfLineSelection(
                    font="frutiger45lightbold",
                    font_size=(9.9, 10.1),
                    text="Management Company and Domiciliation Agent",
                ),
                (-0.2, 1.9),
                1.1,
                1.3,
            )
        ),
        text_filter=TextFilterManagmentCompanyStandard(),
        deserialize=DeserializerManagmentCompanyStandard(),
    )
}
