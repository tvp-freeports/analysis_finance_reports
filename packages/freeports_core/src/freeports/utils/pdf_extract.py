"""Utilities for `pdf_extract` segment"""

# pylint: disable=unused-import
from freeports._internals.formats.utils.pdf_extract.pdf_blks_acquire import (
    pdfline_selection_from_dict,
    pdfline_selection_from_str,
    pdfimages_from_pagedict,
    pdflines_from_pagedict,
)
from freeports._internals.formats.utils.pdf_extract.position import (
    get_groups,
    get_table_coordinates,
    CellGeometry,
    SplittingState,
    NullableState,
    Limits,
    RowConfig,
    ColumnConfig,
    TableConfig,
    CollapseAlgorithm,
    TablePosAlgorithm,
    TablePosMeasureUnit,
)
from freeports._internals.formats.utils.pdf_extract.common import (
    ExtractTextPdfBlockOrFailPage,
    SelectExpectedText,
)
import freeports_lib

PdfLineSelection = freeports_lib.pdf_extract.select.PdfLineSelection
PdfLine = freeports_lib.pdf_extract.select.PdfLine
