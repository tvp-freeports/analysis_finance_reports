from typing import Optional, List
from enum import Enum
from lxml import etree
from freeports_analysis.i18n import _


def _str_blocks(blk: "PdfBlock | TextBlock") -> str:
    """Basic function to format both PdfBlock and TextBlock
    for string rappresentation

    Parameters
    ----------
    blk : PdfBlock | TextBlock
        block to format

    Returns
    -------
    str
        formatted version
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


def _eq_blocks(a: "PdfBlock | TextBlock", b: "PdfBlock | TextBlock") -> bool:
    """Verifies if two TextBlocks or two PdfBlocks are equal

    Parameters
    ----------
    a : PdfBlock | TextBlock
        The first block
    b : PdfBlock | TextBlock
        The second block

    Returns
    -------
    bool
        True if equal, False otherwise
    """
    equal = True
    equal = equal and a.type_block == b.type_block
    equal = equal and a.metadata == b.metadata
    equal = equal and a.content == b.content
    return equal


class PdfBlock:
    """Represents a PDF content block with data to be extracted or relevant
    for subsequent filtering stages.

    Attributes
    ----------
    type_block : Enum
        The type of the PDF block.
    metadata : Optional[dict]
        Additional metadata associated with the block.
    content : Optional[str]
        The textual content extracted from the block.
    """

    type_block: Enum
    metadata: Optional[dict]
    content: Optional[str]

    def _text_form_element(self, ele: etree.Element) -> str:
        """Extracts text content from an XML element representing a PDF block.

        Args
        ----
        ele : etree.Element
            The XML element to extract text from.

        Returns
        -------
        str
            The extracted text content.
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
        """Compares two PdfBlock instances for equality.

        Parameters
        ----------
        other : PdfBlock
            The other PdfBlock to compare with.

        Returns
        -------
        bool
            True if the blocks are equal, False otherwise.
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

        Parameters
        ----------
        type_block : Enum
            The type of the PDF block.
        metadata : dict
            Additional metadata for the block.
        xml_ele : etree.Element | List[etree.Element]
            The XML element(s) containing the block's content.
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

        Returns
        -------
        str
            The string representation.
        """
        return _str_blocks(self)


class TextBlock:
    """Represents a processed text block derived from a PdfBlock.

    Attributes
    ----------
    type_block : Enum
        The type of the text block.
    metadata : dict
        Additional metadata associated with the block.
    content : str
        The textual content of the block.
    pdf_block : PdfBlock
        The original PdfBlock this text was derived from.
    """

    type_block: Enum
    metadata: dict
    content: str
    pdf_block: PdfBlock

    def __init__(self, type_block: Enum, metadata: dict, pdf_block: PdfBlock):
        """Initializes a TextBlock instance.

        Parameters
        ----------
        type_block : Enum
            The type of the text block.
        metadata : dict
            Additional metadata for the block.
        pdf_block : PdfBlock
            The source PdfBlock.
        """
        self.type_block = type_block
        self.metadata = metadata
        self.pdf_block = pdf_block
        self.content = pdf_block.content

    def __str__(self) -> str:
        """Returns a string representation of the TextBlock.

        Returns
        -------
        str
            The string representation.
        """
        return _str_blocks(self)

    def __eq__(self, other: "TextBlock") -> bool:
        """Compares two TextBlock instances for equality.

        Args
        ----
        other : TextBlock
            The other TextBlock to compare with.

        Returns
        -------
        bool
            True if the blocks are equal, False otherwise.
        """
        equal = _eq_blocks(self, other)
        equal = equal and self.pdf_block == other.pdf_block
        return equal


class ExpectedPdfBlockNotFound(Exception):
    """Raised when a required PdfBlock is not found"""


class ExpectedTextBlockNotFound(Exception):
    """Raised when a required TextBlock is not found"""


class PageParseFail(Exception):
    """Raised when the algorithm is unable to parse a page"""


class LineParseFail(Exception):
    """Raised when the algorithm is unable to parse a line"""


class ExtractionFieldFail(Exception):
    """Raised when the algorithm is unable to parse a field"""
