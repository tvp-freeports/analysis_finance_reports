"""Definition of types for identifying characteristics related to typographic aspects of PDF lines.

This module provides classes for working with font properties in PDF documents,
including font names, font sizes, and text content matching with regex support.
"""

from typing import TypeAlias, Optional
import ast
from portion.interval import Interval
from portion.const import Bound, inf
from freeports_analysis.i18n import _


class Font(str):
    """A string-based representation of a font name.

    This class extends str to provide type safety for font names while
    maintaining all string functionality.
    """

    pass


class FontSet(set):
    """A set of font names for filtering PDF lines by font.

    This class extends set to provide specialized font filtering capabilities.

    Parameters
    ----------
    *elements : str
        Font names to include in the set
    """

    def __init__(self, *elements):
        super().__init__([Font(e) for e in elements])

    def __repr__(self):
        return f"{super().__repr__()}"


class AllFonts(FontSet):
    """A special FontSet that matches all fonts.

    This class represents a universal font set that will match any font
    when used in PDF line filtering operations.
    """

    def __repr__(self):
        return f"{super().__repr__()}".replace("()", "({...})").replace(
            self.__class__.__name__, "FontSet"
        )

    def __contains__(self, font):
        return True


class FontSize(float):
    """A class representing a font size in points.

    This class extends float to provide type safety for font sizes
    while maintaining all float functionality.

    Parameters
    ----------
    value : float
        Font size in points

    Raises
    ------
    ValueError
        If the font size is negative
    """

    def __new__(cls, value):
        if value not in [inf, -inf]:
            value = super().__new__(cls, value)
            if value < 0:
                raise ValueError(_("FontSize cannot be negative"))
        return value


class FontSizeInterval(Interval):
    """An interval representation for font size ranges.

    This class extends Interval to provide specialized functionality
    for working with font size ranges in PDF line filtering.
    """

    @classmethod
    def from_atomic(cls, left, lower, upper, right):
        """Create a FontSizeInterval from atomic bounds.

        Parameters
        ----------
        left : Bound
            Left bound type (OPEN or CLOSED)
        lower : float
            Lower bound value
        upper : float
            Upper bound value
        right : Bound
            Right bound type (OPEN or CLOSED)

        Returns
        -------
        FontSizeInterval
            A new FontSizeInterval instance
        """
        return super().from_atomic(left, FontSize(lower), FontSize(upper), right)

    @classmethod
    def from_range(cls, lower: float, upper: float) -> "FontSizeInterval":
        """Create a FontSizeInterval from a range of values.

        Parameters
        ----------
        lower : float
            Lower bound of the font size range
        upper : float
            Upper bound of the font size range

        Returns
        -------
        FontSizeInterval
            A new FontSizeInterval representing the specified range

        Notes
        -----
        - Uses closed bounds for finite values
        - Uses open bounds for infinite values
        - Automatically converts bounds to FontSize instances
        """
        return cls.from_atomic(
            Bound.CLOSED if lower not in [inf, -inf] else Bound.OPEN,
            lower,
            upper,
            Bound.CLOSED if upper not in [inf, -inf] else Bound.OPEN,
        )

    def __repr__(self):
        return f"{self.__class__.__name__}({super().__repr__()})"


FontSizeSet: TypeAlias = FontSizeInterval
"""Type alias for FontSizeInterval for backward compatibility and clarity."""


class _AtomicTextSet(str):
    """Atomic text set element with regex-like matching capabilities.

    This class represents a single text pattern that can match PDF line text
    with beginning (^) and end ($) anchors similar to regex patterns.

    Parameters
    ----------
    value : str
        Text pattern to match
    """

    def __new__(cls, value):
        if isinstance(value, cls):
            return value
        effective_text = value
        begin = False
        end = False
        if len(effective_text) >= 2 and effective_text.startswith(r"\^"):
            effective_text = effective_text[1:]  # Remove the backslash
        elif effective_text.startswith("^"):
            effective_text = effective_text[1:]
            begin = True

        # Check for escaped $ at the end
        if len(effective_text) >= 2 and effective_text.endswith(r"\$"):
            effective_text = (
                effective_text[:-2] + effective_text[-1]
            )  # Remove the backslash
        elif effective_text.endswith("$"):
            effective_text = effective_text[:-1]
            end = True

        pdflinetext = super().__new__(cls, effective_text)
        pdflinetext._begin = begin
        pdflinetext._end = end
        return pdflinetext

    def __repr__(self):
        string = "^" if self._begin else ""
        effective = str(self)
        if len(effective) > 0:
            if effective[0] == "^":
                effective = "\\" + effective
            if effective[-1] == "$":
                effective = effective[:-1] + "\$"
        string += effective
        string += "$" if self._end else ""
        return string

    def __contains__(self, other: str):
        """Check if this text pattern matches the given string.

        Parameters
        ----------
        other : str
            String to check for matching

        Returns
        -------
        bool
            True if the pattern matches the string, False otherwise
        """
        if self._begin and self._end:
            return str(self) == other
        if self._begin:
            return other.startswith(self)
        if self._end:
            return other.endswith(self)
        return str(self) in other

    def __hash__(self):
        return hash((str(self), self._begin, self._end))

    def __eq__(self, other):
        return hash(self) == hash(other)

    def __gt__(self, other):
        if self._begin and not other._begin:
            return False
        if self._end and not other._end:
            return False
        if self == other:
            return False
        string = str(other)
        if string in self:
            return True
        return False

    def __lt__(self, other):
        return other > self

    def __ge__(self, other):
        return self > other or self == other

    def __le__(self, other):
        return other >= self

    def disjoint(self, other):
        """Check if two atomic text sets are disjoint (no overlap).

        Parameters
        ----------
        other : _AtomicTextSet
            Other atomic text set to check

        Returns
        -------
        bool
            True if the sets are disjoint, False otherwise
        """

        def _both(a):
            return a._begin and a._end

        def _neither(a):
            return not a._begin and not a._end

        if _both(self):
            return str(self) not in other
        if _both(other):
            return str(other) not in self
        if _neither(self) or _neither(other):
            return False
        if self._begin and other._end:
            return False
        if self._end and other._begin:
            return False
        if str(self) in other:
            return False
        if str(other) in self:
            return False
        return True


class _FlattenTextSet(set):
    """A flattened set of atomic text patterns for efficient text matching."""

    def _normalize(self):
        """Remove redundant patterns from the set."""
        to_remove = set()
        for e_i in self:
            for e_j in self:
                if e_i < e_j:
                    to_remove.add(e_i)
                    break
        self.difference_update(to_remove)

    def __new__(cls, *elements):
        if len(elements) == 1 and isinstance(elements[0], cls):
            elements[0]._normalize()
            return elements[0]
        return super().__new__(cls)

    def __init__(self, *elements):
        if not (len(elements) == 1 and isinstance(elements[0], _FlattenTextSet)):
            super().__init__([_AtomicTextSet(e) for e in elements])
            self._right = None
        self._normalize()

    def __repr__(self):
        return f"{super().__repr__()}"

    def __gt__(self, other):
        if self == other:
            return False
        for atomic_other in other:
            subset = False
            for atomic_self in self:
                if atomic_self >= atomic_other:
                    subset = True
                    break
            if not subset:
                return False
        return True

    def __lt__(self, other):
        return other > self

    def __ge__(self, other):
        return self > other or self == other

    def __le__(self, other):
        return other >= self

    def __contains__(self, other):
        BIN_OPS = {
            ast.BitAnd: lambda v1, v2: v1 and v2,
            ast.BitOr: lambda v1, v2: v1 or v2,
            ast.Div: lambda v1, v2: v1 and not v2,
        }
        in_set = False
        in_right = False
        for e in self:
            if other in e:
                in_set = True
                break
        if self._right is None:
            return in_set
        else:
            op, right = self._right
            return BIN_OPS[op](in_set, right)

    def disjoint(self, other):
        """Check if two flattened text sets are disjoint.

        Parameters
        ----------
        other : _FlattenTextSet
            Other flattened text set to check

        Returns
        -------
        bool
            True if the sets are disjoint, False otherwise
        """
        for e in self:
            for o in other:
                if not e.disjoint(o):
                    return False
        return True


class TextSet:
    """A set-like container for text patterns with set operations.

    This class provides sophisticated text matching capabilities with
    support for set operations (union, intersection, difference).

    Parameters
    ----------
    *elements : str
        Text patterns to include in the set
    """

    def __init__(self, *elements):
        self._left = _FlattenTextSet(*elements)
        self._right = None

    @property
    def is_simple(self):
        """Check if this is a simple (non-compound) text set.

        Returns
        -------
        bool
            True if the set is simple, False if it's a compound set
        """
        return isinstance(self._left, _FlattenTextSet) and self._right is None

    def __or__(self, other):
        """Create the union of two text sets.

        Parameters
        ----------
        other : TextSet
            Other text set to combine with

        Returns
        -------
        TextSet
            Union of the two text sets
        """
        newset = TextSet()
        if self.is_simple and other.is_simple:
            newset._left = _FlattenTextSet(*(list(self._left) + list(other._left)))
            return newset
        newset._left = self
        newset._right = (ast.Or, other)
        return newset

    def __and__(self, other):
        """Create the intersection of two text sets.

        Parameters
        ----------
        other : TextSet
            Other text set to intersect with

        Returns
        -------
        TextSet
            Intersection of the two text sets
        """
        newset = TextSet()
        if self.is_simple and other.is_simple:
            if self._left.disjoint(other._left):
                newset._left = _FlattenTextSet("^$")
                return newset
            if self._left <= other._left:
                newset._left = self._left
                return newset
            elif self._left >= other._left:
                newset._left = other._left
                return newset
        newset._left = self
        newset._right = (ast.And, other)
        return newset

    def __truediv__(self, other):
        """Create the difference between two text sets.

        Parameters
        ----------
        other : TextSet
            Text set to subtract

        Returns
        -------
        TextSet
            Difference between the text sets
        """
        newset = TextSet()
        if self.is_simple and other.is_simple:
            if self._left.disjoint(other._left):
                newset._left = self._left
                return newset
            if self._left <= other._left:
                newset._left = _FlattenTextSet("^$")
                return newset
        newset._left = self
        newset._right = (ast.Div, other)
        return newset

    def __sum__(self, other):
        return self | other

    def __sub__(self, other):
        return self / other

    def __repr__(self):
        BIN_OPS = {
            ast.And: "&",
            ast.Or: "|",
            ast.Div: "/",
        }
        if isinstance(self._left, _FlattenTextSet):
            left_string = f"{self.__class__.__name__}({set(self._left)})"
        else:
            left_string = f"{repr(self._left)}"

        if self._right is not None:
            op, right = self._right
            right_string = (
                f"{repr(right)}" if right._right is None else f"[{repr(right)}]"
            )
            string = f"{left_string} {BIN_OPS[op]} {right_string}"
        else:
            string = left_string
        return string

    def __contains__(self, other: str):
        """Check if a string matches any pattern in the text set.

        Parameters
        ----------
        other : str
            String to check for matching

        Returns
        -------
        bool
            True if the string matches any pattern, False otherwise
        """
        BIN_OPS = {
            ast.And: lambda v1, v2: v1 and v2,
            ast.Or: lambda v1, v2: v1 or v2,
            ast.Div: lambda v1, v2: v1 and not v2,
        }
        in_right = False
        in_set = other in self._left
        if not in_set or self._right is None:
            return in_set
        else:
            op, right = self._right
            return BIN_OPS[op](in_set, other in right)
