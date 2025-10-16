"""Provide basic constant and types used by all submodules,
should facilitate avoiding circular imports
"""

from abc import ABC, abstractmethod
import datetime
import ast
import operator
from enum import Enum, auto, Flag
from typing import Type, TypeAlias, Any, TypeVar, Annotated
import logging as log
import yaml
from importlib_resources import files
import pandas as pd
from pydantic_core import CoreSchema, core_schema
from pydantic import GetCoreSchemaHandler, TypeAdapter, BaseModel, BeforeValidator
from freeports_analysis import data
from freeports_analysis.i18n import _


logger = log.getLogger(__name__)


PROGRAM_DESCRIPTION = _("""Analyze finance reports searching for investing in companies
allegedly involved interantional law violations by third parties
""")


def flag_to_string(flags):
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


def flag_from_string(expression, cls):
    BIN_OPS = {
        ast.BitAnd: operator.and_,
        ast.BitOr: operator.or_,
        ast.BitXor: operator.xor,
    }
    UNARY_OPS = {ast.Invert: operator.invert}

    def _from_ast(node, cls):
        if isinstance(node, ast.Expression):
            return _from_ast(node.body, cls)
        elif isinstance(node, ast.BinOp):
            left = node.left
            right = node.right
            op = type(node.op)
            if op in BIN_OPS:
                return BIN_OPS[op](_from_ast(left, cls), _from_ast(right, cls))
            else:
                raise ValueError(_("Binary operation {} not supported").format(op))
        elif isinstance(node, ast.UnaryOp):
            operand = node.operand
            op = type(node.op)
            if op in UNARY_OPS:
                return UNARY_OPS[op](_from_ast(operand, cls))
            else:
                raise ValueError(_("Unary operation {} not supported").format(op))
        elif isinstance(node, ast.Name):
            name = node.id.upper()
            if hasattr(cls, name):
                return getattr(cls, name)
            else:
                raise ValueError(_("Invalid flag {}").format(name))
        else:
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


def _cast_input_flags(flag_cls, value):
    if isinstance(value, flag_cls):
        return value
    if isinstance(value, Flag):
        value = flag_to_string(value)
    return flag_from_string(value, flag_cls)


def InputFlags(flag_cls: Type[T]) -> type:
    return Annotated[
        flag_cls,
        BeforeValidator(lambda value: _cast_input_flags(flag_cls, value)),
    ]


S = TypeVar("S", bound=Enum)


def _cast_input_enum(enum_cls, value):
    if isinstance(value, enum_cls):
        return value
    if isinstance(value, Enum):
        value = value.name
    return enum_cls[value.strip().upper()]


def InputEnum(enum_cls: Type[S]) -> type:
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
    def symbol(self):
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


PromisesResolutionMap: TypeAlias = dict
PromisesResolutionContext: TypeAlias = dict


class Promise:
    """Base class for deferred value resolution in financial data processing.
    Implements a promise pattern where values can be resolved later from a mapping.
    Attributes
    ----------
    id : str
        The key used to lookup the promised value in the resolution mapping.
    Methods
    -------
    fulfill_with(mapping: PromisesResolutionMap) -> Any
        Resolves the promised value from the given mapping.
    """

    def __init__(self, ID):
        self._id = str(ID)

    def fulfill_with(self, mapping: PromisesResolutionMap) -> Any:
        """Resolve this promise's value from the given mapping.
        Parameters
        ----------
        mapping : PromisesResolutionMap
            Dictionary containing values to resolve promises from.
        Returns
        -------
        Any
            The resolved value from the mapping.

        """
        return mapping[str(self)]

    def __str__(self) -> str:
        return self._id

    def __repr__(self) -> str:
        """str: String representation showing promise class and ID."""
        return f'{self.__class__.__name__}("{str(self)}")'

    def __eq__(self, other) -> bool:
        return self._id == other._id

    def __format__(self, fmt) -> str:
        return repr(self)

    # @classmethod
    # def __get_pydantic_core_schema__(
    #     cls, source_type: Any, handler: GetCoreSchemaHandler
    # ) -> CoreSchema:
    #     return core_schema.no_info_after_validator_function(cls, handler(str))


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
        Dictionary containing both direct values and Promise objects to be resolved.
    Returns
    -------
    PromisesResolutionMap
        A new dictionary with all Promise objects resolved to their final values.
    Raises
    ------
    CircularPromisesChain
        If a circular reference is detected in the promise resolution chain.
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
                if value.id in resolve_history[p]:
                    _debug_str = f"{resolve_history[p]} -> {value.id}"
                    raise CircularPromisesChain(
                        _("Circular reference detected in promise resolution chain: ")
                        + _debug_str
                    )

                # Track resolution path and follow the reference
                resolve_history[p].append(value.id)
                mapping[p] = mapping[value.id]
                i += 1
            if i >= len(promises):
                break

        if len(promises) == 0:
            break

    return flattened
