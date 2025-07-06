"""Module common to each format, it contains the definitions used by all the formats"""

from enum import Enum
from typing import Optional, List, Callable
import logging as log
from lxml import etree
from freeports_analysis.consts import FinancialData

logger = log.getLogger(__name__)


def _str_blocks(blk) -> str:
    text = f"{blk.__class__.__name__}:  ({blk.type_block.name} type)\n"
    text += f"\tmetadata {blk.metadata}\n"
    text_no_last_nl = blk.content
    if len(blk.content) > 0:
        if blk.content[-1] == "\n":
            text_no_last_nl = text_no_last_nl[:-1]
    text += f'\t"{text_no_last_nl}"'
    return text


def _eq_blocks(a, b) -> bool:
    equal = True
    equal = equal and a.type_block == b.type_block
    equal = equal and a.metadata == b.metadata
    equal = equal and a.content == b.content
    return equal


class PdfBlock:
    """Represents a PDF content block with data to be extracted or relevant
    for subsequent filtering stages.

    Attributes:
        type_block (Enum): The type of the PDF block.
        metadata (Optional[dict]): Additional metadata associated with the block.
        content (Optional[str]): The textual content extracted from the block.
    """

    type_block: Enum
    metadata: Optional[dict]
    content: Optional[str]

    def _text_form_element(self, ele: etree.Element) -> str:
        """Extracts text content from an XML element representing a PDF block.

        Args:
            ele (etree.Element): The XML element to extract text from.
        Returns:
            str: The extracted text content.
        """
        text = ""
        if ele.tag == "line":
            lines = [ele]
        else:
            lines = ele.findall("line")
        for line in lines:
            for e in line.findall(".//char"):
                c = e.get("c")
                if c is not None:
                    text += c
            text += "\n"
        return text

    def __eq__(self, other):
        """Compares two PdfBlock instances for equality.

        Args:
            other (PdfBlock): The other PdfBlock to compare with.
        Returns:
            bool: True if the blocks are equal, False otherwise.
        """
        equal = _eq_blocks(self, other)
        return equal

    def __init__(
        self,
        type_block: Enum,
        metadata: dict,
        xml_ele: etree.Element | List[etree.Element],
    ):
        """Initializes a PdfBlock instance.

        Args:
            type_block (Enum): The type of the PDF block.
            metadata (dict): Additional metadata for the block.
            xml_ele (etree.Element): The XML element containing the block's content.
        """
        self.type_block = type_block
        self.metadata = metadata
        txt = ""
        if isinstance(xml_ele, list):
            for ele in xml_ele:
                txt += self._text_form_element(ele)
        else:
            txt = self._text_form_element(xml_ele)
        self.content = txt

    def __str__(self) -> str:
        """Returns a string representation of the PdfBlock.

        Returns:
            str: The string representation.
        """
        return _str_blocks(self)


class TextBlock:
    """Represents a processed text block derived from a PdfBlock.

    Attributes:
        type_block (Enum): The type of the text block.
        metadata (dict): Additional metadata associated with the block.
        content (str): The textual content of the block.
        pdf_block (PdfBlock): The original PdfBlock this text was derived from.
    """

    type_block: Enum
    metadata: dict
    content: str
    pdf_block: PdfBlock

    def __init__(self, type_block: Enum, metadata: dict, pdf_block: PdfBlock):
        """Initializes a TextBlock instance.

        Args:
            type_block (Enum): The type of the text block.
            metadata (dict): Additional metadata for the block.
            pdf_block (PdfBlock): The source PdfBlock.
        """
        self.type_block = type_block
        self.metadata = metadata
        self.pdf_block = pdf_block
        self.content = pdf_block.content

    def __str__(self) -> str:
        """Returns a string representation of the TextBlock.

        Returns:
            str: The string representation.
        """
        return _str_blocks(self)

    def __eq__(self, other):
        """Compares two TextBlock instances for equality.

        Args:
            other (TextBlock): The other TextBlock to compare with.

        Returns:
            bool: True if the blocks are equal, False otherwise.
        """
        equal = _eq_blocks(self, other)
        equal = equal and self.pdf_block == other.pdf_block
        return equal


def pdf_filter_exec(
    batch_pages: List[str], i_batch_page: int, n_pages: int, pdf_filter_func
) -> List[PdfBlock]:
    """Processes a PDF document through a filter function to extract relevant blocks.

    Args:
        document: The PDF document to process.
        pdf_filter_func: A function that takes an XML element and returns a list of
                        relevant PdfBlock.

    Returns:
        List of PdfBlock objects containing the filtered content.
    """

    batch_results = []
    for page_number, page in enumerate(batch_pages, start=i_batch_page + 1):
        if (page_number + i_batch_page) % (n_pages // min(10, n_pages)) == 0:
            logger.debug("Filtering page %i", page_number)
        result = pdf_filter_func(page, page_number)
        batch_results.extend(result)
    return batch_results


def text_extract_exec(
    pdf_blocks: List[PdfBlock],
    targets: List[str],
    text_extract_func: Callable[[List[PdfBlock], List[str]], List[TextBlock]],
) -> List[TextBlock]:
    """Extracts text content from PDF blocks using a specified extraction function.

    Args:
        pdf_blocks: List of PdfBlock objects to process.
        targets: List of target identifiers for extraction.
        text_extract_func: Function that processes PdfBlocks and targets into TextBlocks.

    Returns:
        List of TextBlock objects containing the extracted content.
    """
    return text_extract_func(pdf_blocks, targets)


def deserialize_exec(
    text_blocks: List[TextBlock],
    targets: List[str],
    deserialize_func: Callable[[TextBlock, List[str]], FinancialData],
) -> List[FinancialData]:
    """Converts TextBlocks into tabular data using a specified formatting function.

    Args:
        TextBlocks: List of TextBlock objects to process.
        deserialize_func: Function that converts a TextBlock into a finantial data class.

    Returns:
        List of FinantialData classes containing the deserialized data.
    """
    return [deserialize_func(txtblk, targets) for txtblk in text_blocks]


class ExpectedPdfBlockNotFound(Exception):
    """Raised when a required PdfBlock is not found"""


class ExpectedTextBlockNotFound(Exception):
    """Raised when a required TextBlock is not found"""
