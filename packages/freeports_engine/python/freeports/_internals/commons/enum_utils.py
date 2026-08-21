"""Provides basic utils for flags and enum cast and type validation

``flag_from_string``'s expression evaluation (parsing e.g. ``"A | B & ~C"``) now delegates to
``freeports_engine.core.evaluate_flag_expression`` — see
``packages/freeports_engine/src/core/flag_expr.rs`` and
``analysis_finance_reports/agent-memory/rust-rewrite-plan.md``. Everything else in this module
stays Python: it is generic over an arbitrary caller-supplied ``Type[Flag]``/``Type[Enum]`` and
builds Pydantic ``Annotated[...]`` types dynamically, which has no meaningful Rust equivalent.
"""

import ast
import operator
from enum import Enum, Flag
from typing import Type, Any, TypeVar, Annotated, Optional, Union
import logging
import pandas as pd
from pydantic import BeforeValidator
import freeports_engine
from freeports.i18n import _

logger = logging.getLogger(__name__)


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
    if isinstance(expression, list):
        expression = " | ".join(expression)
        return flag_from_string(expression, cls)
    if pd.isna(expression):
        return None
    if isinstance(expression, str):
        if expression.strip() == "":
            return cls(0)
        name_to_bit = {f.name: f.value for f in cls}
        try:
            bits = freeports_engine.core.evaluate_flag_expression(
                expression, name_to_bit
            )
        except ValueError as e:
            raise ValueError(str(e)) from e
        return cls(bits)
    raise ValueError(_("Flags should be specified with list or string expression"))


def _legacy_flag_from_string(
    expression: Optional[Union[str, list]], cls: Type[Flag]
) -> Flag:
    """Dead code: the original AST-based implementation, superseded by the Rust expression
    evaluator imported above. Kept until the migration is far enough along to delete it.
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
        return _legacy_flag_from_string(expression, cls)
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
