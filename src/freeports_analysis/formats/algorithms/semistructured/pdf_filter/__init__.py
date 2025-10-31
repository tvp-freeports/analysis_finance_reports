"""PDF filtering algorithms for semi-structured document processing.

This module provides PDF filtering functions specifically designed for
semi-structured documents, including cost and currency extraction.
"""

from typing import List, Optional, Callable, Any
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
    """Input parameters for standard cost and currency PDF filtering.

    This class defines the configuration for filtering PDF content to extract
    cost and currency information from semi-structured documents.

    Attributes
    ----------
    deselection_list : Optional[List[InputPdfLineSet]], optional
        List of line sets to exclude from processing, by default []
    header_set : List[InputPdfLineSet]
        Line sets representing document headers
    body_set : InputPdfLineSet
        Line set representing the main body content
    subfund_set : InputPdfLineSet
        Line set representing subfund information
    currency : Currency
        Currency type to filter for
    algorithm_flags : Optional[InputTablePosAlgorithm], optional
        Algorithm flags for table position detection, by default TablePosAlgorithm(0)
    tolerance : Optional[float], optional
        Tolerance value for position matching, by default 0.0
    row_algorithm_flags : Optional[InputTablePosAlgorithm], optional
        Algorithm flags for row position detection, by default TablePosAlgorithm(0)
    row_tolerance : Optional[float], optional
        Tolerance value for row position matching, by default 0.0
    """

    deselection_list: Optional[List[InputPdfLineSet]] = []
    header_set: List[InputPdfLineSet]
    body_set: InputPdfLineSet
    subfund_set: InputPdfLineSet
    currency: Currency
    algorithm_flags: Optional[InputTablePosAlgorithm] = TablePosAlgorithm(0)
    tolerance: Optional[float] = 0.0
    row_algorithm_flags: Optional[InputTablePosAlgorithm] = TablePosAlgorithm(0)
    row_tolerance: Optional[float] = 0.0


def standard_cost_curr(arg: InputStandardCostCurr) -> Callable[[Any], Any]:
    """Apply standard PDF filtering for cost and currency extraction.

    This function processes PDF content to extract cost and currency information
    using configured line sets and filtering parameters.

    Parameters
    ----------
    arg : InputStandardCostCurr
        Configuration parameters for the filtering operation

    Returns
    -------
    Callable[[Any], Any]
        A filtering function that can be applied to XML content

    Notes
    -----
    The function converts input line sets to internal representations and
    applies the standard PDF filtering algorithm with the specified parameters.
    """
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
