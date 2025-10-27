"""XPath query utilities for PDF XML processing.

This module provides pre-compiled XPath queries for efficiently extracting
PDF metadata from XML elements, including bounding boxes, font information,
and text content.
"""

from lxml import etree

# Pre-compiled XPath queries for performance
_bbox = etree.XPath(".//@bbox[1]")
_font_name = etree.XPath(".//font[1]/@name[1]")
_font_size = etree.XPath(".//font[1]/@size[1]")
_text = etree.XPath(".//@text[1]")


def bbox(x: etree.Element) -> str:
    """Extract bounding box coordinates from XML element.

    Parameters
    ----------
    x : etree.Element
        XML element containing PDF data

    Returns
    -------
    str
        Bounding box coordinates as space-separated string
    """
    return _bbox(x)[0]


def font_name(x: etree.Element) -> str:
    """Extract font name from XML element.

    Parameters
    ----------
    x : etree.Element
        XML element containing PDF data

    Returns
    -------
    str
        Font name used in the element
    """
    return _font_name(x)[0]


def font_size(x: etree.Element) -> str:
    """Extract font size from XML element.

    Parameters
    ----------
    x : etree.Element
        XML element containing PDF data

    Returns
    -------
    str
        Font size as string
    """
    return _font_size(x)[0]


def text(x: etree.Element) -> str:
    """Extract text content from XML element.

    Parameters
    ----------
    x : etree.Element
        XML element containing PDF data

    Returns
    -------
    str
        Text content of the element
    """
    return _text(x)[0]


# XPath query for extracting all line elements
lines = etree.XPath(".//line")
