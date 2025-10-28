"""PDF XML parts in a friendly format (custom Python classes).

This module provides a high-level interface for working with PDF document elements
by wrapping raw XML structures into Python objects with intuitive properties and
methods. The main classes include:

- `PdfLine`: Base class representing a PDF line with font, size, and area properties
- `ExtractedPdfLine`: Concrete implementation that extracts data from XML elements
- `PdfLineSet`: Complex set operations for filtering PDF lines based on multiple criteria

These classes enable sophisticated PDF document analysis by providing:
- Geometric operations (area containment, position filtering)
- Typographic filtering (font, size matching)
- Text content matching with regex support
- Set operations (union, intersection, difference)
- Contextualization based on actual PDF content

Examples
--------
>>> # Create a PDF line set filtering for specific font and area
>>> line_set = PdfLineSet(font="Arial", area=((0, 100), (0, 200)))
>>> # Check if a line matches the criteria
>>> line in line_set
True

Notes
-----
The module uses Shapely for geometric operations and lxml for XML processing.
All coordinates are in PDF points (1/72 inch).
"""

from typing import Optional, Tuple, Annotated, Callable, Any
import re
import ast
from operator import or_, and_, sub, truediv
from functools import reduce
from lxml import etree
from pydantic import BaseModel, AfterValidator, PositiveFloat
from shapely import Polygon, box
from portion.interval import Interval
from freeports_analysis.i18n import _
from .font import Font, FontSize, FontSizeSet, FontSet, TextSet, AllFonts
from ..xml.position import get_bounds
from .position import InputArea
from ..xml import xpath_queries as xpath


class PdfLine:
    """A class representing a PDF line with geometric and typographic properties.

    This class provides a friendly interface to access geometric properties,
    font information, and text size of a line in a PDF document. It serves as
    the base class for PDF line representations and provides common functionality
    for all PDF line types.

    Parameters
    ----------
    text : Optional[str]
        The text content of the line. If None, indicates the line has no text content.
    font : Optional[Font]
        The font used in the line. If None, font information is not available.
    font_size : Optional[FontSize]
        The text size of the line in points. If None, size information is not available.
    area : Optional[Polygon]
        The geometric area (bounding box) in which the line is contained.
        If None, geometric information is not available.

    Attributes
    ----------
    area : Polygon
        Read-only property returning the line's bounding box area
    font : Font
        Read-only property returning the line's font
    font_size : FontSize
        Read-only property returning the line's font size
    text : str
        Read-only property returning the line's text content

    Notes
    -----
    - All coordinates are in PDF points (1/72 inch)
    - The area is represented as a Shapely Polygon for geometric operations
    - Font and font_size are specialized types that support set operations

    Examples
    --------
    >>> line = PdfLine(text="Hello World", font="Arial", font_size=12.0)
    >>> print(line.font)
    Arial
    >>> print(line.area)
    POLYGON ((0 0, 100 0, 100 12, 0 12, 0 0))
    """

    def __init__(
        self,
        font: Optional[Font] = None,
        font_size: Optional[FontSize] = None,
        area: Optional[Polygon] = None,
        text: Optional[str] = None,
    ):
        """Initialize the ExtractedPdfLine from an XML element.

        Parameters
        ----------
        blk : etree.Element
            The XML element containing the line data.
        """
        self._text = text
        self._font = font
        self._font_size = font_size
        self._area = area

    @property
    def area(self) -> Polygon:
        """Get the geometric properties of the line.

        Returns
        -------
        Polygon
            The area representing the line's bounds.
        """
        return self._area

    @property
    def font(self) -> Font:
        """Get the font used in the line.

        Returns
        -------
        Font
            The font used in the line.
        """
        return self._font

    @property
    def font_size(self) -> FontSize:
        """Get the text size used in the line.

        Returns
        -------
        FontSize
            The text size used in the line.
        """
        return self._font_size

    @property
    def text(self) -> str:
        """Get the text used in the line.

        Returns
        -------
        str
            The text used in the line.
        """
        return self._text

    def _fmt_point(self, coor):
        return "" if coor is None else f"{coor:.3f}"

    def __str__(self) -> str:
        """Return a formatted string representation of the line.

        Returns
        -------
        str
            Formatted string showing font, text size, and coordinates.
        """
        string = f"{self.__class__.__name__}:\n"
        string += f"\t'{self.font}' [{self.font_size}]\n"
        if self.text is not None:
            string += f'\t"{self.text}"\n'
        if self.area is not None:
            string += f"\t{self.area}\n"

        return string


class ExtractedPdfLine(PdfLine):
    """Concrete PDF line implementation that extracts data from XML elements.

    This class extends PdfLine by automatically extracting line properties
    from XML elements representing PDF content. It provides the bridge between
    raw PDF XML data and the high-level PdfLine interface.

    Parameters
    ----------
    blk : etree.Element
        The XML element containing PDF line data. Expected to have:
        - A 'bbox' attribute with bounding box coordinates
        - Font information in child elements
        - Text content in attributes

    Attributes
    ----------
    xml_blk : etree.Element
        Read-only property returning the original XML element

    Raises
    ------
    ValueError
        If the XML element does not contain required attributes

    Notes
    -----
    - The XML element is expected to follow PDF XML structure conventions
    - Bounding box coordinates are parsed from the 'bbox' attribute
    - Font information is extracted from child <font> elements
    - Text content is extracted from the 'text' attribute

    Examples
    --------
    >>> # Assuming xml_element is a valid PDF XML line element
    >>> line = ExtractedPdfLine(xml_element)
    >>> print(f"Text: {line.text}, Font: {line.font}, Size: {line.font_size}")
    Text: Sample Text, Font: Arial, Size: 12.0
    """

    def __init__(self, blk: etree.Element):
        """Initialize the ExtractedPdfLine from an XML element.

        Parameters
        ----------
        blk : etree.Element
            The XML element containing the line data. Must have:
            - 'bbox' attribute with format "x0 y0 x1 y1"
            - Font information accessible via xpath queries
            - Text content in 'text' attribute

        Notes
        -----
        The initialization process:
        1. Extracts bounding box coordinates and creates Polygon
        2. Extracts font name and creates Font object
        3. Extracts font size and creates FontSize object
        4. Extracts text content
        5. Stores reference to original XML element
        """
        bounds = get_bounds(blk)
        super().__init__(
            text=xpath.text(blk),
            font=Font(xpath.font_name(blk)),
            font_size=FontSize(xpath.font_size(blk)),
            area=box(bounds[0][0], bounds[1][0], bounds[0][1], bounds[1][1]),
        )
        self._blk = blk

    @property
    def xml_blk(self) -> etree.Element:
        """Get the original XML element containing the line data.

        Returns
        -------
        etree.Element
            The original XML element containing the line data.

        Notes
        -----
        This property provides access to the raw XML data for advanced
        operations that require direct XML manipulation.
        """
        return self._blk


class InputPdfLineSet(BaseModel):
    text: Optional[str] = None
    font: Optional[str] = None
    font_size: Optional[PositiveFloat] = None
    area: Optional[InputArea] = None


_LINE_SET_FONT_REGEXP = r"(?P<font>[\w\-, ]+)"
_NUMBER_REGEXP = r"[0-9]+(\.[0-9]+)?"
_LINE_SET_FONTSIZE_REGEXP = rf"\[(?P<font_size>{_NUMBER_REGEXP})\]"
_RANGE_REGEXP = rf"\(({_NUMBER_REGEXP})?:({_NUMBER_REGEXP})?\)"
_LINE_SET_AREA_REGEXP = (
    rf"(?P<y_range>{_RANGE_REGEXP})|\((?P<area>{_RANGE_REGEXP}{_RANGE_REGEXP})\)"
)
_LINE_SET_TEXT_REGEXP = '"(?P<text>.*)"'
LINE_SET_REGEXP = f"({_LINE_SET_FONT_REGEXP})? ?"
LINE_SET_REGEXP += f"({_LINE_SET_FONTSIZE_REGEXP})? ?"
LINE_SET_REGEXP += f"({_LINE_SET_AREA_REGEXP})? ?"
LINE_SET_REGEXP += f"({_LINE_SET_TEXT_REGEXP})?"
_LINE_SET_REGEXP = re.compile(LINE_SET_REGEXP)


def _op_over_none(op: Callable, v1: Any, v2: Any) -> Any:
    """Apply operation to values, handling None cases gracefully.

    Parameters
    ----------
    op : Callable
        Binary operation to apply
    v1 : Any
        First operand
    v2 : Any
        Second operand

    Returns
    -------
    Any
        Result of operation, or the non-None value if only one is provided,
        or None if both are None
    """
    if v1 is not None and v2 is not None:
        return op(v1, v2)
    if v1 is not None:
        return v1
    if v2 is not None:
        return v2
    return None


class _FlattenPdfLineSet:
    def __init__(
        self,
        font: Optional[FontSet] = None,
        font_size: Optional[FontSizeSet] = None,
        area: Optional[Polygon | Tuple[float, float]] = None,
        text: Optional[TextSet] = None,
    ):
        self._font = font
        self._font_size = font_size
        self._area = area
        self._text = text

    @property
    def font(self) -> FontSet:
        """Get the font used in the line.

        Returns
        -------
        Font
            The font used in the line.
        """
        return self._font

    @property
    def font_size(self) -> FontSizeSet:
        """Get the text size used in the line.

        Returns
        -------
        FontSize
            The text size used in the line.
        """
        return self._font_size

    @property
    def text(self) -> TextSet:
        """Get the text used in the line.

        Returns
        -------
        str
            The text used in the line.
        """
        return self._text

    @property
    def area(self) -> Polygon:
        return self._area

    def __contains__(self, other: ExtractedPdfLine):
        if self.font is not None:
            if other.font not in self.font:
                return False
        if self.font_size is not None:
            if other.font_size not in self.font_size:
                return False
        if self.area is not None:
            if not self.area.contains(other.area):
                return False
        if self.text is not None:
            if other.text not in self.text:
                return False
        return True

    def __repr__(self):
        string = f"{self.__class__.__name__}:\n"
        font = "{}"
        if self.font is not None:
            font = (
                f"{set(self.font)}" if isinstance(self.font, FontSet) else "<font_ref>"
            )
        fs = "[]"
        if self.font_size is not None:
            fs = (
                f"{Interval(self.font_size)}"
                if isinstance(self.font_size, FontSizeSet)
                else "<font_size_ref>"
            )
        area = f"{self.area}" if isinstance(self.area, Polygon) else "<area_ref>"
        text = f"{self.text}" if isinstance(self.text, TextSet) else "<text_ref>"
        string += f"\t{font} {fs}\n"
        if self.text is not None:
            string += f"\t{text}\n"
        if self.area is not None:
            string += f"\t{area}\n"
        return string

    def contextualize(self, xml_root):
        concrete = _FlattenPdfLineSet(
            font=self.font, font_size=self.font_size, area=self.area, text=self.text
        )
        lines = [ExtractedPdfLine(el) for el in xml_root.findall(".//line")]

        def _contextualize(t, value, aggregators, lines, xml_root):
            if value is None:
                return None
            handled = isinstance(value, t)
            if handled:
                return value
            for condition, agg_func in aggregators:
                if isinstance(condition, type):
                    handled = isinstance(value, condition)
                else:
                    handled = condition(value)
                if handled:
                    return agg_func(value, lines, xml_root)
            raise ValueError(
                _("Not possible aggregate to {} from {}:\n{}").format(
                    t, type(value), value
                )
            )

        font_aggregators = _font_aggregators
        font_size_aggregators = _font_size_aggregators
        text_aggregators = _text_aggregators
        area_aggregators = _area_aggregators
        concrete._font = _contextualize(
            FontSet, concrete.font, font_aggregators, lines, xml_root
        )
        concrete._font_size = _contextualize(
            FontSizeSet, concrete.font_size, font_size_aggregators, lines, xml_root
        )
        concrete._text = _contextualize(
            TextSet, concrete.text, text_aggregators, lines, xml_root
        )
        concrete._area = _contextualize(
            Polygon, concrete.area, area_aggregators, lines, xml_root
        )
        return concrete

    @property
    def is_concrete(self):
        if self._font is not None and not isinstance(self._font, FontSet):
            return False
        if self._font_size is not None and not isinstance(self._font_size, FontSizeSet):
            return False
        if self._text is not None and not isinstance(self._text, TextSet):
            return False
        if self._area is not None and not isinstance(self._area, Polygon):
            return False
        return True


class PdfLineSet:
    """A set-like container for filtering PDF lines based on multiple criteria.

    This class provides sophisticated filtering capabilities for PDF lines using
    geometric, typographic, and textual criteria. It supports complex set operations
    (union, intersection, difference) and can be contextualized based on actual
    PDF content.

    Parameters
    ----------
    font : Optional[FontSet], optional
        Font criteria for filtering. Can be a single font name or FontSet.
        If None, no font filtering is applied.
    font_size : Optional[FontSizeSet], optional
        Font size criteria for filtering. Can be a single size or size range.
        If None, no size filtering is applied.
    area : Optional[Polygon | Tuple[float, float]], optional
        Geometric area criteria. Can be:
        - Polygon: Exact geometric area
        - Tuple[Tuple[float, float], Tuple[float, float]]: ((xmin, xmax), (ymin, ymax))
        - Tuple[float, float]: (ymin, ymax) with x-range unbounded
        If None, no area filtering is applied.
    text : Optional[TextSet], optional
        Text content criteria. Can be exact text, regex patterns, or TextSet.
        If None, no text filtering is applied.

    Attributes
    ----------
    is_simple : bool
        True if the set represents a simple (non-compound) filter
    is_concrete : bool
        True if all criteria are concrete (not references to other sets)
    one_d : bool
        True if the set filters on exactly one dimension

    Notes
    -----
    - The class uses a binary tree structure internally to represent complex filters
    - Set operations (|, &, /) create new compound sets
    - Contextualization resolves references based on actual PDF content
    - The class supports both simple and complex filtering scenarios

    Examples
    --------
    >>> # Simple filter for Arial font in specific area
    >>> line_set = PdfLineSet(font="Arial", area=((0, 100), (0, 200)))
    >>>
    >>> # Complex filter using set operations
    >>> header_set = PdfLineSet(font="Arial-Bold", font_size=14)
    >>> body_set = PdfLineSet(font="Arial", font_size=10)
    >>> combined_set = header_set | body_set
    >>>
    >>> # Check if a line matches the criteria
    >>> line in combined_set
    True
    """

    def __init__(
        self,
        font: Optional[FontSet] = None,
        font_size: Optional[FontSizeSet] = None,
        area: Optional[Polygon | Tuple[float, float]] = None,
        text: Optional[TextSet] = None,
    ):
        # Convert area tuples to proper Polygon objects
        if isinstance(area, tuple):
            if isinstance(area[0], tuple):
                # Full bounding box: ((xmin, xmax), (ymin, ymax))
                ((xmin, xmax), (ymin, ymax)) = area
                if xmin is None:
                    xmin = -1e6
                if ymin is None:
                    ymin = -1e6
                if xmax is None:
                    xmax = +1e6
                if ymax is None:
                    ymax = +1e6
                area = box(xmin, ymin, xmax, ymax)
            elif isinstance(area[0], (float, int)) or area[0] is None:
                # Vertical range only: (ymin, ymax)
                ymin, ymax = area
                if ymin is None:
                    ymin = -1e6
                if ymax is None:
                    ymax = +1e6
                area = box(-1e6, ymin, 1e6, ymax)

        # Convert string inputs to proper typed objects
        if isinstance(font, str):
            font = FontSet(font)
        if isinstance(font_size, (float, int)):
            font_size = FontSizeSet.from_range(font_size - 1e-4, font_size + 1e-4)
        if isinstance(text, str):
            text = TextSet(text)

        # Initialize the internal representation
        self._left = _FlattenPdfLineSet(
            font=font, font_size=font_size, area=area, text=text
        )
        self._right = None

    @property
    def is_simple(self) -> bool:
        """Check if this is a simple (non-compound) PDF line set.

        Returns
        -------
        bool
            True if the set represents a simple filter with no compound operations,
            False if it's a compound set created by set operations.

        Notes
        -----
        Simple sets have a direct _FlattenPdfLineSet as _left and no _right.
        Compound sets have binary operations stored in _right.
        """
        return isinstance(self._left, _FlattenPdfLineSet) and self._right is None

    @property
    def is_concrete(self):
        l_concrete = self._left.is_concrete
        r_concrete = None
        if self._right is None or not l_concrete:
            return l_concrete
        if self._right is not None:
            right = self._right[1]
            r_concrete = right.is_concrete
        return l_concrete and r_concrete

    @property
    def one_d(self):
        if not self.is_simple:
            return False
        dim = 0
        for d in ["font", "font_size", "area", "text"]:
            if getattr(self._left, d) is not None:
                dim += 1
            if dim > 1:
                return False
        return True

    def disjoint(self, other):
        if not self.is_simple or not other.is_simple:
            return None
        sbj = self._left
        obj = other._left
        if not sbj.font.isdisjoint(obj.font):
            return False
        if sbj.font_size.overlap(obj.font_size):
            return False
        if sbj.area.intersect(obj.area):
            return False
        if not sbj.text.is_simple or not obj.text.is_simple:
            return None
        stxt = sbj.text._left
        otxt = obj.text._left
        if not stxt.disjoint(otxt):
            return False
        return True

    def __or__(self, other: "PdfLineSet") -> "PdfLineSet":
        """Create the union of two PDF line sets (set operation).

        Parameters
        ----------
        other : PdfLineSet
            The other PDF line set to combine with this one

        Returns
        -------
        PdfLineSet
            A new PDF line set representing the union of both sets

        Notes
        -----
        - If both sets are simple and concrete, performs direct union
        - Otherwise creates a compound set with OR operation
        - The union includes lines that match either set's criteria

        Examples
        --------
        >>> set1 = PdfLineSet(font="Arial")
        >>> set2 = PdfLineSet(font_size=12)
        >>> union_set = set1 | set2  # Lines with Arial font OR size 12
        """
        newset = PdfLineSet()
        if self.is_simple and self.is_concrete and other.one_d and other.is_concrete:
            # Direct union for simple, concrete sets
            newset._left._font = _op_over_none(or_, self._left.font, other.font)
            newset._left._font_size = _op_over_none(
                or_, self._left.font_size, other._left.font_size
            )
            newset._left._area = _op_over_none(or_, self._left.area, other._left.area)
            newset._left._text = _op_over_none(or_, self._left.text, other._left.text)
            return newset
        # Create compound set for complex cases
        newset._left = self
        newset._right = (ast.Or, other)
        return newset

    def __and__(self, other: "PdfLineSet") -> "PdfLineSet":
        """Create the intersection of two PDF line sets (set operation).

        Parameters
        ----------
        other : PdfLineSet
            The other PDF line set to intersect with this one

        Returns
        -------
        PdfLineSet
            A new PDF line set representing the intersection of both sets

        Notes
        -----
        - If both sets are simple and concrete, performs direct intersection
        - Otherwise creates a compound set with AND operation
        - The intersection includes only lines that match both sets' criteria

        Examples
        --------
        >>> set1 = PdfLineSet(font="Arial")
        >>> set2 = PdfLineSet(font_size=12)
        >>> intersection_set = set1 & set2  # Lines with Arial font AND size 12
        """
        newset = PdfLineSet()
        if self.is_simple and other.one_d and self.is_concrete and other.is_concrete:
            # Direct intersection for simple, concrete sets
            newset._left._font = _op_over_none(and_, self._left.font, other._left.font)
            newset._left._font_size = _op_over_none(
                and_, self._left.font_size, other._left.font_size
            )
            newset._left._area = _op_over_none(and_, self._left.area, other._left.area)
            newset._left._text = _op_over_none(and_, self._left.text, other._left.text)
            return newset
        # Create compound set for complex cases
        newset._left = self
        newset._right = (ast.And, other)
        return newset

    def __truediv__(self, other: "PdfLineSet") -> "PdfLineSet":
        """Create the difference between two PDF line sets (set operation).

        Parameters
        ----------
        other : PdfLineSet
            The PDF line set to subtract from this one

        Returns
        -------
        PdfLineSet
            A new PDF line set representing lines in this set but not in the other

        Notes
        -----
        - If both sets are simple and concrete, performs direct difference
        - Otherwise creates a compound set with DIV operation
        - The difference includes lines that match this set but not the other
        - For text sets, uses truediv operation which handles regex patterns

        Examples
        --------
        >>> set1 = PdfLineSet(font="Arial")
        >>> set2 = PdfLineSet(font_size=12)
        >>> difference_set = set1 / set2  # Lines with Arial font but NOT size 12
        """
        newset = PdfLineSet()
        if self.is_simple and other.one_d and self.is_concrete and other.is_concrete:
            # Direct difference for simple, concrete sets
            sf, of = self._left.font, other._left.font
            sfs, ofs = self._left.font_size, other._left.font_size
            st, ot = self._left.text, other._left.text
            sa, oa = self._left.area, other._left.area

            # Handle None cases by creating universal sets
            if sf is None and of is not None:
                sf = AllFonts()
            if sfs is None and ofs is not None:
                sf = FontSizeSet.from_range(1e-6, 1e6)
            if st is None and ot is not None:
                st = TextSet("")
            if sa is None and oa is not None:
                sa = box(-1e6, -1e6, 1e6, 1e6)

            # Perform set difference operations
            newset._left._font = _op_over_none(sub, sf, of)
            newset._left._font_size = _op_over_none(sub, sfs, ofs)
            newset._left._area = _op_over_none(sub, sa, oa)
            newset._left._text = _op_over_none(truediv, st, ot)
            return newset
        # Create compound set for complex cases
        newset._left = self
        newset._right = (ast.Div, other)
        return newset

    def __sum__(self, other):
        return self | other

    def __sub__(self, other):
        return self / other

    def __repr__(self):
        bin_ops = {
            ast.And: "AND",
            ast.Or: "OR",
            ast.Div: "BESIDES",
        }
        if isinstance(self._left, _FlattenPdfLineSet):
            left_string = (f"{repr(self._left)}").replace(
                self._left.__class__.__name__, self.__class__.__name__
            )
        else:
            left_string = f"{repr(self._left)}"

        if self._right is not None:
            op, right = self._right
            if right._right is not None:
                right_string = "-----------------------------------\n"
                right_string += repr(right)
                right_string += "-----------------------------------"
            else:
                right_string = repr(right)

            string = f"{left_string}\n\t{bin_ops[op]}\n{right_string}"
        else:
            string = left_string
        return string

    def __contains__(self, other):
        bin_ops = {
            ast.And: lambda v1, v2: v1 and v2,
            ast.Or: lambda v1, v2: v1 or v2,
            ast.Div: lambda v1, v2: v1 and not v2,
        }
        in_set = other in self._left
        if self._right is None:
            return in_set
        op, right = self._right
        return bin_ops[op](in_set, other in right)

    @classmethod
    def from_dict(cls, data):
        ls = InputPdfLineSet(**data)
        input_area = ls.area.model_dump() if ls.area is not None else None
        return cls(
            font=FontSet(ls.font) if ls.font is not None else None,
            font_size=FontSizeSet.from_range(ls.font_size - 1e-3, ls.font_size + 1e-3)
            if ls.font_size is not None
            else None,
            area=box(
                input_area["x_min"] if input_area["x_min"] is not None else -1e6,
                input_area["y_min"] if input_area["y_min"] is not None else -1e6,
                input_area["x_max"] if input_area["x_max"] is not None else +1e6,
                input_area["y_max"] if input_area["y_max"] is not None else +1e6,
            )
            if input_area is not None
            else None,
            text=TextSet(ls.text) if ls.text is not None else None,
        )

    @classmethod
    def from_str(cls, string):
        matched = _LINE_SET_REGEXP.match(string).groupdict()
        area = None
        tmp_area = matched["area"]
        tmp_range = matched["y_range"]

        def _to_floats(x):
            return (
                (float(c) if c != "" else None)
                for c in x.replace("(", "").replace(")", "").split(":")
            )

        if tmp_area is not None:
            x_range, y_range = tmp_area.split(")(")
            x_min, x_max = _to_floats(x_range)
            y_min, y_max = _to_floats(y_range)
            area = box(
                x_min if x_min is not None else -1e6,
                y_min if y_min is not None else -1e6,
                x_max if x_max is not None else +1e6,
                y_max if y_max is not None else +1e6,
            )
        elif tmp_range is not None:
            y_min, y_max = _to_floats(tmp_range)
            area = box(
                -1e6,
                y_min if y_min is not None else -1e6,
                1e6,
                y_max if y_max is not None else +1e6,
            )
        fs = matched["font_size"]
        fs = float(fs) if fs is not None else None
        return cls(
            font=FontSet(matched["font"].strip())
            if matched["font"] is not None
            else None,
            font_size=FontSizeSet.from_range(fs - 1e-3, fs + 1e-3)
            if fs is not None
            else None,
            area=area,
            text=TextSet(matched["text"]) if matched["text"] is not None else None,
        )

    def contextualize(self, xml_root):
        concrete = PdfLineSet()
        concrete._left = self._left.contextualize(xml_root)
        if self._right is not None:
            op, right = self._right
            concrete._right = (op, right.contextualize(xml_root))
        return concrete


def _pdf_line_set_aggregator(attribute):
    def wrapper(agg_func):
        def new_agg(value, lines, xml_root):
            value = value.contextualize(xml_root)
            inputs = [getattr(l, attribute) for l in lines if l in value]
            return agg_func(inputs)

        return new_agg

    return wrapper


@_pdf_line_set_aggregator("font")
def _default_font_agg(fonts):
    return FontSet(*fonts)


@_pdf_line_set_aggregator("font_size")
def _default_font_size_agg(font_sizes):
    if len(font_sizes) == 1:
        fs = font_sizes[0]
        font_sizes = [fs - 1e-4, fs + 1e-4]
    return FontSizeSet.from_range(min(*font_sizes), max(*font_sizes))


@_pdf_line_set_aggregator("text")
def _default_text_agg(texts):
    return TextSet(*[f"^{t}$" for t in texts])


@_pdf_line_set_aggregator("area")
def _pdflineset_area_agg(areas):
    if len(areas) == 1:
        y_max = areas[0].bounds[1]
        return box(-1e6, -1e6, 1e6, y_max)
    if len(areas) == 2:
        x_min0, y_min0, x_max0, y_max0 = areas[0].bounds
        x_min1, y_min1, x_max1, y_max1 = areas[1].bounds
        h_a = y_max0 - y_min1
        h_b = y_max1 - y_min0
        h = min(abs(h_a), abs(h_b))
        w_a = x_max0 - x_min1
        w_b = x_max1 - x_min0
        w = min(abs(w_a), abs(w_b))
        if h > w:
            return box(-1e6, min(y_max0, y_max1), 1e6, max(y_min0, y_min1))
        return box(min(x_max0, x_max1), -1e6, min(x_min0, x_min1), 1e6)
    if len(areas) == 3:
        x_min1, y_min1, x_max1, y_max1 = areas[0].bounds
        x_min2, y_min2, x_max2, y_max2 = areas[1].bounds
        x_min3, y_min3, x_max3, y_max3 = areas[2].bounds
        h_a12 = y_max1 - y_min2
        h_b12 = y_max2 - y_min1
        h_a23 = y_max2 - y_min3
        h_b23 = y_max3 - y_min2
        h_a13 = y_max1 - y_min3
        h_b13 = y_max3 - y_min1
        h_12 = min(abs(h_a12), abs(h_b12))
        h_23 = min(abs(h_a23), abs(h_b23))
        h_13 = min(abs(h_a13), abs(h_b13))
        i_h, h = max((0, h_12), (1, h_23), (2, h_13), key=lambda x: x[1])

        w_a12 = x_max1 - x_min2
        w_b12 = x_max2 - x_min1
        w_a23 = x_max2 - x_min3
        w_b23 = x_max3 - x_min2
        w_a13 = x_max1 - x_min3
        w_b13 = x_max3 - x_min1
        w_12 = min(abs(w_a12), abs(w_b12))
        w_23 = min(abs(w_a23), abs(w_b23))
        w_13 = min(abs(w_a13), abs(w_b13))
        i_w, w = max((0, w_12), (1, w_23), (2, w_13), key=lambda x: x[1])
        if h > w:
            y_maxs = [(y_max1, y_max2), (y_max2, y_max3), (y_max1, y_max3)]
            y_mins = [(y_min1, y_min2), (y_min2, y_min3), (y_min1, y_min3)]
            other_x = [(x_min3, x_max3), (x_min1, x_max1), (x_min2, x_max2)]
            x_mins, x_maxs = tuple(zip(*other_x))
            return box(
                other_x[i_h][1] if other_x[i_h][1] == min(*x_maxs) else -1e6,
                min(*y_maxs[i_h]),
                other_x[i_h][0] if other_x[i_h][0] == max(*x_mins) else 1e6,
                max(*y_mins[i_h]),
            )
        x_maxs = [(x_max1, x_max2), (x_max2, x_max3), (x_max1, x_max3)]
        x_mins = [(x_min1, x_min2), (x_min2, x_min3), (x_min1, x_min3)]
        other_y = [(y_min3, y_max3), (y_min1, y_max1), (y_min2, y_max2)]
        y_mins, y_maxs = tuple(zip(*other_y))
        return box(
            min(*x_maxs[i_w]),
            other_y[i_w][1] if other_y[i_w][1] == min(*y_maxs) else -1e6,
            max(*x_mins[i_w]),
            other_y[i_w][0] if other_y[i_w][0] == max(*y_mins) else 1e6,
        )

    bounds = [a.bounds for a in areas]
    x_mins, y_mins, x_maxs, y_maxs = tuple(zip(*bounds))
    return box(min(*x_maxs), min(*y_maxs), max(*x_mins), max(*y_mins))


def _default_area_agg(value, lines, xml_root):
    concrete_values = {"x_min": None, "x_max": None, "y_min": None, "y_max": None}
    for k in concrete_values:
        vl = value[k]
        if isinstance(vl, PdfLineSet):
            vl = vl.contextualize(xml_root)
            bounds = [l.area.bounds for l in lines if l in vl]
            if len(bounds) == 0:
                vl = None
            elif len(bounds) == 1:
                vl = bounds[0][{"x_min": 2, "y_min": 3, "x_max": 0, "y_max": 1}[k]]
            else:
                bounds_t = tuple(zip(*bounds))
                vl = {
                    "x_min": max(*bounds_t[2]),
                    "y_min": max(*bounds_t[3]),
                    "x_max": min(*bounds_t[0]),
                    "y_max": min(*bounds_t[1]),
                }[k]
        concrete_values[k] = vl
    return box(
        concrete_values["x_min"] if concrete_values["x_min"] is not None else -1e6,
        concrete_values["y_min"] if concrete_values["y_min"] is not None else -1e6,
        concrete_values["x_max"] if concrete_values["x_max"] is not None else +1e6,
        concrete_values["y_max"] if concrete_values["y_max"] is not None else +1e6,
    )


def _relative_area_agg(value, lines, xml_root):
    ref = None
    x = 0.0
    y = 0.0
    w = 1.0
    h = 1.0
    if len(value) == 2:
        ref, (x, y) = value
    elif len(value) == 3:
        ref, (x, y), (w, h) = value
    else:
        raise ValueError(_("Wrong number of arguent in tuple aggregation"))
    ref = ref.contextualize(xml_root)
    ref_concrete = [l for l in lines if l in ref][0]
    x0, y0, x1, y1 = ref_concrete.area.bounds
    w0 = x1 - x0
    h0 = y1 - y0
    return box(x0 + x * w0, y0 + y * h0, x0 + (w + x) * w0, y0 + (h + y) * h0)


_font_aggregators = [(PdfLineSet, _default_font_agg)]
_font_size_aggregators = [(PdfLineSet, _default_font_size_agg)]
_text_aggregators = [(PdfLineSet, _default_text_agg)]
_area_aggregators = [
    (dict, _default_area_agg),
    (tuple, _relative_area_agg),
    (PdfLineSet, _pdflineset_area_agg),
]
