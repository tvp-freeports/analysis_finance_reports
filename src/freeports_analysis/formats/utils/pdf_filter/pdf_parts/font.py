"""Definition of types for identify carateristic related with typographic aspect of the line"""

from typing import TypeAlias
import ast
from portion.interval import Interval
from portion.const import Bound, inf
from freeports_analysis.i18n import _


class Font(str):
    pass


class FontSet(set):
    def __init__(self, *elements):
        super().__init__([Font(e) for e in elements])

    def __repr__(self):
        return f"{super().__repr__()}"


class AllFonts(FontSet):
    def __repr__(self):
        return f"{super().__repr__()}".replace("()", "({...})").replace(
            self.__class__.__name__, "FontSet"
        )

    def __contains__(self, font):
        return True


class FontSize(float):
    """A class rappresenting a font"""

    def __new__(cls, value):
        if value not in [inf, -inf]:
            value = super().__new__(cls, value)
            if value < 0:
                raise ValueError(_("FontSize cannot be negative"))
        return value


class FontSizeInterval(Interval):
    @classmethod
    def from_atomic(cls, left, lower, upper, right):
        return super().from_atomic(left, FontSize(lower), FontSize(upper), right)

    @classmethod
    def from_range(cls, lower, upper):
        return cls.from_atomic(
            Bound.CLOSED if lower not in [inf, -inf] else Bound.OPEN,
            lower,
            upper,
            Bound.CLOSED if upper not in [inf, -inf] else Bound.OPEN,
        )

    def __repr__(self):
        return f"{self.__class__.__name__}({super().__repr__()})"


FontSizeSet: TypeAlias = FontSizeInterval


class _AtomicTextSet(str):
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
    def _normalize(self):
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
        for e in self:
            for o in other:
                if not e.disjoint(o):
                    return False
        return True


class TextSet:
    def __init__(self, *elements):
        self._left = _FlattenTextSet(*elements)
        self._right = None

    @property
    def is_simple(self):
        return isinstance(self._left, _FlattenTextSet) and self._right is None

    def __or__(self, other):
        newset = TextSet()
        if self.is_simple and other.is_simple:
            newset._left = _FlattenTextSet(*(list(self._left) + list(other._left)))
            return newset
        newset._left = self
        newset._right = (ast.Or, other)
        return newset

    def __and__(self, other):
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
