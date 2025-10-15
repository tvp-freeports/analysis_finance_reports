from lxml import etree

_bbox = etree.XPath(".//@bbox[1]")
_font_name = etree.XPath(".//font[1]/@name[1]")
_font_size = etree.XPath(".//font[1]/@size[1]")
_text = etree.XPath(".//@text[1]")


def bbox(x):
    return _bbox(x)[0]


def font_name(x):
    return _font_name(x)[0]


def font_size(x):
    return _font_size(x)[0]


def text(x):
    return _text(x)[0]


lines = etree.XPath(".//line")
