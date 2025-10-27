"""Core data structures and exceptions for PDF document processing.

This module defines the fundamental data structures (PdfBlock, TextBlock) and
exception classes used throughout the document processing pipeline.
"""

from typing import Optional, List, Union
from enum import Enum
from lxml import etree
from freeports_analysis.i18n import _


def _str_blocks(blk: Union["PdfBlock", "TextBlock"]) -> str:
    """Format PdfBlock or TextBlock for string representation.

    Parameters
    ----------
    blk : Union[PdfBlock, TextBlock]
        Block to format

    Returns
    -------
    str
        Formatted string representation
    """
    type_translated = _("({} type)").format(blk.type_block.name)
    metadata_translated = _("metadata")
    text = f"{blk.__class__.__name__}:  {type_translated}\n"
    text += f"\t{metadata_translated} {blk.metadata}\n"
    text_no_last_nl = blk.content
    if len(blk.content) > 0:
        if blk.content[-1] == "\n":
            text_no_last_nl = text_no_last_nl[:-1]
    text += f'\t"{text_no_last_nl}"'
    return text


def _eq_blocks(
    a: Union["PdfBlock", "TextBlock"], b: Union["PdfBlock", "TextBlock"]
) -> bool:
    """Compare two TextBlocks or PdfBlocks for equality.

    Parameters
    ----------
    a : Union[PdfBlock, TextBlock]
        First block to compare
    b : Union[PdfBlock, TextBlock]
        Second block to compare

    Returns
    -------
    bool
        True if blocks are equal, False otherwise
    """
    equal = True
    equal = equal and a.type_block == b.type_block
    equal = equal and a.metadata == b.metadata
    equal = equal and a.content == b.content
    return equal


class PdfBlock:
    """Represents a PDF content block with data to be extracted or relevant for filtering.

    Attributes
    ----------
    type_block : Enum
        The type of the PDF block
    metadata : Optional[dict]
        Additional metadata associated with the block
    content : Optional[str]
        The textual content extracted from the block
    """

    type_block: Enum
    metadata: Optional[dict]
    content: Optional[str]

    def _text_form_element(self, ele: etree.Element) -> str:
        """Extract text content from an XML element representing a PDF block.

        Parameters
        ----------
        ele : etree.Element
            XML element to extract text from

        Returns
        -------
        str
            Extracted text content
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

    def __eq__(self, other: "PdfBlock") -> bool:
        """Compare two PdfBlock instances for equality.

        Parameters
        ----------
        other : PdfBlock
            Other PdfBlock to compare with

        Returns
        -------
        bool
            True if blocks are equal, False otherwise
        """
        return _eq_blocks(self, other)

    def __init__(
        self,
        type_block: Enum,
        metadata: dict,
        xml_ele: Union[etree.Element, List[etree.Element]],
    ):
        """Initialize a PdfBlock instance.

        Parameters
        ----------
        type_block : Enum
            Type of the PDF block
        metadata : dict
            Additional metadata for the block
        xml_ele : Union[etree.Element, List[etree.Element]]
            XML element(s) containing the block's content
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
        """Return string representation of the PdfBlock.

        Returns
        -------
        str
            String representation
        """
        return _str_blocks(self)


class TextBlock:
    """Represents a processed text block derived from a PdfBlock.

    Attributes
    ----------
    type_block : Enum
        Type of the text block
    metadata : dict
        Additional metadata associated with the block
    content : str
        Textual content of the block
    pdf_block : PdfBlock
        Original PdfBlock this text was derived from
    """

    type_block: Enum
    metadata: dict
    content: str
    pdf_block: PdfBlock

    def __init__(self, type_block: Enum, metadata: dict, pdf_block: PdfBlock):
        """Initialize a TextBlock instance.

        Parameters
        ----------
        type_block : Enum
            Type of the text block
        metadata : dict
            Additional metadata for the block
        pdf_block : PdfBlock
            Source PdfBlock
        """
        self.type_block = type_block
        self.metadata = metadata
        self.pdf_block = pdf_block
        self.content = pdf_block.content

    def __str__(self) -> str:
        """Return string representation of the TextBlock.

        Returns
        -------
        str
            String representation
        """
        return _str_blocks(self)

    def __eq__(self, other: "TextBlock") -> bool:
        """Compare two TextBlock instances for equality.

        Parameters
        ----------
        other : TextBlock
            Other TextBlock to compare with

        Returns
        -------
        bool
            True if blocks are equal, False otherwise
        """
        equal = _eq_blocks(self, other)
        equal = equal and self.pdf_block == other.pdf_block
        return equal


class ExpectedPdfBlockNotFound(Exception):
    """Raised when a required PdfBlock is not found during processing."""


class ExpectedTextBlockNotFound(Exception):
    """Raised when a required TextBlock is not found during processing."""


class PageParseFail(Exception):
    """Raised when the algorithm is unable to parse a page."""


class LineParseFail(Exception):
    """Raised when the algorithm is unable to parse a line."""


class ExtractionFieldFail(Exception):
    """Raised when the algorithm is unable to parse a field."""
