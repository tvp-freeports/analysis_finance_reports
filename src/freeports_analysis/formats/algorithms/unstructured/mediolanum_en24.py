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
                PdfLineSelection(font_size=(9.40, 9.55), text="MANAGER AND GLOBAL"),
                (1.2, -0.1),
                100.0,
                1.2,
            )
        ),
        text_filter=TextFilterManagmentCompanyStandard(),
        deserialize=DeserializerManagmentCompanyStandard(),
    )
}
