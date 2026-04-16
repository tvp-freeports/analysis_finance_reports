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
from freeports_analysis.formats.templates import mediolanum_24

compute_page_class = mediolanum_24.compute_page_class

pipelines = {
    "inv_managers_begin": Pipeline(
        pdf_extract=mediolanum_24.inv_managers.PdfExtractBeginPage("^DELEGATE INV"),
        text_filter=mediolanum_24.inv_managers.text_filter_begin_page,
        deserialize=(
            mediolanum_24.inv_managers.deserialize,
            mediolanum_24.inv_managers.deserialize_fund,
            mediolanum_24.inv_managers.deserialize_manco,
        ),
    ),
    "inv_managers": Pipeline(
        pdf_extract=mediolanum_24.inv_managers.pdf_extract,
        text_filter=mediolanum_24.inv_managers.text_filter,
        deserialize=(
            mediolanum_24.inv_managers.deserialize,
            mediolanum_24.inv_managers.deserialize_fund,
        ),
    ),
    "inv_managers_end": Pipeline(
        pdf_extract=mediolanum_24.inv_managers.PdfExtractEndPage("^TRUSTEE"),
        text_filter=mediolanum_24.inv_managers.text_filter,
        deserialize=(
            mediolanum_24.inv_managers.deserialize,
            mediolanum_24.inv_managers.deserialize_fund,
        ),
    ),
}
