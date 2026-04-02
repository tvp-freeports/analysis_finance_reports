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
                    font="arial", font_size=(9.9, 10.1), text="Società di gestione:"
                ),
                (-0.2, 1.5),
                100.0,
                1.2,
            )
        ),
        text_filter=TextFilterManagmentCompanyStandard(),
        deserialize=DeserializerManagmentCompanyStandard(),
    )
}
