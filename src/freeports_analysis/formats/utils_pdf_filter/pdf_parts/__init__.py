from lxml import etree
from .font import Font, TextSize
from ..xml.position import get_bounds
from .position import Area, XRange, YRange


class ExtractedPdfLine:
    def __init__(self, blk: etree.Element):
        self._blk = blk
        bounds = get_bounds(blk)
        self._geometry = Area(
            XRange(bounds[0][0], bounds[0][1]), YRange(bounds[1][0], bounds[1][1])
        )
        self._font = Font(blk.xpath(".//font/@name")[0])
        self._txt_size = TextSize(blk.xpath(".//font/@size")[0])

    @property
    def geometry(self):
        return self._geometry

    @property
    def c(self):
        return self._geometry.c

    @property
    def corners(self):
        return self._geometry.corners

    @property
    def font(self):
        return self._font

    @property
    def text_size(self):
        return self._txt_size

    @property
    def xml_blk(self):
        return self._blk

    def __str__(self):
        string = f"Line PDF - Font '{self.font}' [{self.text_size}]\n"
        (((x_tl, y_tl), (x_tr, y_tr)), ((x_bl, y_bl), (x_br, y_br))) = self.corners
        x, y = self.c
        string += f"\t({x_tl:.3f}, {y_tl:.3f})\t({x_tr:.3f}, {y_tr:.3f})\n"
        string += f"\t\t({x:.3f}, {y:.3f})\n"
        string += f"\t({x_bl:.3f}, {y_bl:.3f})\t({x_br:.3f}, {y_br:.3f})\n"
        return string
