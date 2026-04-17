"""MEDIOLANUM_ES24_B format submodule.

This module provides processing functions for the MEDIOLANUM_ES24_B format,
which handles Spanish financial documents with specific layout characteristics.
"""

from typing import List, Optional, Any
from enum import auto, Enum
from lxml import etree
from freeports_analysis.formats.algorithms.commons import Pipeline
from freeports_analysis.formats.utils.pdf_extract import (
    PdfExtractInvestmentsStandard,
    PdfExtractPageClassifyStandard,
    PdfExtractFundStandard,
    PdfExtractCurrencyConstant,
    ResultStandardExtraction,
)
from freeports_analysis.formats.utils.pdf_extract.pdf_parts import (
    PdfLineSelection,
    pdflines_from_pagedict,
)
from freeports_analysis.formats.utils.text_filter import (
    TextFilterInvestmentsStandard,
    ResultStandardFiltering,
)
from freeports_analysis.formats.utils.deserialize import (
    DeserializerInvestmentStandard,
    deserialize_block_types,
)
from freeports_analysis.consts import (
    Promise,
    Currency,
    PromisesResolutionContext,
)
from freeports_analysis.output import Fund
from .. import PdfBlock, TextBlock


class PdfBlockType(Enum):
    """Types of PDF blocks for MEDIOLANUM_ES24_B format."""

    RELEVANT_BLOCK = auto()


class TextBlockType(Enum):
    """Types of text blocks for MEDIOLANUM_ES24_B format."""

    BOND_TARGET = auto()
    EQUITY_TARGET = auto()


class FirstPageTextBlockType(Enum):
    """Types of text blocks for MEDIOLANUM_ES24_B format."""

    SUBFUND = auto()


def pdf_extract_first_page(dict_root) -> List[PdfBlock]:
    lines = pdflines_from_pagedict(dict_root)
    sl = PdfLineSelection.area(0, 88, 1e6, 102).select(lines)[0]
    subfund = sl.text.strip().upper()
    return [PdfBlock(PdfBlockType.RELEVANT_BLOCK, {"subfund": subfund}, sl.text)]


def text_filter_first_page(
    pdf_blocks: List[PdfBlock], targets: List[str]
) -> List[TextBlock]:
    if len(pdf_blocks) == 1 and pdf_blocks[0].type_block == PdfBlockType.RELEVANT_BLOCK:
        return [
            TextBlock(
                FirstPageTextBlockType.SUBFUND,
                {"subfund": pdf_blocks[0].metadata["subfund"]},
                pdf_blocks[0],
            )
        ]


def deserialize_first_page(txt_blk: Optional[TextBlock]) -> Optional[Any]:
    return {"title document": txt_blk.metadata["subfund"]}


@deserialize_block_types(
    ResultStandardFiltering.BOND_TARGET,
    ResultStandardFiltering.EQUITY_TARGET,
    ResultStandardFiltering.FUND,
)
def deserialize(txt_blk: Optional[TextBlock]) -> Optional[Any]:
    """Deserialize text blocks into structured data for MEDIOLANUM_ES24_B format.

    Parameters
    ----------
    txt_blk : Optional[TextBlock]
        Text block to deserialize, or None

    Returns
    -------
    Optional[Any]
        Deserialized data object or None if input is None

    Notes
    -----
    Handles subfund context resolution and applies specific scaling
    to market values (multiplies by 1000).
    """
    if txt_blk.type_block == ResultStandardFiltering.FUND:
        return None
    std = DeserializerInvestmentStandard()
    blk = std(txt_blk)
    if blk is not None:
        blk.market_value = blk.market_value * 1000
    return blk


pipelines = {
    "investments": Pipeline(
        pdf_extract=(
            PdfExtractInvestmentsStandard(
                body_set=PdfLineSelection.font("Helvetica"),
                deselection_list=[
                    PdfLineSelection.text("Cartera de inversiones financieras a"),
                    PdfLineSelection(text="-$", font="Helvetica"),
                ],
            ),
            PdfExtractCurrencyConstant(Currency.EUR),
            lambda _: [
                PdfBlock(
                    ResultStandardExtraction.FUND_NAME, {}, Promise("title document")
                )
            ],
        ),
        deserialize=deserialize,
    ),
    "subfund": Pipeline(
        pdf_extract=pdf_extract_first_page,
        text_filter=text_filter_first_page,
        deserialize=(
            deserialize_first_page,
            lambda txt_blk: Fund(name=txt_blk.metadata["subfund"]),
        ),
    ),
}
