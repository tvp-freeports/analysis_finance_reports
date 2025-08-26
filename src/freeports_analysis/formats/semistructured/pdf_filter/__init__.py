from typing import List, Optional
from pydantic import BaseModel
from freeports_analysis.formats_utils.pdf_filter.pdf_parts import PdfLineSetDict
from freeports_analysis.formats_utils.pdf_filter import standard_pdf_filtering
from freeports_analysis.formats_utils.pdf_filter.select_position import (
    InputTablePosAlgorithm,
)
from freeports_analysis.consts import Currency


class InputStandardCostCurr(BaseModel):
    deselection_list: Optional[List[PdfLineSetDict]] = []
    header_set: List[PdfLineSetDict]
    body_set: PdfLineSetDict
    subfund_set: PdfLineSetDict
    currency: Currency
    algorithm_flags: Optional[InputTablePosAlgorithm] = None
    tolerance: Optional[float] = 0.0


def standard_cost_curr(arg: InputStandardCostCurr):
    return standard_pdf_filtering(
        deselection_list=arg.deselection_list,
        header_set=arg.header_set,
        subfund_set=arg.subfund_set,
        currency_set=arg.currency,
        body_set=arg.body_set,
        algorithm_flags=arg.algorithm_flags,
        tolerance=arg.tolerance,
    )
