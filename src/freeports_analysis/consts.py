"""Provides basic constants and types used by all submodules.

This module facilitates avoiding circular imports by providing shared
constants, types, and utility functions for the entire codebase.
"""

import ast
import operator
from enum import Enum, auto, Flag
from typing import Type, TypeAlias, Any, TypeVar, Annotated, Optional, Union, Dict
import logging as log
import pandas as pd
from pydantic import BeforeValidator
from freeports_analysis.i18n import _

logger = log.getLogger(__name__)

PROGRAM_DESCRIPTION = _(
    """Analyze finance reports searching for investing in companies
allegedly involved interantional law violations by third parties
"""
)


def flag_to_string(flags: Flag) -> str:
    """Convert a Flag object to a string representation using bitwise OR syntax.

    Parameters
    ----------
    flags : Flag
        The flag object to convert to string

    Returns
    -------
    str
        String representation of flags using '|' as separator
    """
    string = ""
    first = True
    clss = flags.__class__
    for f in clss:
        if f in flags:
            if not first:
                string += " | "
            string += f.name
            first = False
    return string


def flag_from_string(expression: Optional[Union[str, list]], cls: Type[Flag]) -> Flag:
    """Convert a string expression to a Flag object.

    Parameters
    ----------
    expression : Optional[Union[str, list]]
        String expression or list of flag names to convert
    cls : Type[Flag]
        The Flag class to instantiate

    Returns
    -------
    Flag
        Flag object created from the expression

    Raises
    ------
    ValueError
        If the expression contains unsupported operations or invalid flag names
    """
    bin_ops = {
        ast.BitAnd: operator.and_,
        ast.BitOr: operator.or_,
        ast.BitXor: operator.xor,
    }
    unary_ops = {ast.Invert: operator.invert}

    def _from_ast(node: ast.AST, flag_cls: Type[Flag]) -> Flag:
        if isinstance(node, ast.Expression):
            return _from_ast(node.body, flag_cls)
        if isinstance(node, ast.BinOp):
            left = node.left
            right = node.right
            op = type(node.op)
            if op in bin_ops:
                return bin_ops[op](
                    _from_ast(left, flag_cls), _from_ast(right, flag_cls)
                )
            raise ValueError(_("Binary operation {} not supported").format(op))
        if isinstance(node, ast.UnaryOp):
            operand = node.operand
            op = type(node.op)
            if op in unary_ops:
                return unary_ops[op](_from_ast(operand, flag_cls))
            raise ValueError(_("Unary operation {} not supported").format(op))
        if isinstance(node, ast.Name):
            name = node.id.upper()
            if hasattr(flag_cls, name):
                return getattr(flag_cls, name)
            raise ValueError(_("Invalid flag {}").format(name))
        raise ValueError(_("Unsupported AST node: {}").format(type(node)))

    if isinstance(expression, list):
        expression = " | ".join(expression)
        return flag_from_string(expression, cls)
    if pd.isna(expression):
        return None
    if isinstance(expression, str):
        if expression.strip() == "":
            return cls(0)
        expression = ast.parse(expression, mode="eval")
        return _from_ast(expression, cls)
    raise ValueError(_("Flags should be specified with list or string expression"))


T = TypeVar("T", bound=Flag)


def _cast_input_flags(flag_cls: Type[T], value: Any) -> T:
    """Cast input value to Flag type.

    Parameters
    ----------
    flag_cls : Type[T]
        The Flag class to cast to
    value : Any
        The value to cast

    Returns
    -------
    T
        Cast Flag object
    """
    if isinstance(value, flag_cls):
        return value
    if isinstance(value, Flag):
        value = flag_to_string(value)
    return flag_from_string(value, flag_cls)


def input_flags(flag_cls: Type[T]) -> type:
    """Create an annotated type for Flag input validation.

    Parameters
    ----------
    flag_cls : Type[T]
        The Flag class to validate against

    Returns
    -------
    type
        Annotated type for Pydantic validation
    """
    return Annotated[
        flag_cls,
        BeforeValidator(lambda value: _cast_input_flags(flag_cls, value)),
    ]


S = TypeVar("S", bound=Enum)


def _cast_input_enum(enum_cls: Type[S], value: Any) -> S:
    """Cast input value to Enum type.

    Parameters
    ----------
    enum_cls : Type[S]
        The Enum class to cast to
    value : Any
        The value to cast

    Returns
    -------
    S
        Cast Enum object
    """
    if isinstance(value, enum_cls):
        return value
    if isinstance(value, Enum):
        value = value.name
    return enum_cls[value.strip().upper()]


def input_enum(enum_cls: Type[S]) -> type:
    """Create an annotated type for Enum input validation.

    Parameters
    ----------
    enum_cls : Type[S]
        The Enum class to validate against

    Returns
    -------
    type
        Annotated type for Pydantic validation
    """
    return Annotated[
        enum_cls,
        BeforeValidator(lambda value: _cast_input_enum(enum_cls, value)),
    ]


class FinancialInstrument(Enum):
    """Enumeration of financial instrument types."""

    EQUITY = auto()
    BOND = auto()


class Currency(Enum):
    """Enumeration of supported currency codes.

    Contains standard 3-letter ISO currency codes for major world currencies.
    """

    USD = "USD"
    EUR = "EUR"
    EURO = "EUR"
    GBP = "GBP"
    JPY = "JPY"
    CNY = "CNY"
    AUD = "AUD"
    CAD = "CAD"
    CHF = "CHF"
    SEK = "SEK"
    NOK = "NOK"
    DKK = "DKK"
    SGD = "SGD"
    HKD = "HKD"
    KRW = "KRW"
    INR = "INR"
    BRL = "BRL"
    MXN = "MXN"
    RUB = "RUB"
    ZAR = "ZAR"
    TRY = "TRY"
    PLN = "PLN"
    THB = "THB"
    IDR = "IDR"
    MYR = "MYR"
    PHP = "PHP"
    ILS = "ILS"
    AED = "AED"
    SAR = "SAR"
    QAR = "QAR"
    KWD = "KWD"
    CLP = "CLP"
    COP = "COP"
    PEN = "PEN"
    ARS = "ARS"
    VND = "VND"
    UAH = "UAH"
    CZK = "CZK"
    HUF = "HUF"
    RON = "RON"
    HRK = "HRK"
    BGN = "BGN"
    ISK = "ISK"
    NZD = "NZD"
    EGP = "EGP"
    TWD = "TWD"

    @property
    def symbol(self) -> str:
        """Get the currency symbol for this currency.

        Returns
        -------
        str
            The currency symbol
        """
        return {
            "USD": "$",
            "EUR": "€",
            "GBP": "£",
            "JPY": "¥",
            "CNY": "¥",
            "AUD": "$",
            "CAD": "$",
            "CHF": "CHF",
            "SEK": "kr",
            "NOK": "kr",
            "DKK": "kr",
            "SGD": "$",
            "HKD": "$",
            "KRW": "₩",
            "INR": "₹",
            "BRL": "R$",
            "MXN": "$",
            "RUB": "₽",
            "ZAR": "R",
            "TRY": "₺",
            "PLN": "zł",
            "THB": "฿",
            "IDR": "Rp",
            "MYR": "RM",
            "PHP": "₱",
            "ILS": "₪",
            "AED": "د.إ",
            "SAR": "﷼",
            "QAR": "ر.ق",
            "KWD": "د.ك",
            "EGP": "ج.م",
            "CLP": "$",
            "COP": "$",
            "PEN": "S/.",
            "ARS": "$",
            "VND": "₫",
            "UAH": "₴",
            "CZK": "Kč",
            "HUF": "Ft",
            "RON": "lei",
            "HRK": "kn",
            "BGN": "лв",
            "ISK": "kr",
            "NZD": "$",
            "TWD": "$",
        }[self.value]


PromisesResolutionMap: TypeAlias = Dict[str, Any]
"""Type alias for promise resolution mapping.

A dictionary mapping promise IDs to their resolved values.
"""


class Promise:
    """Base class for deferred value resolution in financial data processing.

    Implements a promise pattern where values can be resolved later from a mapping.

    Attributes
    ----------
    _id : str
        The unique identifier for this promise

    Methods
    -------
    fulfill_with(mapping: PromisesResolutionMap) -> Any
        Resolves the promised value from the given mapping.
    """

    def __init__(self, promise_id: str):
        """Initialize a Promise with a unique identifier.

        Parameters
        ----------
        promise_id : str
            Unique identifier for this promise
        """
        self._id = str(promise_id)

    def fulfill_with(self, mapping: PromisesResolutionMap) -> Any:
        """Resolve this promise's value from the given mapping.

        Parameters
        ----------
        mapping : PromisesResolutionMap
            Dictionary containing values to resolve promises from

        Returns
        -------
        Any
            The resolved value from the mapping
        """
        return mapping[str(self)]

    def __str__(self) -> str:
        """Get string representation of the promise.

        Returns
        -------
        str
            The promise's unique identifier
        """
        return self._id

    def __repr__(self) -> str:
        """Get detailed string representation showing promise class and ID.

        Returns
        -------
        str
            String representation showing promise class and ID
        """
        return f'{self.__class__.__name__}("{str(self)}")'

    def __eq__(self, other: object) -> bool:
        """Check equality with another promise.

        Parameters
        ----------
        other : object
            The object to compare with

        Returns
        -------
        bool
            True if promises have the same ID
        """
        if not isinstance(other, Promise):
            return False
        return self._id == other._id

    def __format__(self, fmt: str) -> str:
        """Format the promise for string formatting.

        Parameters
        ----------
        fmt : str
            Format specification

        Returns
        -------
        str
            Formatted string representation
        """
        return repr(self)


PromisesResolutionContext: TypeAlias = Dict[str, Union[Promise, Any]]
"""Type alias for promise resolution context.

A dictionary containing promises and their dependencies during resolution.
"""


class CircularPromisesChain(Exception):
    """Exception raised when a circular dependency is detected in promise resolution.

    This occurs when a promise chain references itself either directly or indirectly,
    creating an infinite loop that cannot be resolved.
    """


def flatten_promise_map(mapping: PromisesResolutionMap) -> PromisesResolutionMap:
    """Flatten a mapping containing Promise objects by resolving all references.

    Processes a dictionary that may contain Promise objects, resolving each promise
    by looking up its value in the mapping until all values are concrete (non-Promise).
    Detects and prevents circular references that would cause infinite resolution loops.

    Parameters
    ----------
    mapping : PromisesResolutionMap
        Dictionary containing both direct values and Promise objects to be resolved

    Returns
    -------
    PromisesResolutionMap
        A new dictionary with all Promise objects resolved to their final values

    Raises
    ------
    CircularPromisesChain
        If a circular reference is detected in the promise resolution chain

    Notes
    -----
    This function implements a depth-first resolution algorithm that follows
    promise chains until concrete values are found. It maintains a resolution
    history to detect and prevent infinite loops from circular dependencies.
    """
    flattened = {}
    resolve_history = {}
    promises = []

    # Initial pass: separate promises from concrete values
    for key, value in mapping.items():
        if isinstance(value, Promise):
            promises.append(key)
            resolve_history[key] = []
        else:
            flattened[key] = value
    if len(promises) == 0:
        return flattened

    # Process promises until all are resolved
    while True:
        i = 0
        while True:
            p = promises[i]
            value = mapping[p]
            if not isinstance(value, Promise):
                # Found concrete value - add to flattened and remove from processing
                flattened[p] = value
                promises.pop(i)
            else:
                # Check for circular reference
                if value._id in resolve_history[p]:
                    _debug_str = f"{resolve_history[p]} -> {value._id}"
                    raise CircularPromisesChain(
                        _("Circular reference detected in promise resolution chain: ")
                        + _debug_str
                    )

                # Track resolution path and follow the reference
                resolve_history[p].append(value._id)
                mapping[p] = mapping[value._id]
                i += 1
            if i >= len(promises):
                break

        if len(promises) == 0:
            break

    return flattened
