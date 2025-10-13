from typing import List, Optional
from pydantic import BaseModel
from freeports_analysis.formats.utils.pdf_filter.pdf_parts import (
    InputPdfLineSet,
    PdfLineSet,
)
from freeports_analysis.formats.utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats.utils.pdf_filter.select_position import (
    InputTablePosAlgorithm,
    TablePosAlgorithm,
)
from freeports_analysis.consts import Currency


class InputStandardCostCurr(BaseModel):
    deselection_list: Optional[List[InputPdfLineSet]] = []
    header_set: List[InputPdfLineSet]
    body_set: InputPdfLineSet
    subfund_set: InputPdfLineSet
    currency: Currency
    algorithm_flags: Optional[InputTablePosAlgorithm] = TablePosAlgorithm(0)
    tolerance: Optional[float] = 0.0
    row_algorithm_flags: Optional[InputTablePosAlgorithm] = TablePosAlgorithm(0)
    row_tolerance: Optional[float] = 0.0


def standard_cost_curr(arg: InputStandardCostCurr):
    return standard_pdf_filtering(
        deselection_list=[
            PdfLineSet.from_dict(il.model_dump()) for il in arg.deselection_list
        ],
        header_set=[PdfLineSet.from_dict(il.model_dump()) for il in arg.header_set],
        subfund_set=PdfLineSet.from_dict(arg.subfund_set.model_dump()),
        currency_set=arg.currency,
        body_set=PdfLineSet.from_dict(arg.body_set.model_dump()),
        algorithm_flags=arg.algorithm_flags,
        tolerance=arg.tolerance,
        row_algorithm_flags=arg.row_algorithm_flags,
        row_tolerance=arg.row_tolerance,
    )
